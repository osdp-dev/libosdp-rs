//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

#[allow(dead_code)]
mod common;

use common::memory_channel::MemoryChannel;
use libosdp::{
    BioFormat, BioMatchCommand, BioReadCommand, BioReadReply, BioStatus, BioType, Buzzer,
    BuzzerCommand, CapFunction, Capability, CardRead, Challenge, Channel, ChannelError, Command,
    CompletionError, ControlPanel, ControlPanelBuilder, CpEvent, Error, Event, FileError, FileOps,
    FileTxCommand, Finalize, Flag, KeyPress, Metrics, MfgCommand, MfgReply, MfgStatus, MpKind,
    MpOutcome, MpPhase, Multipart, Nak, Notification, OpenMode, PdId, PdIndex, PdInfoBuilder,
    PeripheralDevice, PivReply, PivRequest, ReaderStatus, Responder, Status, StatusKind,
    Submission, TimeDateCommand, TrsCommand, TrsScan,
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
    builder.add_pd(pd_info()?.file_ops(Box::new(NoFiles(drops.guard()))))?;
    let cp = builder.build()?;
    assert_eq!(drops.count(), 0);
    drop(cp);
    assert_eq!(drops.count(), 2);
    Ok(())
}

#[test]
fn pd_drop_frees_channel_handler_and_file_ops() -> Result<()> {
    let drops = DropCounter::default();
    let pd = PeripheralDevice::new(
        pd_info()?.file_ops(Box::new(NoFiles(drops.guard()))),
        Box::new(IdleChannel(drops.guard())),
    )?;
    let guard = drops.guard();
    pd.set_command_handler(move |_, _: &mut Responder<'_>| {
        let _ = &guard;
        Ok(())
    });
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
    notifying_pair_from(pd_info()?, pd_info()?)
}

