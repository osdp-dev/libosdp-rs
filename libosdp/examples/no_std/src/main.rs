//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

//! A CP and a PD on one Cortex-M4, talking over an in-memory loopback with
//! secure channel enforced. Exits through semihosting with success once a
//! command has reached the PD and an event has reached the CP.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::{boxed::Box, collections::VecDeque, vec};
use core::{
    cell::{Cell, RefCell},
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use cortex_m::peripheral::syst::SystClkSource;
use cortex_m_rt::{entry, exception};
use cortex_m_semihosting::{debug, hprintln};
use critical_section::Mutex;
use embedded_alloc::LlffHeap as Heap;
use libosdp::{
    Buzzer, BuzzerCommand, CapFunction, Capability, CardRead, Channel, ChannelError, Command,
    ControlPanelBuilder, CpEvent, Event, Flag, PdInfoBuilder, PeripheralDevice, Platform,
    Responder,
};

/// MPS2-AN386 core clock as QEMU models it.
const CORE_CLOCK_HZ: u32 = 25_000_000;
const HEAP_SIZE: usize = 64 * 1024;
const TIMEOUT_MS: u64 = 10_000;

#[rustfmt::skip]
const SC_KEY: [u8; 16] = [
    0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
    0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9,
];

#[global_allocator]
static HEAP: Heap = Heap::empty();

static MILLIS: Mutex<Cell<u64>> = Mutex::new(Cell::new(0));

#[exception]
fn SysTick() {
    critical_section::with(|cs| {
        let ms = MILLIS.borrow(cs);
        ms.set(ms.get() + 1);
    });
}

fn millis() -> u64 {
    critical_section::with(|cs| MILLIS.borrow(cs).get())
}

static RNG_STATE: Mutex<Cell<u32>> = Mutex::new(Cell::new(0x2545_f491));

/// QEMU's MPS2 has no TRNG, so xorshift keeps the demo self-contained. Secure
/// channel challenges are drawn from this: a real product must read a
/// hardware TRNG here instead.
fn random() -> u32 {
    critical_section::with(|cs| {
        let state = RNG_STATE.borrow(cs);
        let mut x = state.get() ^ millis() as u32;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        state.set(x);
        x
    })
}

static PLATFORM: Platform = Platform { millis, random };

/// newlib-nano's snprintf() shares its output code with asprintf(), which
/// grows a buffer it allocated itself, so the link wants malloc() and the
/// _sbrk() under it even though LibOSDP's fixed buffers never get there.
/// Refusing every request keeps a second heap from ever growing.
#[no_mangle]
extern "C" fn _sbrk(_increment: isize) -> *mut core::ffi::c_void {
    usize::MAX as *mut core::ffi::c_void
}

/// One direction of the simulated RS-485 bus.
type Wire = Mutex<RefCell<VecDeque<u8>>>;

static CP_TO_PD: Wire = Mutex::new(RefCell::new(VecDeque::new()));
static PD_TO_CP: Wire = Mutex::new(RefCell::new(VecDeque::new()));

struct Loopback {
    rx: &'static Wire,
    tx: &'static Wire,
}

impl Channel for Loopback {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ChannelError> {
        critical_section::with(|cs| {
            let mut rx = self.rx.borrow_ref_mut(cs);
            let n = buf.len().min(rx.len());
            for (dst, src) in buf.iter_mut().zip(rx.drain(..n)) {
                *dst = src;
            }
            Ok(n)
        })
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), ChannelError> {
        critical_section::with(|cs| self.tx.borrow_ref_mut(cs).extend(buf));
        Ok(())
    }

    fn flush(&mut self) {
        critical_section::with(|cs| self.rx.borrow_ref_mut(cs).clear());
    }
}

fn pd_info() -> PdInfoBuilder {
    PdInfoBuilder::new()
        .name("PD 101")
        .unwrap()
        .address(101)
        .unwrap()
        .baud_rate(115200)
        .unwrap()
        .flag(Flag::EnforceSecure)
        .secure_channel_key(SC_KEY)
}

fn fail(what: &str) -> ! {
    hprintln!("FAIL: {}", what);
    debug::exit(debug::EXIT_FAILURE);
    loop {}
}

#[entry]
fn main() -> ! {
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }

    let mut core = cortex_m::Peripherals::take().unwrap();
    core.SYST.set_clock_source(SystClkSource::Core);
    core.SYST.set_reload(CORE_CLOCK_HZ / 1000 - 1);
    core.SYST.clear_current();
    core.SYST.enable_interrupt();
    core.SYST.enable_counter();

    libosdp::set_platform(&PLATFORM);

    let pd = PeripheralDevice::new(
        pd_info()
            .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
            .capability(Capability::new(CapFunction::AudibleOutput, 1, 1)),
        Box::new(Loopback {
            rx: &CP_TO_PD,
            tx: &PD_TO_CP,
        }),
    )
    .unwrap_or_else(|_| fail("PD setup"));

    static COMMAND_SEEN: AtomicBool = AtomicBool::new(false);
    pd.set_command_handler(|command, _: &mut Responder<'_>| {
        hprintln!("PD: received {:?}", command);
        COMMAND_SEEN.store(true, Ordering::Release);
        Ok(())
    });

    let mut builder = ControlPanelBuilder::new(Box::new(Loopback {
        rx: &PD_TO_CP,
        tx: &CP_TO_PD,
    }));
    let pd_index = builder
        .add_pd(pd_info())
        .unwrap_or_else(|_| fail("CP add_pd"));
    let cp = builder.build().unwrap_or_else(|_| fail("CP setup"));

    let started = millis();
    let card_read_seen = Cell::new(false);
    let refresh_both = || {
        if millis() - started > TIMEOUT_MS {
            fail("timed out");
        }
        cp.refresh();
        pd.refresh();
        while let Some((_, event)) = cp.next_event() {
            hprintln!("CP: received {:?}", event);
            if let CpEvent::Event(Event::CardRead(_)) = event {
                card_read_seen.set(true);
            }
        }
        cortex_m::asm::delay(CORE_CLOCK_HZ / 1000);
    };

    while !cp.is_secure(pd_index) {
        refresh_both();
    }
    hprintln!("secure channel up after {} ms", millis() - started);

    let buzz = Command::Buzzer(BuzzerCommand {
        reader: 0,
        action: Buzzer::Tone {
            on: Duration::from_millis(500),
            off: Duration::from_millis(200),
            repeat: 3,
        },
    });
    let command = cp
        .submit(pd_index, buzz)
        .unwrap_or_else(|_| fail("CP refused the command"));
    let event = pd
        .submit(Event::CardRead(CardRead::new_raw(vec![0x55, 0xaa])))
        .unwrap_or_else(|_| fail("PD refused the event"));

    // The PD completes its event once the reply carrying it is sent; the CP
    // reads that reply on its next refresh.
    while command.status().is_none() || event.status().is_none() || !card_read_seen.get() {
        refresh_both();
    }
    if command.status() != Some(Ok(())) || !COMMAND_SEEN.load(Ordering::Acquire) {
        fail("command did not complete");
    }
    if event.status() != Some(Ok(())) {
        fail("event did not complete");
    }

    hprintln!("PASS: CP and PD exchanged a command and an event over SC");
    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    hprintln!("{}", info);
    debug::exit(debug::EXIT_FAILURE);
    loop {}
}
