//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

#[allow(dead_code)]
mod common;

use common::memory_channel::MemoryChannel;
use libosdp::{
    Channel, ChannelError, ControlPanel, ControlPanelBuilder, OsdpCommand, OsdpCommandBuzzer,
    OsdpError, OsdpFileOps, PdCapEntity, PdCapability, PdInfoBuilder, PeripheralDevice,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

type Result<T> = core::result::Result<T, OsdpError>;

#[derive(Clone, Default)]
struct DropCounter(Arc<AtomicUsize>);

impl DropCounter {
    fn guard(&self) -> DropGuard {
        DropGuard(self.0.clone())
    }
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

struct DropGuard(Arc<AtomicUsize>);

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct IdleChannel(#[allow(dead_code)] DropGuard);

impl Channel for IdleChannel {
    fn get_id(&self) -> i32 {
        0
    }
    fn read(&mut self, _: &mut [u8]) -> core::result::Result<usize, ChannelError> {
        Err(ChannelError::WouldBlock)
    }
    fn write(&mut self, buf: &[u8]) -> core::result::Result<usize, ChannelError> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> core::result::Result<(), ChannelError> {
        Ok(())
    }
    fn close(&mut self) -> core::result::Result<(), ChannelError> {
        Ok(())
    }
}

struct NoFiles(#[allow(dead_code)] DropGuard);

impl OsdpFileOps for NoFiles {
    fn open(&mut self, _: i32, _: bool) -> Result<usize> {
        Err(OsdpError::FileTransfer("none"))
    }
    fn offset_read(&self, _: &mut [u8], _: u64) -> Result<usize> {
        Ok(0)
    }
    fn offset_write(&self, _: &[u8], _: u64) -> Result<usize> {
        Ok(0)
    }
    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

fn pd_info() -> Result<PdInfoBuilder> {
    PdInfoBuilder::new().name("PD 101")?.address(101)?.baud_rate(115200)
}

#[test]
fn cp_drop_frees_channel_callback_and_file_ops() -> Result<()> {
    let drops = DropCounter::default();
    let mut cp = ControlPanelBuilder::new(Box::new(IdleChannel(drops.guard())))
        .add_pd(pd_info()?)
        .build()?;
    let guard = drops.guard();
    cp.set_event_callback(move |_, _| {
        let _ = &guard;
        0
    });
    cp.register_file_ops(0, Box::new(NoFiles(drops.guard())))?;
    assert_eq!(drops.count(), 0);
    drop(cp);
    assert_eq!(drops.count(), 3);
    Ok(())
}

#[test]
fn cp_replacing_callback_frees_the_old_one() -> Result<()> {
    let drops = DropCounter::default();
    let mut cp = ControlPanelBuilder::new(Box::new(IdleChannel(drops.guard())))
        .add_pd(pd_info()?)
        .build()?;
    let first = drops.guard();
    cp.set_event_callback(move |_, _| {
        let _ = &first;
        0
    });
    cp.set_event_callback(|_, _| 0);
    assert_eq!(drops.count(), 1);
    drop(cp);
    assert_eq!(drops.count(), 2);
    Ok(())
}

#[test]
fn pd_drop_frees_channel_callback_and_file_ops() -> Result<()> {
    let drops = DropCounter::default();
    let mut pd = PeripheralDevice::new(pd_info()?, Box::new(IdleChannel(drops.guard())))?;
    let guard = drops.guard();
    pd.set_command_callback(move |_| {
        let _ = &guard;
        0
    });
    pd.register_file_ops(Box::new(NoFiles(drops.guard())))?;
    assert_eq!(drops.count(), 0);
    drop(pd);
    assert_eq!(drops.count(), 3);
    Ok(())
}

#[rustfmt::skip]
const KEY: [u8; 16] = [
    0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
    0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9,
];

fn secure_pair() -> Result<(ControlPanel, PeripheralDevice)> {
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let mut pd = PeripheralDevice::new(
        pd_info()?
            .capability(PdCapability::CommunicationSecurity(PdCapEntity::new(1, 1)))
            .capability(PdCapability::AudibleOutput(PdCapEntity::new(1, 1)))
            .capability(PdCapability::LedControl(PdCapEntity::new(1, 1)))
            .secure_channel_key(KEY),
        Box::new(pd_bus),
    )?;
    let mut cp = ControlPanelBuilder::new(Box::new(cp_bus))
        .add_pd(pd_info()?.secure_channel_key(KEY))
        .build()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(cp.is_online(0) && cp.is_sc_active(0)) {
        assert!(Instant::now() < deadline, "secure channel never came up");
        cp.refresh();
        pd.refresh();
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok((cp, pd))
}

#[test]
fn cp_drop_with_commands_queued_frees_them_via_aborted() -> Result<()> {
    let (mut cp, pd) = secure_pair()?;
    let buzz = OsdpCommand::Buzzer(OsdpCommandBuzzer {
        reader: 0,
        control_code: 1,
        on_count: 0,
        off_count: 0,
        rep_count: 0,
    });
    for _ in 0..8 {
        cp.send_command(0, buzz.clone())?;
    }
    drop(cp);
    drop(pd);
    Ok(())
}

#[test]
fn pd_capabilities_survive_setup() -> Result<()> {
    let (cp, _pd) = secure_pair()?;
    let cap = cp.get_capability(0, PdCapability::LedControl(PdCapEntity::new(0, 0)))?;
    assert_eq!(cap, PdCapability::LedControl(PdCapEntity::new(1, 1)));
    Ok(())
}