/// `cp_side` describes the PD to the CP, and `pd_side` to the PD itself.
fn notifying_pair_from(
    cp_side: PdInfoBuilder,
    pd_side: PdInfoBuilder,
) -> Result<(ControlPanel, PdIndex, PeripheralDevice)> {
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let pd = PeripheralDevice::new(
        pd_side
            .flag(Flag::EnableNotification)
            .id(&PdId::from_number(7))
            .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
            .secure_channel_key(KEY),
        Box::new(pd_bus),
    )?;
    let mut builder = ControlPanelBuilder::new(Box::new(cp_bus));
    let target = builder.add_pd(
        cp_side
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
    static SEND_TARGET: Cell<Option<PdIndex>> = const { Cell::new(None) };
}

struct SubmitsFromOpen;

impl FileOps for SubmitsFromOpen {
    fn open(&mut self, _: i32, _: OpenMode) -> core::result::Result<u32, FileError> {
        let target = SEND_TARGET.get().unwrap();
        let nested_send = CP.with(|cp| {
            cp.get()
                .unwrap()
                .submit(target, Command::FileTx(FileTxCommand { id: 1 }))
                .map(|_| ())
        });
        NESTED_SEND.with(|slot| *slot.borrow_mut() = Some(nested_send));
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
fn submit_from_open_during_submit_is_rejected() -> Result<()> {
    let (cp, target, pd) =
        notifying_pair_from(pd_info()?.file_ops(Box::new(SubmitsFromOpen)), pd_info()?)?;
    refresh_until(&cp, &pd, || cp.is_online(target) && cp.is_secure(target));
    SEND_TARGET.set(Some(target));
    CP.with(|slot| slot.set(cp).unwrap());
    let sent = CP.with(|cp| {
        cp.get()
            .unwrap()
            .submit(target, Command::FileTx(FileTxCommand { id: 1 }))
            .map(|_| ())
    });
    assert!(sent.is_err());
    assert_eq!(
        NESTED_SEND.with(|slot| slot.borrow_mut().take()),
        Some(Err(Error::Rejected))
    );
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

#[test]
fn manufacturer_error_reply_fails_the_command_and_reaches_the_cp() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.set_command_handler(|cmd, reply: &mut Responder<'_>| match cmd {
        Command::Mfg(_) => reply
            .reply(Event::MfgError(MfgStatus { data: vec![0xE1] }))
            .map_err(|_| Nak::Record),
        _ => Ok(()),
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
    let mut received = None;
    refresh_until(&cp, &pd, || {
        received = cp.next_event();
        received.is_some()
    });
    let expected = Event::MfgError(MfgStatus { data: vec![0xE1] });
    assert_eq!(received, Some((target, CpEvent::Event(expected))));
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
fn status_with_an_entry_per_reader_the_pd_lacks_is_rejected() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    let replied = Arc::new(Mutex::new(None));
    let seen = replied.clone();
    pd.set_command_handler(move |cmd, reply: &mut Responder<'_>| match cmd {
        Command::Status(StatusKind::Reader) => {
            let status = Status::Reader(vec![ReaderStatus::Normal]);
            let result = reply.reply(Event::Status(status));
            *seen.lock().unwrap() = Some(result);
            result.map_err(|_| Nak::Record)
        }
        _ => Ok(()),
    });
    let submission = cp.submit(target, Command::Status(StatusKind::Reader))?;
    assert_eq!(
        refresh_until_done(&cp, &pd, &submission),
        Err(CompletionError::Failed)
    );
    assert_eq!(*replied.lock().unwrap(), Some(Err(Error::Rejected)));
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

struct MemFile(Vec<u8>);

impl FileOps for MemFile {
    fn open(&mut self, _: i32, _: OpenMode) -> core::result::Result<u32, FileError> {
        Ok(self.0.len() as u32)
    }
    fn read(&mut self, buf: &mut [u8], offset: u32) -> core::result::Result<usize, FileError> {
        let rest = self.0.get(offset as usize..).ok_or(FileError::Io)?;
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        Ok(n)
    }
    fn write(&mut self, buf: &[u8], _: u32) -> core::result::Result<usize, FileError> {
        Ok(buf.len())
    }
    fn close(&mut self) -> core::result::Result<(), FileError> {
        Ok(())
    }
}

#[test]
fn cancelled_file_transfer_fails_and_reports_aborted() -> Result<()> {
    let (cp, target, pd) = notifying_pair_from(
        pd_info()?.file_ops(Box::new(MemFile(vec![0x5A; 16 * 1024]))),
        pd_info()?.file_ops(Box::new(MemFile(Vec::new()))),
    )?;
    refresh_until(&cp, &pd, || cp.is_online(target) && cp.is_secure(target));
    pd.set_command_handler(|_, _: &mut Responder<'_>| Ok(()));
    let transfer = cp.submit(target, Command::FileTx(FileTxCommand { id: 1 }))?;
    assert_eq!(transfer.cancel(), Ok(()));
    assert_eq!(
        refresh_until_done(&cp, &pd, &transfer),
        Err(CompletionError::Failed)
    );
    assert_eq!(transfer.cancel(), Err(Error::NotInProgress));
    let mut done = None;
    while let Some((_, item)) = cp.next_event() {
        if let CpEvent::Notification(Notification::Multipart(mp)) = item {
            if let MpPhase::Done(outcome) = mp.phase {
                done = Some((mp.kind, outcome));
            }
        }
    }
    assert_eq!(done, Some((MpKind::FileTransfer, MpOutcome::Aborted)));
    Ok(())
}

#[test]
fn only_a_multi_part_operation_can_be_cancelled() -> Result<()> {
    let (cp, target, _pd) = secure_pair()?;
    let submission = cp.submit(target, buzz())?;
    assert!(matches!(
        submission.cancel(),
        Err(Error::InvalidArgument(_))
    ));
    Ok(())
}

struct VerifyingSink {
    checks_left: Arc<AtomicUsize>,
}

impl FileOps for VerifyingSink {
    fn open(&mut self, _: i32, _: OpenMode) -> core::result::Result<u32, FileError> {
        Ok(0)
    }
    fn read(&mut self, _: &mut [u8], _: u32) -> core::result::Result<usize, FileError> {
        Ok(0)
    }
    fn write(&mut self, buf: &[u8], _: u32) -> core::result::Result<usize, FileError> {
        Ok(buf.len())
    }
    fn close(&mut self) -> core::result::Result<(), FileError> {
        Ok(())
    }
    fn finalize(&mut self) -> Finalize {
        match self.checks_left.load(Ordering::SeqCst) {
            0 => Finalize::Done(MpOutcome::Invalid),
            n => {
                self.checks_left.store(n - 1, Ordering::SeqCst);
                Finalize::Pending {
                    retry_after: Duration::from_millis(10),
                }
            }
        }
    }
}

#[test]
fn pd_finalizes_a_received_file_with_its_own_outcome() -> Result<()> {
    let checks_left = Arc::new(AtomicUsize::new(2));
    let (cp, target, pd) = notifying_pair_from(
        pd_info()?.file_ops(Box::new(MemFile(vec![0x5A; 512]))),
        pd_info()?.file_ops(Box::new(VerifyingSink {
            checks_left: checks_left.clone(),
        })),
    )?;
    refresh_until(&cp, &pd, || cp.is_online(target) && cp.is_secure(target));
    pd.set_command_handler(|_, _: &mut Responder<'_>| Ok(()));
    let transfer = cp.submit(target, Command::FileTx(FileTxCommand { id: 1 }))?;
    assert_eq!(
        refresh_until_done(&cp, &pd, &transfer),
        Err(CompletionError::Failed)
    );
    assert_eq!(checks_left.load(Ordering::SeqCst), 0);
    let mut done = None;
    while let Some((_, item)) = cp.next_event() {
        if let CpEvent::Notification(Notification::Multipart(mp)) = item {
            if let MpPhase::Done(outcome) = mp.phase {
                done = Some(outcome);
            }
        }
    }
    assert_eq!(done, Some(MpOutcome::Invalid));
    Ok(())
}

fn bio_pair() -> Result<(ControlPanel, PdIndex, PeripheralDevice)> {
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let pd = PeripheralDevice::new(
        pd_info()?
            .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
            .capability(Capability::new(CapFunction::Biometrics, 1, 1))
            .secure_channel_key(KEY),
        Box::new(pd_bus),
    )?;
    let mut builder = ControlPanelBuilder::new(Box::new(cp_bus));
    let target = builder.add_pd(pd_info()?.secure_channel_key(KEY))?;
    let cp = builder.build()?;
    refresh_until(&cp, &pd, || cp.is_online(target) && cp.is_secure(target));
    Ok((cp, target, pd))
}

fn scan() -> BioReadReply {
    BioReadReply {
        reader: 0,
        status: BioStatus::Success,
        kind: BioType::RightIndexFinger,
        quality: 90,
        template: vec![0xB1; 40],
    }
}

#[test]
fn handler_answers_a_biometric_read_inline() -> Result<()> {
    let (cp, target, pd) = bio_pair()?;
    pd.set_command_handler(|cmd, reply: &mut Responder<'_>| match cmd {
        Command::BioRead(read) if read.kind == BioType::RightIndexFinger => {
            reply.reply(Event::BioRead(scan())).map_err(|_| Nak::Record)
        }
        _ => Err(Nak::BioType),
    });
    let read = BioReadCommand {
        reader: 0,
        kind: BioType::RightIndexFinger,
        format: BioFormat::AnsiIncits378,
        quality: 80,
    };
    let submission = cp.submit(target, Command::BioRead(read))?;
    assert_eq!(refresh_until_done(&cp, &pd, &submission), Ok(()));
    let mut received = None;
    refresh_until(&cp, &pd, || {
        received = cp.next_event();
        received.is_some()
    });
    assert_eq!(
        received,
        Some((target, CpEvent::Event(Event::BioRead(scan()))))
    );
    Ok(())
}

#[test]
fn a_biometric_read_is_a_cancellable_operation() -> Result<()> {
    let (cp, target, _pd) = bio_pair()?;
    let read = BioReadCommand {
        reader: 0,
        kind: BioType::RightIndexFinger,
        format: BioFormat::AnsiIncits378,
        quality: 80,
    };
    let submission = cp.submit(target, Command::BioRead(read))?;
    let cancelled = submission.cancel();
    assert!(
        matches!(cancelled, Ok(()) | Err(Error::NotInProgress)),
        "{cancelled:?}"
    );
    Ok(())
}

#[test]
fn biometric_match_of_an_unsupported_type_is_refused() -> Result<()> {
    let (cp, target, pd) = bio_pair()?;
    let seen = Arc::new(Mutex::new(None));
    let sink = seen.clone();
    pd.set_command_handler(move |cmd, _: &mut Responder<'_>| {
        *sink.lock().unwrap() = Some(cmd);
        Err(Nak::BioType)
    });
    let request = BioMatchCommand {
        reader: 0,
        kind: BioType::LeftRetina,
        format: BioFormat::NotSpecified,
        threshold: 200,
        template: vec![1, 2, 3],
    };
    let submission = cp.submit(target, Command::BioMatch(request.clone()))?;
    assert_eq!(
        refresh_until_done(&cp, &pd, &submission),
        Err(CompletionError::Failed)
    );
    assert_eq!(*seen.lock().unwrap(), Some(Command::BioMatch(request)));
    Ok(())
}

#[test]
fn pd_that_keeps_time_receives_the_date_and_time() -> Result<()> {
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let pd = PeripheralDevice::new(
        pd_info()?
            .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
            .capability(Capability::new(CapFunction::TimeKeeping, 1, 0))
            .secure_channel_key(KEY),
        Box::new(pd_bus),
    )?;
    let mut builder = ControlPanelBuilder::new(Box::new(cp_bus));
    let target = builder.add_pd(pd_info()?.secure_channel_key(KEY))?;
    let cp = builder.build()?;
    refresh_until(&cp, &pd, || cp.is_online(target) && cp.is_secure(target));
    let seen = Arc::new(Mutex::new(None));
    let sink = seen.clone();
    pd.set_command_handler(move |cmd, _: &mut Responder<'_>| {
        *sink.lock().unwrap() = Some(cmd);
        Ok(())
    });
    let set = TimeDateCommand {
        year: 2026,
        month: 10,
        day: 5,
        hour: 12,
        minute: 30,
        second: 0,
    };
    let submission = cp.submit(target, Command::TimeDate(set))?;
    assert_eq!(refresh_until_done(&cp, &pd, &submission), Ok(()));
    assert_eq!(*seen.lock().unwrap(), Some(Command::TimeDate(set)));
    Ok(())
}

#[test]
fn disabled_pd_goes_offline_until_enabled() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    assert!(cp.is_enabled(target));
    assert_eq!(cp.enable(target), Err(Error::Rejected));
    cp.disable(target)?;
    assert_eq!(cp.disable(target), Err(Error::Rejected));
    refresh_until(&cp, &pd, || !cp.is_enabled(target));
    assert!(!cp.is_online(target));
    assert!(matches!(cp.submit(target, buzz()), Err(Error::Rejected)));
    cp.enable(target)?;
    refresh_until(&cp, &pd, || cp.is_online(target) && cp.is_secure(target));
    assert!(cp.is_enabled(target));
    Ok(())
}

#[test]
fn disabling_fails_the_command_in_flight_and_keeps_the_queue() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    let in_flight = cp.submit(target, buzz())?;
    let queued = cp.submit(target, buzz())?;
    cp.disable(target)?;
    refresh_until(&cp, &pd, || !cp.is_enabled(target));
    assert_eq!(in_flight.status(), Some(Err(CompletionError::Failed)));
    assert_eq!(queued.status(), None);
    assert_eq!(cp.flush(target), 1);
    assert_eq!(queued.status(), Some(Err(CompletionError::Flushed)));
    Ok(())
}

#[test]
fn metrics_count_since_they_were_last_taken() -> Result<()> {
    let (cp, target, pd) = secure_pair()?;
    pd.set_command_handler(|_, _: &mut Responder<'_>| Ok(()));
    let submission = cp.submit(target, buzz())?;
    assert_eq!(refresh_until_done(&cp, &pd, &submission), Ok(()));
    let on_cp = cp.take_metrics(target);
    assert!(on_cp.packets_sent > 0 && on_cp.packets_received > 0);
    assert_eq!(on_cp.sc_handshakes, 1);
    assert!(pd.take_metrics().commands >= 1);
    assert_eq!(cp.take_metrics(target), Metrics::default());
    assert_eq!(pd.take_metrics(), Metrics::default());
    Ok(())
}

fn smart_card_pair() -> Result<(ControlPanel, PdIndex, PeripheralDevice)> {
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let pd = PeripheralDevice::new(
        pd_info()?
            .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
            .capability(Capability::new(CapFunction::SmartCardSupport, 1, 1))
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
    refresh_until(&cp, &pd, || cp.is_online(target) && cp.is_secure(target));
    Ok((cp, target, pd))
}

fn certificate() -> PivReply {
    PivReply {
        data: (0..=199).collect(),
    }
}

fn answer_smart_card_commands(pd: &PeripheralDevice) {
    pd.set_command_handler(|cmd, reply: &mut Responder<'_>| {
        let answer = match cmd {
            Command::PivData(_) => Event::PivData(certificate()),
            Command::GenAuth(challenge) => Event::GenAuth(PivReply {
                data: challenge.data.iter().rev().copied().collect(),
            }),
            _ => return Ok(()),
        };
        reply.reply(answer).map_err(|_| Nak::Record)
    });
}

fn request() -> PivRequest {
    PivRequest {
        object_id: [0x5F, 0xC1, 0x05],
        element: 0x70,
        offset: 0,
    }
}

fn drain_events(cp: &ControlPanel) -> Vec<CpEvent> {
    core::iter::from_fn(|| cp.next_event().map(|(_, item)| item)).collect()
}

#[test]
fn piv_data_resolves_into_the_cards_reply() -> Result<()> {
    let (cp, target, pd) = smart_card_pair()?;
    answer_smart_card_commands(&pd);
    let piv = cp.piv_data(target, request())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let reply = loop {
        if let Some(status) = piv.status() {
            break status;
        }
        assert!(Instant::now() < deadline, "PIV data never arrived");
        cp.refresh();
        pd.refresh();
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(reply, Ok(certificate()));
    assert!(!drain_events(&cp)
        .iter()
        .any(|item| matches!(item, CpEvent::Event(_))));

    let challenge = Challenge {
        algorithm: 0x11,
        key: 0x9E,
        data: vec![1, 2, 3, 4],
    };
    let auth = cp.general_auth(target, challenge)?;
    refresh_until(&cp, &pd, || auth.status().is_some());
    assert_eq!(
        auth.status(),
        Some(Ok(PivReply {
            data: vec![4, 3, 2, 1]
        }))
    );
    Ok(())
}

#[test]
fn reply_to_a_dropped_handle_reaches_next_event() -> Result<()> {
    let (cp, target, pd) = smart_card_pair()?;
    answer_smart_card_commands(&pd);
    drop(cp.piv_data(target, request())?);
    let mut events = Vec::new();
    refresh_until(&cp, &pd, || {
        events.extend(drain_events(&cp));
        events.contains(&CpEvent::Event(Event::PivData(certificate())))
    });
    Ok(())
}

#[test]
fn one_smart_card_command_runs_at_a_time() -> Result<()> {
    let (cp, target, pd) = smart_card_pair()?;
    pd.set_command_handler(|_, _: &mut Responder<'_>| Ok(()));
    let first = cp.piv_data(target, request())?;
    assert!(matches!(
        cp.piv_data(target, request()),
        Err(Error::Rejected)
    ));
    assert_eq!(first.cancel(), Ok(()));
    assert_eq!(
        refresh_until_done_with(&cp, &pd, || first.status().map(|s| s.map(|_| ()))),
        Err(CompletionError::Failed)
    );
    assert!(
        drain_events(&cp).contains(&CpEvent::Notification(Notification::Multipart(Multipart {
            kind: MpKind::Piv,
            phase: MpPhase::Done(MpOutcome::Aborted),
            object_id: 0x70,
            offset: 0,
            total: 0,
        })))
    );
    Ok(())
}

fn refresh_until_done_with(
    cp: &ControlPanel,
    pd: &PeripheralDevice,
    mut status: impl FnMut() -> Option<core::result::Result<(), CompletionError>>,
) -> core::result::Result<(), CompletionError> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = status() {
            return status;
        }
        assert!(Instant::now() < deadline, "submission never completed");
        cp.refresh();
        pd.refresh();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(not(feature = "trs"))]
#[test]
fn transparent_reader_commands_need_the_trs_feature() -> Result<()> {
    let (cp, target, _pd) = smart_card_pair()?;
    assert!(matches!(
        cp.submit(target, Command::Trs(TrsCommand::Start)),
        Err(Error::Rejected)
    ));
    Ok(())
}

#[cfg(feature = "trs")]
#[test]
fn card_session_carries_apdus_to_the_card() -> Result<()> {
    use libosdp::{CardData, TrsReply, TrsSessionStatus};

    let (cp, target, pd) = smart_card_pair()?;
    let r_apdu = TrsReply::CardData(CardData {
        reader: 0,
        status: 0,
        apdu: vec![0x90, 0x00],
    });
    let answer = r_apdu.clone();
    pd.set_command_handler(move |cmd, reply: &mut Responder<'_>| match cmd {
        Command::Trs(TrsCommand::Apdu(_)) => reply
            .reply(Event::Trs(answer.clone()))
            .map_err(|_| Nak::Record),
        _ => Ok(()),
    });
    let start = cp.submit(target, Command::Trs(TrsCommand::Start))?;
    let select = TrsCommand::Apdu(vec![0x00, 0xA4, 0x04, 0x00]);
    let apdu = cp.submit(target, Command::Trs(select))?;
    assert!(matches!(cp.submit(target, buzz()), Err(Error::Rejected)));
    let stop = cp.submit(target, Command::Trs(TrsCommand::Stop))?;
    let closed = CpEvent::Notification(Notification::TrsStatus(TrsSessionStatus::Closed));
    let mut events = Vec::new();
    refresh_until(&cp, &pd, || {
        events.extend(drain_events(&cp));
        events.contains(&closed)
    });
    for submission in [&start, &apdu, &stop] {
        assert_eq!(submission.status(), Some(Ok(())));
    }
    assert!(events.contains(&CpEvent::Event(Event::Trs(r_apdu))));
    let opened = CpEvent::Notification(Notification::TrsStatus(TrsSessionStatus::Opened));
    assert!(events.contains(&opened));
    Ok(())
}

#[test]
fn presence_scan_runs_only_with_the_trs_feature() -> Result<()> {
    let (cp, target, _pd) = smart_card_pair()?;
    let enabled = cp.trs_scan_enable(target, TrsScan::default());
    let disabled = cp.trs_scan_disable(target);
    if cfg!(feature = "trs") {
        assert_eq!((enabled, disabled), (Ok(()), Ok(())));
        let max = cp.max_apdu_len(target).unwrap();
        assert!(max > 0 && max < 258);
    } else {
        assert_eq!(
            (enabled, disabled),
            (Err(Error::Rejected), Err(Error::Rejected))
        );
        assert_eq!(cp.max_apdu_len(target), None);
    }
    Ok(())
}

