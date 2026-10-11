//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "async")]

#[allow(dead_code)]
mod common;

use common::memory_channel::MemoryChannel;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use libosdp::{
    Buzzer, BuzzerCommand, CapFunction, Capability, CardRead, Channel, ChannelError, Command,
    CompletionError, ControlPanel, ControlPanelBuilder, Event, PdIndex, PdInfoBuilder,
    PeripheralDevice, Responder,
};
use std::{
    cell::{Cell, RefCell},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Wake, Waker},
    thread::LocalKey,
    time::{Duration, Instant},
};

type Result<T> = core::result::Result<T, libosdp::Error>;

#[rustfmt::skip]
const KEY: [u8; 16] = [
    0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
    0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9,
];

fn pd_info() -> Result<PdInfoBuilder> {
    PdInfoBuilder::new()
        .name("PD 101")?
        .address(101)?
        .baud_rate(115200)
}

fn secure_pair() -> Result<(ControlPanel, PdIndex, PeripheralDevice)> {
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let pd_bus = ReadCounter::new(pd_bus, &PD_READS);
    let pd = PeripheralDevice::new(
        pd_info()?
            .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
            .capability(Capability::new(CapFunction::AudibleOutput, 1, 1))
            .secure_channel_key(KEY),
        Box::new(pd_bus),
    )?;
    let mut builder = ControlPanelBuilder::new(Box::new(cp_bus));
    let target = builder.add_pd(pd_info()?.secure_channel_key(KEY))?;
    let cp = builder.build()?;
    refresh_until(&cp, &pd, || cp.is_online(target) && cp.is_secure(target));
    Ok((cp, target, pd))
}

thread_local! {
    static PD_READS: Cell<usize> = const { Cell::new(0) };
    static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
}

/// Counts the channel reads LibOSDP makes on this thread, which a PD makes
/// on every refresh that runs.
struct ReadCounter {
    inner: MemoryChannel,
    reads: &'static LocalKey<Cell<usize>>,
}

impl ReadCounter {
    fn new(inner: MemoryChannel, reads: &'static LocalKey<Cell<usize>>) -> Self {
        Self { inner, reads }
    }
}

