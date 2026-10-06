//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

#[allow(dead_code)]
mod common;

use common::memory_channel::MemoryChannel;
use libosdp::{
    Buzzer, BuzzerCommand, CapFunction, Capability, CardRead, Channel, ChannelError, Command,
    CompletionError, ControlPanel, ControlPanelBuilder, CpEvent, Error, Event, FileError, FileOps,
    FileTxCommand, Flag, KeyPress, MfgCommand, MfgReply, Nak, Notification, OpenMode, PdId,
    PdIndex, PdInfoBuilder,
    PeripheralDevice, ReaderStatus, Responder, Status, StatusKind, Submission,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::cell::{Cell, OnceCell};
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

type Result<T> = core::result::Result<T, Error>;

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
    fn read(&mut self, _: &mut [u8]) -> core::result::Result<usize, ChannelError> {
        Err(ChannelError::WouldBlock)
    }
    fn write(&mut self, _: &[u8]) -> core::result::Result<(), ChannelError> {
        Ok(())
    }
}

struct NoFiles(#[allow(dead_code)] DropGuard);

impl FileOps for NoFiles {
    fn open(&mut self, _: i32, _: OpenMode) -> core::result::Result<u32, FileError> {
        Err(FileError::NotFound)
    }
    fn read(&mut self, _: &mut [u8], _: u32) -> core::result::Result<usize, FileError> {
        Ok(0)
    }
    fn write(&mut self, _: &[u8], _: u32) -> core::result::Result<usize, FileError> {
        Ok(0)
    }
    fn close(&mut self) -> core::result::Result<(), FileError> {
        Ok(())
    }
}

fn pd_info() -> Result<PdInfoBuilder> {
    PdInfoBuilder::new().name("PD 101")?.address(101)?.baud_rate(115200)
}

#[derive(Clone, Default)]
struct Lifecycle(Arc<Mutex<Vec<&'static str>>>);

impl Lifecycle {
    fn record(&self, step: &'static str) {
        self.0.lock().unwrap().push(step);
    }
    fn steps(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().clone()
    }
}

struct RecordingChannel(Lifecycle);

impl Channel for RecordingChannel {
    fn read(&mut self, _: &mut [u8]) -> core::result::Result<usize, ChannelError> {
        Err(ChannelError::WouldBlock)
    }
    fn write(&mut self, _: &[u8]) -> core::result::Result<(), ChannelError> {
        Ok(())
    }
    fn close(&mut self) {
        self.0.record("close");
    }
}

impl Drop for RecordingChannel {
    fn drop(&mut self) {
        self.0.record("drop");
    }
}

#[test]
fn channel_is_closed_once_before_it_is_freed() -> Result<()> {
    let cp_log = Lifecycle::default();
    let mut builder = ControlPanelBuilder::new(Box::new(RecordingChannel(cp_log.clone())));
    builder.add_pd(pd_info()?)?;
    builder.add_pd(PdInfoBuilder::new().address(102)?.baud_rate(115200)?)?;
    drop(builder.build()?);
    assert_eq!(cp_log.steps(), ["close", "drop"]);

    let pd_log = Lifecycle::default();
    let pd = PeripheralDevice::new(pd_info()?, Box::new(RecordingChannel(pd_log.clone())))?;
    drop(pd);
    assert_eq!(pd_log.steps(), ["close", "drop"]);
    Ok(())
}

#[test]
fn cp_drop_frees_channel_and_file_ops() -> Result<()> {
    let drops = DropCounter::default();
    let mut builder = ControlPanelBuilder::new(Box::new(IdleChannel(drops.guard())));
    let pd = builder.add_pd(pd_info()?)?;
    let cp = builder.build()?;
    cp.register_file_ops(pd, Box::new(NoFiles(drops.guard())))?;
    assert_eq!(drops.count(), 0);
    drop(cp);
    assert_eq!(drops.count(), 2);
    Ok(())
}

#[test]
fn replacing_file_ops_frees_the_old_ones() -> Result<()> {
    let drops = DropCounter::default();
    let mut builder = ControlPanelBuilder::new(Box::new(IdleChannel(drops.guard())));
    let target = builder.add_pd(pd_info()?)?;
    let cp = builder.build()?;
    cp.register_file_ops(target, Box::new(NoFiles(drops.guard())))?;
    cp.register_file_ops(target, Box::new(NoFiles(drops.guard())))?;
    assert_eq!(drops.count(), 1);
    let pd = PeripheralDevice::new(pd_info()?, Box::new(IdleChannel(drops.guard())))?;
    pd.register_file_ops(Box::new(NoFiles(drops.guard())))?;
    pd.register_file_ops(Box::new(NoFiles(drops.guard())))?;
    assert_eq!(drops.count(), 2);
    drop(cp);
    drop(pd);
    assert_eq!(drops.count(), 6);
    Ok(())
}

#[test]
fn pd_drop_frees_channel_handler_and_file_ops() -> Result<()> {
    let drops = DropCounter::default();
    let pd = PeripheralDevice::new(pd_info()?, Box::new(IdleChannel(drops.guard())))?;
    let guard = drops.guard();
    pd.set_command_handler(move |_, _: &mut Responder<'_>| {
        let _ = &guard;
        Ok(())
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

fn secure_pair() -> Result<(ControlPanel, PdIndex, PeripheralDevice)> {
    secure_pair_with_event_queue(32)
}

fn secure_pair_with_event_queue(
    capacity: usize,
) -> Result<(ControlPanel, PdIndex, PeripheralDevice)> {
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let pd = PeripheralDevice::new(
        pd_info()?
            .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
            .capability(Capability::new(CapFunction::AudibleOutput, 1, 1))
            .capability(Capability::new(CapFunction::LedControl, 1, 1))
            .secure_channel_key(KEY),
        Box::new(pd_bus),
    )?;
    let mut builder = ControlPanelBuilder::new(Box::new(cp_bus))
        .event_queue_capacity(NonZeroUsize::new(capacity).unwrap());
    let target = builder.add_pd(pd_info()?.secure_channel_key(KEY))?;
    let cp = builder.build()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(cp.is_online(target) && cp.is_secure(target)) {
        assert!(Instant::now() < deadline, "secure channel never came up");
        cp.refresh();
        pd.refresh();
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok((cp, target, pd))
}

fn notifying_pair() -> Result<(ControlPanel, PdIndex, PeripheralDevice)> {
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let pd = PeripheralDevice::new(
        pd_info()?
            .flag(Flag::EnableNotification)
            .id(&PdId::from_number(7))
            .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
            .secure_channel_key(KEY),
        Box::new(pd_bus),
    )?;
    let mut builder = ControlPanelBuilder::new(Box::new(cp_bus));
    let target = builder.add_pd(
        pd_info()?
            .flag(Flag::EnableNotification)
            .secure_channel_key(KEY),
    )?;
    let cp = builder.build()?;
    Ok((cp, target, pd))
}

#[test]
fn link_changes_are_reported_as_notifications() -> Result<()> {
    let (cp, target, pd) = notifying_pair()?;
    let commands = Arc::new(AtomicUsize::new(0));
    let seen = commands.clone();
    pd.set_command_handler(move |_, _: &mut Responder<'_>| {
        seen.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    let (mut on_cp, mut on_pd) = (Vec::new(), Vec::new());
    let secure = Notification::ScStatus {
        active: true,
        scbk_d: false,
    };
    refresh_until(&cp, &pd, || {
        while let Some((pd, item)) = cp.next_event() {
            assert_eq!(pd, target);
            if let CpEvent::Notification(notification) = item {
                on_cp.push(notification);
            }
        }
        while let Some(notification) = pd.next_notification() {
            on_pd.push(notification);
        }
        on_cp.contains(&secure) && on_pd.contains(&secure)
    });
    assert!(on_cp.contains(&Notification::PdStatus { online: true }));
    assert!(on_cp.contains(&Notification::PdId(PdId::from_number(7))));
    assert!(on_pd.contains(&Notification::PdStatus { online: true }));
    assert_eq!(commands.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn cp_drop_with_commands_queued_frees_them_via_aborted() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    let buzz = Command::Buzzer(BuzzerCommand {
        reader: 0,
        action: Buzzer::Off,
    });
    for _ in 0..8 {
        cp.submit(target, buzz.clone())?;
    }
    drop(cp);
    drop(pd);
    Ok(())
}

#[test]
fn pd_capabilities_survive_setup() -> Result<()> {
    let (cp, target, _pd) = secure_pair()?;
    assert_eq!(
        cp.capability(target, CapFunction::LedControl),
        Some(Capability::new(CapFunction::LedControl, 1, 1))
    );
    assert_eq!(cp.capability(target, CapFunction::Biometrics), None);
    assert_eq!(cp.pd_id(target), Some(PdId::default()));
    Ok(())
}

#[test]
fn capabilities_set_after_setup_reach_the_cp() -> Result<()> {
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let pd = PeripheralDevice::new(pd_info()?, Box::new(pd_bus))?;
    pd.set_capabilities(&[Capability::new(CapFunction::LedControl, 1, 4)]);
    let mut builder = ControlPanelBuilder::new(Box::new(cp_bus));
    let target = builder.add_pd(pd_info()?)?;
    let cp = builder.build()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cp.is_online(target) {
        assert!(Instant::now() < deadline, "PD never came online");
        cp.refresh();
        pd.refresh();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        cp.capability(target, CapFunction::LedControl),
        Some(Capability::new(CapFunction::LedControl, 1, 4))
    );
    Ok(())
}

thread_local! {
    static CP: OnceCell<ControlPanel> = const { OnceCell::new() };
    static PD: OnceCell<PeripheralDevice> = const { OnceCell::new() };
    static NESTED_REFRESHES: Cell<usize> = const { Cell::new(0) };
}

struct RefreshesItsOwnCp;

impl Channel for RefreshesItsOwnCp {
    fn read(&mut self, _: &mut [u8]) -> core::result::Result<usize, ChannelError> {
        CP.with(|cp| {
            if let Some(cp) = cp.get() {
                NESTED_REFRESHES.set(NESTED_REFRESHES.get() + 1);
                cp.refresh();
            }
        });
        Err(ChannelError::WouldBlock)
    }
    fn write(&mut self, _: &[u8]) -> core::result::Result<(), ChannelError> {
        Ok(())
    }
}

#[test]
fn refresh_from_inside_a_callback_does_nothing() -> Result<()> {
    let mut builder = ControlPanelBuilder::new(Box::new(RefreshesItsOwnCp));
    builder.add_pd(pd_info()?)?;
    CP.with(|cp| cp.set(builder.build().unwrap()).unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    while NESTED_REFRESHES.get() == 0 {
        assert!(Instant::now() < deadline, "CP never read its channel");
        CP.with(|cp| cp.get().unwrap().refresh());
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

#[test]
fn pd_handler_replaced_from_inside_itself_is_freed_after_it_returns() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    let drops = DropCounter::default();
    let first = drops.guard();
    let first_calls = Arc::new(AtomicUsize::new(0));
    let second_calls = Arc::new(AtomicUsize::new(0));
    let (seen_first, seen_second) = (first_calls.clone(), second_calls.clone());
    pd.set_command_handler(move |_, _: &mut Responder<'_>| {
        let seen_second = seen_second.clone();
        PD.with(|pd| {
            pd.get()
                .unwrap()
                .set_command_handler(move |_, _: &mut Responder<'_>| {
                    seen_second.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
        });
        let _ = &first;
        seen_first.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    PD.with(|cell| cell.set(pd).unwrap());
    let buzz = Command::Buzzer(BuzzerCommand {
        reader: 0,
        action: Buzzer::Off,
    });
    for _ in 0..2 {
        cp.submit(target, buzz.clone())?;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while second_calls.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline, "second command never arrived");
        cp.refresh();
        PD.with(|pd| pd.get().unwrap().refresh());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(drops.count(), 1);
    Ok(())
}

thread_local! {
    static NESTED_SEND: std::cell::RefCell<Option<Result<()>>> =
        const { std::cell::RefCell::new(None) };
    static NESTED_REGISTRATION: std::cell::RefCell<Option<Result<()>>> =
        const { std::cell::RefCell::new(None) };
}

struct RegistersFileOpsFromOpen {
    _guard: DropGuard,
    ballast: Vec<u8>,
    target: PdIndex,
}

impl FileOps for RegistersFileOpsFromOpen {
    fn open(&mut self, _: i32, _: OpenMode) -> core::result::Result<u32, FileError> {
        let replacement = Box::new(NoFiles(DropCounter::default().guard()));
        let result = CP.with(|cp| cp.get().unwrap().register_file_ops(self.target, replacement));
        NESTED_REGISTRATION.with(|slot| *slot.borrow_mut() = Some(result));
        let nested_send = CP.with(|cp| {
            cp.get()
                .unwrap()
                .submit(self.target, Command::FileTx(FileTxCommand { id: 1, flags: 0 }))
                .map(|_| ())
        });
        NESTED_SEND.with(|slot| *slot.borrow_mut() = Some(nested_send));
        self.ballast.fill(1);
        Err(FileError::NotFound)
    }
    fn read(&mut self, _: &mut [u8], _: u32) -> core::result::Result<usize, FileError> {
        Ok(0)
    }
    fn write(&mut self, _: &[u8], _: u32) -> core::result::Result<usize, FileError> {
        Ok(0)
    }
    fn close(&mut self) -> core::result::Result<(), FileError> {
        Ok(())
    }
}

#[test]
fn register_file_ops_from_open_during_submit_is_rejected() -> Result<()> {
    let (cp, target, _pd) = secure_pair()?;
    let drops = DropCounter::default();
    cp.register_file_ops(
        target,
        Box::new(RegistersFileOpsFromOpen {
            _guard: drops.guard(),
            ballast: vec![0; 4096],
            target,
        }),
    )?;
    CP.with(|slot| slot.set(cp).unwrap());
    let sent = CP.with(|cp| {
        cp.get()
            .unwrap()
            .submit(target, Command::FileTx(FileTxCommand { id: 1, flags: 0 }))
            .map(|_| ())
    });
    assert!(sent.is_err());
    assert_eq!(
        NESTED_REGISTRATION.with(|slot| slot.borrow_mut().take()),
        Some(Err(Error::Rejected))
    );
    assert_eq!(
        NESTED_SEND.with(|slot| slot.borrow_mut().take()),
        Some(Err(Error::Rejected))
    );
    assert_eq!(drops.count(), 0);
    Ok(())
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

fn refresh_until_done(
    cp: &ControlPanel,
    pd: &PeripheralDevice,
    submission: &Submission<'_>,
) -> core::result::Result<(), CompletionError> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = submission.status() {
            return status;
        }
        assert!(Instant::now() < deadline, "submission never completed");
        cp.refresh();
        pd.refresh();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn command_the_pd_acks_completes_ok() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.set_command_handler(|_, _: &mut Responder<'_>| Ok(()));
    let submission = cp.submit(target, buzz())?;
    assert_eq!(submission.status(), None);
    assert_eq!(refresh_until_done(&cp, &pd, &submission), Ok(()));
    Ok(())
}

#[test]
fn command_the_pd_naks_completes_failed() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.set_command_handler(|_, _: &mut Responder<'_>| Err(Nak::Record));
    let submission = cp.submit(target, buzz())?;
    assert_eq!(
        refresh_until_done(&cp, &pd, &submission),
        Err(CompletionError::Failed)
    );
    Ok(())
}

#[test]
fn flushed_commands_complete_flushed() -> Result<()> {
    let (cp, target, _pd) = secure_pair()?;
    let first = cp.submit(target, buzz())?;
    let second = cp.submit(target, buzz())?;
    assert_eq!(cp.flush(target), 2);
    assert_eq!(first.status(), Some(Err(CompletionError::Flushed)));
    assert_eq!(second.status(), Some(Err(CompletionError::Flushed)));
    Ok(())
}

#[test]
fn event_the_cp_takes_completes_ok() -> Result<()> {
    let (cp, _target, pd) = secure_pair()?;
    let submission = pd.submit(card())?;
    assert_eq!(refresh_until_done(&cp, &pd, &submission), Ok(()));
    Ok(())
}

#[test]
fn flushed_events_complete_flushed() -> Result<()> {
    let (_cp, _target, pd) = secure_pair()?;
    let first = pd.submit(card())?;
    let second = pd.submit(card())?;
    assert_eq!(pd.flush(), 2);
    assert_eq!(first.status(), Some(Err(CompletionError::Flushed)));
    assert_eq!(second.status(), Some(Err(CompletionError::Flushed)));
    Ok(())
}

#[test]
fn pd_drop_with_events_queued_frees_them_via_aborted() -> Result<()> {
    let (cp, _target, pd) = secure_pair()?;
    for _ in 0..8 {
        pd.submit(card())?;
    }
    drop(pd);
    drop(cp);
    Ok(())
}

fn refresh_until(cp: &ControlPanel, pd: &PeripheralDevice, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "condition never held");
        cp.refresh();
        pd.refresh();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn cp_hands_over_pd_events_after_refresh() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.submit(card())?;
    let mut received = Vec::new();
    refresh_until(&cp, &pd, || {
        while let Some(item) = cp.next_event() {
            cp.submit(target, buzz()).unwrap();
            received.push(item);
        }
        !received.is_empty()
    });
    assert_eq!(received, vec![(target, CpEvent::Event(card()))]);
    Ok(())
}

#[test]
fn full_event_queue_drops_the_newest_events() -> Result<()> {
    let (cp, target, pd) = secure_pair_with_event_queue(2)?;
    for key in 1..=3u8 {
        pd.submit(Event::KeyPress(KeyPress::new(vec![key])))?;
    }
    refresh_until(&cp, &pd, || cp.dropped_events() == 1);
    for key in 1..=2u8 {
        let expected = CpEvent::Event(Event::KeyPress(KeyPress::new(vec![key])));
        assert_eq!(cp.next_event(), Some((target, expected)));
    }
    assert_eq!(cp.next_event(), None);
    Ok(())
}

#[test]
fn handler_answers_a_manufacturer_command_inline() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.set_command_handler(|cmd, reply: &mut Responder<'_>| match cmd {
        Command::Mfg(mfg) => {
            let echo = MfgReply {
                vendor_code: mfg.vendor_code,
                data: mfg.data,
            };
            reply.reply(Event::MfgReply(echo)).map_err(|_| Nak::Record)
        }
        _ => Ok(()),
    });
    let mfg = MfgCommand {
        vendor_code: (0x0A, 0x0B, 0x0C),
        data: vec![1, 2, 3],
    };
    let submission = cp.submit(target, Command::Mfg(mfg))?;
    assert_eq!(refresh_until_done(&cp, &pd, &submission), Ok(()));
    let expected = MfgReply {
        vendor_code: (0x0A, 0x0B, 0x0C),
        data: vec![1, 2, 3],
    };
    let mut received = None;
    refresh_until(&cp, &pd, || {
        received = cp.next_event();
        received.is_some()
    });
    assert_eq!(
        received,
        Some((target, CpEvent::Event(Event::MfgReply(expected))))
    );
    Ok(())
}

fn local_status(tamper: bool, power: bool) -> Status {
    Status::Local { tamper, power }
}

#[test]
fn handler_answers_a_status_query_inline() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.set_command_handler(|cmd, reply: &mut Responder<'_>| match cmd {
        Command::Status(_) => reply
            .reply(Event::Status(local_status(true, false)))
            .map_err(|_| Nak::Record),
        _ => Ok(()),
    });
    let submission = cp.submit(target, Command::Status(StatusKind::Local))?;
    assert_eq!(refresh_until_done(&cp, &pd, &submission), Ok(()));
    let mut received = None;
    refresh_until(&cp, &pd, || {
        received = cp.next_event();
        received.is_some()
    });
    assert_eq!(
        received,
        Some((
            target,
            CpEvent::Event(Event::Status(local_status(true, false)))
        ))
    );
    Ok(())
}

#[test]
fn status_reply_of_another_kind_is_refused() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    let replied = Arc::new(Mutex::new(None));
    let seen = replied.clone();
    pd.set_command_handler(move |_, reply: &mut Responder<'_>| {
        let status = Status::Input(vec![]);
        *seen.lock().unwrap() = Some(reply.reply(Event::Status(status)));
        Err(Nak::Record)
    });
    let submission = cp.submit(target, Command::Status(StatusKind::Local))?;
    assert_eq!(
        refresh_until_done(&cp, &pd, &submission),
        Err(CompletionError::Failed)
    );
    assert!(matches!(
        *replied.lock().unwrap(),
        Some(Err(Error::InvalidArgument(_)))
    ));
    Ok(())
}

#[test]
fn status_with_an_entry_per_reader_the_pd_lacks_is_naked() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.set_command_handler(|cmd, reply: &mut Responder<'_>| match cmd {
        Command::Status(StatusKind::Reader) => reply
            .reply(Event::Status(Status::Reader(vec![ReaderStatus::Normal])))
            .map_err(|_| Nak::Record),
        _ => Ok(()),
    });
    let submission = cp.submit(target, Command::Status(StatusKind::Reader))?;
    assert_eq!(
        refresh_until_done(&cp, &pd, &submission),
        Err(CompletionError::Failed)
    );
    Ok(())
}

fn oversized_mfg_reply() -> Event {
    Event::MfgReply(MfgReply {
        vendor_code: (0x0A, 0x0B, 0x0C),
        data: vec![0; 129],
    })
}

#[test]
fn oversized_event_is_refused_by_submit() -> Result<()> {
    let (_cp, _target, pd) = secure_pair()?;
    assert!(matches!(
        pd.submit(oversized_mfg_reply()),
        Err(Error::TooLong { max: 128 })
    ));
    Ok(())
}

#[test]
fn oversized_command_is_refused_by_submit() -> Result<()> {
    let (cp, target, _pd) = secure_pair()?;
    let mfg = MfgCommand {
        vendor_code: (0x0A, 0x0B, 0x0C),
        data: vec![0; 65],
    };
    assert!(matches!(
        cp.submit(target, Command::Mfg(mfg)),
        Err(Error::TooLong { max: 64 })
    ));
    Ok(())
}

#[test]
fn oversized_reply_is_refused_inside_the_handler() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    let replied = Arc::new(Mutex::new(None));
    let seen = replied.clone();
    pd.set_command_handler(move |_, reply: &mut Responder<'_>| {
        *seen.lock().unwrap() = Some(reply.reply(oversized_mfg_reply()));
        Err(Nak::Record)
    });
    let mfg = MfgCommand {
        vendor_code: (0x0A, 0x0B, 0x0C),
        data: vec![1],
    };
    let submission = cp.submit(target, Command::Mfg(mfg))?;
    assert_eq!(
        refresh_until_done(&cp, &pd, &submission),
        Err(CompletionError::Failed)
    );
    assert_eq!(
        *replied.lock().unwrap(),
        Some(Err(Error::TooLong { max: 128 }))
    );
    Ok(())
}