impl Channel for ReadCounter {
    fn read(&mut self, buf: &mut [u8]) -> core::result::Result<usize, ChannelError> {
        self.reads.with(|r| r.set(r.get() + 1));
        self.inner.read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> core::result::Result<(), ChannelError> {
        self.inner.write(buf)
    }
}

/// Tells whether a waker runs outside LibOSDP's callbacks: from inside
/// one, the context refuses to be used again.
struct Probe {
    outside_callback: Box<dyn Fn() -> bool>,
    probing: Cell<bool>,
}

fn arm_probe(outside_callback: impl Fn() -> bool + 'static) {
    PROBE.with(|p| {
        *p.borrow_mut() = Some(Probe {
            outside_callback: Box::new(outside_callback),
            probing: Cell::new(false),
        })
    });
}

fn refresh_both(cp: &ControlPanel, pd: &PeripheralDevice) {
    cp.refresh();
    pd.refresh();
}

fn refresh_until(cp: &ControlPanel, pd: &PeripheralDevice, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "condition never held");
        refresh_both(cp, pd);
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Counts wakes, and wakes that came from inside a LibOSDP callback.
#[derive(Default)]
struct CountingWaker {
    wakes: AtomicUsize,
    wakes_inside_callback: AtomicUsize,
}

impl Wake for CountingWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let verdict = PROBE.with(|p| {
            let p = p.borrow();
            let probe = p.as_ref()?;
            if probe.probing.replace(true) {
                return Some(None);
            }
            let outside = (probe.outside_callback)();
            probe.probing.set(false);
            Some(Some(!outside))
        });
        if verdict == Some(None) {
            return;
        }
        self.wakes.fetch_add(1, Ordering::SeqCst);
        if verdict == Some(Some(true)) {
            self.wakes_inside_callback.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn counting_waker() -> (Arc<CountingWaker>, Waker) {
    let counter = Arc::new(CountingWaker::default());
    (counter.clone(), Waker::from(counter))
}

fn buzz() -> Command {
    Command::Buzzer(BuzzerCommand {
        reader: 0,
        action: Buzzer::Off,
    })
}

fn card() -> Event {
    Event::CardRead(CardRead::new_raw(vec![0x55, 0xAA]))
}

#[test]
fn awaited_command_resolves_ok() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.set_command_handler(|_, _: &mut Responder<'_>| Ok(()));
    let (counter, waker) = counting_waker();
    let mut cx = Context::from_waker(&waker);
    let mut submission = cp.submit(target, buzz())?;
    assert_eq!(Pin::new(&mut submission).poll(&mut cx), Poll::Pending);
    refresh_until(&cp, &pd, || counter.wakes.load(Ordering::SeqCst) > 0);
    assert_eq!(Pin::new(&mut submission).poll(&mut cx), Poll::Ready(Ok(())));
    Ok(())
}

#[test]
fn wakers_run_after_refresh_returns() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    let cp_ptr: *const ControlPanel = &cp;
    arm_probe(move || unsafe { &*cp_ptr }.submit(target, buzz()).is_ok());
    pd.set_command_handler(|_, _: &mut Responder<'_>| Ok(()));
    let (counter, waker) = counting_waker();
    let mut cx = Context::from_waker(&waker);
    let mut submission = cp.submit(target, buzz())?;
    assert_eq!(Pin::new(&mut submission).poll(&mut cx), Poll::Pending);
    refresh_until(&cp, &pd, || counter.wakes.load(Ordering::SeqCst) > 0);
    assert_eq!(counter.wakes.load(Ordering::SeqCst), 1);
    assert_eq!(counter.wakes_inside_callback.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn already_completed_submission_is_ready_at_once() -> Result<()> {
    let (cp, target, _pd) = secure_pair()?;
    let (counter, waker) = counting_waker();
    let mut cx = Context::from_waker(&waker);
    let mut submission = cp.submit(target, buzz())?;
    assert_eq!(cp.flush(target), 1);
    assert_eq!(
        Pin::new(&mut submission).poll(&mut cx),
        Poll::Ready(Err(CompletionError::Flushed))
    );
    assert_eq!(counter.wakes.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn flush_wakes_the_awaiting_task() -> Result<()> {
    let (cp, target, _pd) = secure_pair()?;
    let (counter, waker) = counting_waker();
    let mut cx = Context::from_waker(&waker);
    let mut submission = cp.submit(target, buzz())?;
    assert_eq!(Pin::new(&mut submission).poll(&mut cx), Poll::Pending);
    assert_eq!(cp.flush(target), 1);
    assert_eq!(counter.wakes.load(Ordering::SeqCst), 1);
    assert_eq!(
        Pin::new(&mut submission).poll(&mut cx),
        Poll::Ready(Err(CompletionError::Flushed))
    );
    Ok(())
}

#[test]
fn flush_inside_a_callback_wakes_after_refresh_returns() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    let pd_addr = &pd as *const PeripheralDevice as usize;
    let (counter, waker) = counting_waker();
    let not_handled = usize::MAX;
    let wakes_inside_handler = Arc::new(AtomicUsize::new(not_handled));
    let (seen, handler_counter) = (wakes_inside_handler.clone(), counter.clone());
    pd.set_command_handler(move |_, _: &mut Responder<'_>| {
        if seen.load(Ordering::SeqCst) == not_handled {
            let pd = unsafe { &*(pd_addr as *const PeripheralDevice) };
            if let Ok(mut submission) = pd.submit(card()) {
                let _ = Pin::new(&mut submission).poll(&mut Context::from_waker(&waker));
                pd.flush();
            }
            let wakes = handler_counter.wakes.load(Ordering::SeqCst);
            seen.store(wakes, Ordering::SeqCst);
        }
        Ok(())
    });
    cp.submit(target, buzz())?;
    refresh_until(&cp, &pd, || {
        wakes_inside_handler.load(Ordering::SeqCst) != not_handled
    });
    assert_eq!(wakes_inside_handler.load(Ordering::SeqCst), 0);
    assert_eq!(counter.wakes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn awaited_pd_event_resolves_ok() -> Result<()> {
    let (cp, _target, pd) = secure_pair()?;
    let pd_ptr: *const PeripheralDevice = &pd;
    arm_probe(move || {
        let before = PD_READS.with(|r| r.get());
        unsafe { &*pd_ptr }.refresh();
        PD_READS.with(|r| r.get()) != before
    });
    let (counter, waker) = counting_waker();
    let mut cx = Context::from_waker(&waker);
    let mut submission = pd.submit(card())?;
    assert_eq!(Pin::new(&mut submission).poll(&mut cx), Poll::Pending);
    refresh_until(&cp, &pd, || counter.wakes.load(Ordering::SeqCst) > 0);
    assert_eq!(counter.wakes_inside_callback.load(Ordering::SeqCst), 0);
    assert_eq!(Pin::new(&mut submission).poll(&mut cx), Poll::Ready(Ok(())));
    Ok(())
}

#[test]
fn dropping_a_pending_future_leaves_the_submission_running() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.set_command_handler(|_, _: &mut Responder<'_>| Ok(()));
    let (_counter, waker) = counting_waker();
    let mut cx = Context::from_waker(&waker);
    let mut submission = cp.submit(target, buzz())?;
    assert_eq!(Pin::new(&mut submission).poll(&mut cx), Poll::Pending);
    drop(submission);
    let second = cp.submit(target, buzz())?;
    refresh_until(&cp, &pd, || second.status().is_some());
    assert_eq!(second.status(), Some(Ok(())));
    Ok(())
}
