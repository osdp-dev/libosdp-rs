//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

//! The Control Panel (CP) is responsible to connecting to and managing multiple Peripheral Devices
//! (PD) on the OSDP bus. It can send commands to and receive events from PDs.

use crate::{
    busy::Busy, CapFunction, Capability, Challenge, Channel, Command, Error, Event, FileOps,
    FileProgress, Flag, Metrics, MpKind, Notification, PdId, PdInfoBuilder, PivReply, PivRequest,
    Submission, TrsScan,
};
use crate::submit::{Operation, Slot};
use crate::ffi::{
    callback::{cp_event_trampoline, CpEvents, Shared},
    channel::OwnedChannel,
    file::{FileRole, OwnedFileOps},
    log::cp_log_handler,
    submit::{cp_command_complete, RawSubmission},
};
use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::num::NonZeroUsize;
use core::cell::Cell;

type Result<T> = core::result::Result<T, Error>;

const MAX_PDS_PER_BUS: usize = 126;
const DEFAULT_EVENT_QUEUE_CAPACITY: NonZeroUsize = match NonZeroUsize::new(32) {
    Some(capacity) => capacity,
    None => unreachable!(),
};

/// A PD on a [`ControlPanel`]'s bus, handed out by
/// [`ControlPanelBuilder::add_pd`] in the order the PDs were added. An index
/// is only meaningful on the `ControlPanel` built by the builder that issued
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PdIndex(u8);

impl PdIndex {
    pub(crate) fn new(offset: u8) -> Self {
        Self(offset)
    }

    pub(crate) fn as_c(self) -> core::ffi::c_int {
        self.0 as core::ffi::c_int
    }
}

impl From<PdIndex> for usize {
    fn from(pd: PdIndex) -> usize {
        pd.0 as usize
    }
}

impl core::fmt::Display for PdIndex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What [`ControlPanel::next_event`] hands over.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CpEvent {
    /// An event a PD reported.
    Event(Event),

    /// Something LibOSDP observed about the link to a PD; reported for PDs
    /// with [`Flag::EnableNotification`] set.
    Notification(Notification),
}

/// Builder for a [`ControlPanel`] driving the PDs on one OSDP bus.
#[derive(Debug)]
pub struct ControlPanelBuilder {
    channel: Box<dyn Channel>,
    pd_info: Vec<PdInfoBuilder>,
    event_queue_capacity: NonZeroUsize,
}

impl ControlPanelBuilder {
    /// Create a builder for the PDs reachable over `channel`.
    pub fn new(channel: Box<dyn Channel>) -> Self {
        Self {
            channel,
            pd_info: Vec::new(),
            event_queue_capacity: DEFAULT_EVENT_QUEUE_CAPACITY,
        }
    }

    /// Hold up to `capacity` events (32 by default) between two calls that
    /// drain them; see [`ControlPanel::next_event`].
    pub fn event_queue_capacity(mut self, capacity: NonZeroUsize) -> Self {
        self.event_queue_capacity = capacity;
        self
    }

    /// Add a PD on this bus; the returned index addresses it on the built
    /// [`ControlPanel`]. The PD's [`file_ops`](PdInfoBuilder::file_ops), if
    /// any, serve the files sent to it.
    pub fn add_pd(&mut self, pd_info: PdInfoBuilder) -> Result<PdIndex> {
        if self.pd_info.len() == MAX_PDS_PER_BUS {
            return Err(Error::InvalidArgument("more than 126 PDs on one bus"));
        }
        self.pd_info.push(pd_info);
        Ok(PdIndex::new((self.pd_info.len() - 1) as u8))
    }

    /// Build the [`ControlPanel`] instance.
    pub fn build(self) -> Result<ControlPanel> {
        let mut file_ops = Vec::new();
        let info: Vec<crate::PdInfoHandle> = self
            .pd_info
            .into_iter()
            .enumerate()
            .map(|(offset, mut pd)| {
                if let Some(ops) = pd.take_file_ops() {
                    file_ops.push((PdIndex::new(offset as u8), ops));
                }
                pd.build().into()
            })
            .collect();
        let channel = OwnedChannel::new(self.channel);
        let raw = channel.as_raw();
        unsafe { libosdp_sys::osdp_set_log_callback(Some(cp_log_handler)) };
        let ctx = unsafe {
            libosdp_sys::osdp_cp_setup(&raw, info.len() as i32, info.as_ptr() as *const _)
        };
        if ctx.is_null() {
            return Err(Error::Setup);
        }
        let mut cp = ControlPanel {
            ctx,
            busy: Cell::new(false),
            channel,
            events: Shared::new(CpEvents::new(self.event_queue_capacity)),
            #[cfg(feature = "async")]
            wakeups: Shared::new(Default::default()),
            file_ops: Vec::new(),
        };
        #[cfg(feature = "async")]
        let completion_arg = cp.wakeups.data();
        #[cfg(not(feature = "async"))]
        let completion_arg = core::ptr::null_mut();
        let completions = unsafe {
            libosdp_sys::osdp_cp_set_command_completion_callback(
                ctx,
                Some(cp_command_complete),
                completion_arg,
            )
        };
        let events = unsafe {
            libosdp_sys::osdp_cp_set_event_callback(
                ctx,
                Some(cp_event_trampoline),
                cp.events.data(),
            )
        };
        if completions < 0 || events < 0 {
            return Err(Error::Setup);
        }
        for (pd, ops) in file_ops {
            cp.attach_file_ops(pd, ops)?;
        }
        Ok(cp)
    }
}

/// OSDP CP device context.
///
/// A context can move to another thread but cannot be shared between
/// threads; wrap it in a `Mutex` to submit from several:
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<libosdp::ControlPanel>();
/// ```
pub struct ControlPanel {
    ctx: *mut core::ffi::c_void,
    busy: Cell<bool>,
    #[allow(dead_code)]
    channel: OwnedChannel,
    events: Shared<CpEvents>,
    #[cfg(feature = "async")]
    wakeups: Shared<crate::wake::Wakeups>,
    file_ops: Vec<OwnedFileOps>,
}

impl core::fmt::Debug for ControlPanel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ControlPanel").field("ctx", &self.ctx).finish()
    }
}

unsafe impl Send for ControlPanel {}

impl ControlPanel {
    /// The application must call this method periodically to refresh the
    /// underlying LibOSDP state. To meet the OSDP timing guarantees, this
    /// function must be called at least once every 50ms. This method does not
    /// block and returns early if there is nothing to be done.
    ///
    /// Called from inside one of this context's callbacks, it does nothing.
    /// That includes application code that [`submit`](Self::submit)
    /// runs, such as [`FileOps::open`] when starting a file transfer.
    pub fn refresh(&self) {
        {
            let Some(_busy) = Busy::enter(&self.busy) else {
                return;
            };
            unsafe { libosdp_sys::osdp_cp_refresh(self.ctx) }
        }
        self.wake_waiting();
    }

    /// Wake the tasks whose submissions LibOSDP completed, and the one
    /// waiting on the event stream when events are queued, during the call
    /// that just returned.
    ///
    /// Called from inside one of this context's callbacks, it wakes nothing:
    /// the refresh that is running wakes them once LibOSDP returns.
    fn wake_waiting(&self) {
        #[cfg(feature = "async")]
        {
            if self.busy.get() {
                return;
            }
            self.wakeups.get().wake_all();
            let events = self.events.get();
            if !events.queue.is_empty() {
                if let Some(waker) = events.stream_waker.take() {
                    waker.wake();
                }
            }
        }
    }

    /// Refresh this context forever, awaiting `tick()` between refreshes.
    ///
    /// Join it with the application's futures on a local executor. It never
    /// returns; drop its future to stop it, for example by selecting it
    /// against the application's.
    ///
    /// Make `tick` a timer of at most 50ms, such as `tokio::time::sleep` or
    /// an embassy `Timer`: LibOSDP needs refreshing about that often, and a
    /// longer tick delays protocol traffic and completions. A tick that is
    /// ready at once, such as `async {}`, never yields, so `run` spins and
    /// starves the futures joined with it.
    #[cfg(feature = "async")]
    pub async fn run<F, Fut>(&self, mut tick: F) -> !
    where
        F: FnMut() -> Fut,
        Fut: core::future::Future<Output = ()>,
    {
        loop {
            self.refresh();
            tick().await;
        }
    }

    /// Queue `cmd` for `pd`; the returned handle reports how it ended, once
    /// [`ControlPanel::refresh`] has run it.
    ///
    /// LibOSDP accepts commands only while `pd` is online, and returns
    /// [`Error::Rejected`] otherwise. An accepted command outlasts a link
    /// outage: it stays queued and is sent when the PD comes back, unless
    /// [`ControlPanel::flush`] drops it first. A [`Command::FileTx`] ends
    /// when the transfer does, not when the PD acknowledges the command,
    /// and [`Submission::cancel`] stops the transfer early.
    ///
    /// Refused with [`Error::Rejected`] from inside one of this context's
    /// callbacks, including application code that this method itself runs,
    /// such as [`FileOps::open`] when starting a file transfer.
    ///
    /// A command whose timings LibOSDP cannot express is refused with
    /// [`Error::InvalidArgument`] before it is queued.
    pub fn submit(&self, pd: PdIndex, cmd: Command) -> Result<Submission<'_>> {
        let (slot, operation) = self.queue_command(pd, cmd, false)?;
        Ok(Submission::new(slot, operation))
    }

    /// Read a PIV data object from the smart card in `pd`'s reader; the
    /// returned handle resolves into the card's reply. LibOSDP runs one
    /// PIV or authenticate command per PD at a time, and refuses another
    /// with [`Error::Rejected`] until it ends.
    ///
    /// Dropping the handle sends the reply, when it comes, to
    /// [`ControlPanel::next_event`] as an [`Event::PivData`], which is also
    /// where it goes when the command is sent with
    /// [`submit`](Self::submit).
    pub fn piv_data(&self, pd: PdIndex, request: PivRequest) -> Result<Submission<'_, PivReply>> {
        let (slot, operation) = self.queue_command(pd, Command::PivData(request), true)?;
        Ok(Submission::new(slot, operation))
    }

    /// Have the smart card in `pd`'s reader answer `challenge` with a
    /// general authenticate (osdp_GENAUTH); otherwise as
    /// [`piv_data`](Self::piv_data).
    pub fn general_auth(
        &self,
        pd: PdIndex,
        challenge: Challenge,
    ) -> Result<Submission<'_, PivReply>> {
        let (slot, operation) = self.queue_command(pd, Command::GenAuth(challenge), true)?;
        Ok(Submission::new(slot, operation))
    }

    /// Have the smart card in `pd`'s reader answer `challenge` with a
    /// challenge/response authenticate (osdp_CRAUTH); otherwise as
    /// [`piv_data`](Self::piv_data).
    pub fn challenge_auth(
        &self,
        pd: PdIndex,
        challenge: Challenge,
    ) -> Result<Submission<'_, PivReply>> {
        let (slot, operation) = self.queue_command(pd, Command::CrAuth(challenge), true)?;
        Ok(Submission::new(slot, operation))
    }

    /// Hand `cmd` to LibOSDP for `pd`. A PIV or authenticate command's reply
    /// is routed to the returned slot when `wants_reply`, and to the event
    /// queue otherwise.
    fn queue_command(
        &self,
        pd: PdIndex,
        cmd: Command,
        wants_reply: bool,
    ) -> Result<(Rc<Slot>, Option<Operation>)> {
        let Some(busy) = Busy::enter(&self.busy) else {
            return Err(Error::Rejected);
        };
        let kind = cmd.multipart_kind();
        let (raw, slot) = RawSubmission::into_raw(libosdp_sys::osdp_cmd::try_from(cmd)?);
        let rc = unsafe { libosdp_sys::osdp_cp_submit_command(self.ctx, pd.as_c(), raw) };
        drop(busy);
        self.wake_waiting();
        if rc < 0 {
            unsafe { RawSubmission::reclaim(raw) };
            return Err(Error::Rejected);
        }
        if matches!(kind, Some(MpKind::Piv | MpKind::GenAuth | MpKind::CrAuth)) {
            let waiting = wants_reply.then(|| Rc::downgrade(&slot));
            self.events.get().route_reply(pd, waiting);
        }
        let operation = kind.map(|kind| Operation {
            ctx: self.ctx,
            pd,
            kind,
        });
        Ok((slot, operation))
    }

    /// Drop the commands queued for `pd` that have not been sent yet; each
    /// one ends with [`CompletionError::Flushed`](crate::CompletionError).
    /// Returns how many were dropped.
    pub fn flush(&self, pd: PdIndex) -> usize {
        let dropped = unsafe { libosdp_sys::osdp_cp_flush_commands(self.ctx, pd.as_c()) };
        self.wake_waiting();
        usize::try_from(dropped).unwrap_or(0)
    }

    /// The events and notifications that [`next_event`](Self::next_event)
    /// hands over, as a stream that wakes its task after a refresh queues
    /// one. Refreshing is still up to the application, for example with
    /// [`run`](Self::run).
    #[cfg(feature = "async")]
    pub fn events(&self) -> crate::EventStream<'_> {
        crate::EventStream::new(self)
    }

    #[cfg(feature = "async")]
    pub(crate) fn park_event_waker(&self, waker: &core::task::Waker) {
        self.events.get().stream_waker.register(waker);
    }

    /// Take the oldest event that PDs reported, or notification that
    /// LibOSDP raised, during [`ControlPanel::refresh`]. Drain them after
    /// every refresh: once the queue is full, newer ones are dropped and
    /// counted by [`ControlPanel::dropped_events`].
    pub fn next_event(&self) -> Option<(PdIndex, CpEvent)> {
        self.events.get().queue.pop()
    }

    /// How many events were dropped because the queue was full, counted
    /// since this context was built; it is never reset.
    pub fn dropped_events(&self) -> usize {
        self.events.get().queue.dropped()
    }

    /// The [`PdId`] that `pd` reported, or `None` while `pd` is offline:
    /// the CP reads it each time `pd` comes online, and keeps the last one
    /// read until then.
    pub fn pd_id(&self, pd: PdIndex) -> Option<PdId> {
        if !self.is_online(pd) {
            return None;
        }
        let mut pd_id: libosdp_sys::osdp_pd_id =
            unsafe { core::mem::MaybeUninit::zeroed().assume_init() };
        let rc = unsafe { libosdp_sys::osdp_cp_get_pd_id(self.ctx, pd.as_c(), &mut pd_id) };
        (rc == 0).then(|| pd_id.into())
    }

    /// The [`Capability`] that `pd` last advertised for `function`, or
    /// `None` while `pd` is offline or when it did not advertise `function`.
    /// A capability advertised with a compliance level and item count of 0
    /// reads as `None` too.
    pub fn capability(&self, pd: PdIndex, function: CapFunction) -> Option<Capability> {
        if !self.is_online(pd) {
            return None;
        }
        let mut cap = Capability::new(function, 0, 0).into();
        let rc = unsafe { libosdp_sys::osdp_cp_get_capability(self.ctx, pd.as_c(), &mut cap) };
        let cap = Capability::try_from(cap).ok().filter(|_| rc == 0)?;
        (cap.compliance != 0 || cap.items != 0).then_some(cap)
    }

    /// Set `flags` for `pd` on a running link.
    ///
    /// Only [`Flag::EnforceSecure`], [`Flag::IgnoreUnsolicited`],
    /// [`Flag::EnableNotification`] and
    /// [`Flag::AllowEmptyEncryptedDataBlock`] can change after setup.
    /// LibOSDP refuses any other flag, and [`Flag::EnforceSecure`] on a PD
    /// set up without a secure channel key, with [`Error::Rejected`]; a
    /// refused call changes no flag.
    pub fn set_flags(&self, pd: PdIndex, flags: Flag) -> Result<()> {
        self.modify_flags(pd, flags, true)
    }

    /// Clear `flags` for `pd` on a running link. As for
    /// [`set_flags`](Self::set_flags), and [`Flag::EnforceSecure`], once
    /// set, cannot be cleared.
    pub fn clear_flags(&self, pd: PdIndex, flags: Flag) -> Result<()> {
        self.modify_flags(pd, flags, false)
    }

    fn modify_flags(&self, pd: PdIndex, flags: Flag, set: bool) -> Result<()> {
        let rc =
            unsafe { libosdp_sys::osdp_cp_modify_flag(self.ctx, pd.as_c(), flags.bits(), set) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            Ok(())
        }
    }

    /// Stop talking to `pd`: on the next refresh, LibOSDP brings it to a
    /// safe state and stops polling it, so it goes offline and refuses
    /// commands until [`enable`](Self::enable)d again. A
    /// [`submit`](Self::submit) before that refresh is still accepted.
    /// The command in flight then fails with
    /// [`CompletionError::Failed`](crate::CompletionError), and a file
    /// transfer is aborted; commands queued behind it stay pending
    /// until the PD is enabled and they are sent, or
    /// [`flush`](Self::flush) drops them. Refused with
    /// [`Error::Rejected`] when `pd` is disabled already.
    pub fn disable(&self, pd: PdIndex) -> Result<()> {
        let rc = unsafe { libosdp_sys::osdp_cp_disable_pd(self.ctx, pd.as_c()) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            Ok(())
        }
    }

    /// Start talking to a [`disable`](Self::disable)d `pd` again, from
    /// scratch as at setup, on the next refresh; commands left queued
    /// are then sent. Refused with [`Error::Rejected`] when `pd` is not
    /// disabled.
    pub fn enable(&self, pd: PdIndex) -> Result<()> {
        let rc = unsafe { libosdp_sys::osdp_cp_enable_pd(self.ctx, pd.as_c()) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            Ok(())
        }
    }

    /// Check whether `pd` is enabled; a PD is disabled from the refresh
    /// after [`disable`](Self::disable) until the one after
    /// [`enable`](Self::enable).
    pub fn is_enabled(&self, pd: PdIndex) -> bool {
        unsafe { libosdp_sys::osdp_cp_is_pd_enabled(self.ctx, pd.as_c()) }
    }

    /// Take the link counters for `pd` and reset them, so each call returns
    /// what happened since the previous one.
    pub fn take_metrics(&self, pd: PdIndex) -> Metrics {
        crate::metrics::take_metrics(self.ctx, pd.as_c())
    }

    /// Have LibOSDP scan `pd`'s reader for smart cards in the background,
    /// at the cadence `scan` sets, and report a card it sees as an
    /// [`Event::Trs`]. Only readers that do not report cards on their own
    /// need this. The scan pauses during a card session or file transfer;
    /// a reader that refuses a probe raises
    /// [`TrsSessionStatus::ScanSuspended`](crate::TrsSessionStatus) and is
    /// probed again later, backing off.
    ///
    /// Refused with [`Error::Rejected`] without the `trs` feature.
    pub fn trs_scan_enable(&self, pd: PdIndex, scan: TrsScan) -> Result<()> {
        let params = libosdp_sys::osdp_trs_scan_params::try_from(scan)?;
        let rc = unsafe { libosdp_sys::osdp_cp_trs_scan_enable(self.ctx, pd.as_c(), &params) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            Ok(())
        }
    }

    /// Stop the presence scan on `pd`; its reader is returned to its
    /// default mode on the next refresh. Refused with [`Error::Rejected`]
    /// without the `trs` feature.
    pub fn trs_scan_disable(&self, pd: PdIndex) -> Result<()> {
        let rc = unsafe { libosdp_sys::osdp_cp_trs_scan_disable(self.ctx, pd.as_c()) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            Ok(())
        }
    }

    /// The largest C-APDU that fits a [`TrsCommand::Apdu`](crate::TrsCommand)
    /// to `pd`, which the packet size in use on the link bounds; final only
    /// once `pd` has been online. `None` without the `trs` feature.
    pub fn max_apdu_len(&self, pd: PdIndex) -> Option<usize> {
        let len = unsafe { libosdp_sys::osdp_cp_trs_get_max_apdu_len(self.ctx, pd.as_c()) };
        usize::try_from(len).ok()
    }

    /// Check whether `pd` is online.
    pub fn is_online(&self, pd: PdIndex) -> bool {
        let mut buf: [u8; 16] = [0; 16];
        unsafe { libosdp_sys::osdp_get_status_mask(self.ctx, &mut buf as *mut u8) };
        let pd = usize::from(pd);
        buf[pd / 8] & (1 << (pd % 8)) != 0
    }

    /// Check whether `pd` has an active secure channel.
    pub fn is_secure(&self, pd: PdIndex) -> bool {
        let mut buf: [u8; 16] = [0; 16];
        unsafe { libosdp_sys::osdp_get_sc_status_mask(self.ctx, &mut buf as *mut u8) };
        let pd = usize::from(pd);
        buf[pd / 8] & (1 << (pd % 8)) != 0
    }

    /// How far the file transfer to `pd` has got, or `None` when no
    /// transfer is in progress: none was started, or the last one ended,
    /// however it ended.
    pub fn file_progress(&self, pd: PdIndex) -> Option<FileProgress> {
        let mut progress = FileProgress::default();
        let rc = unsafe {
            libosdp_sys::osdp_get_file_tx_status(
                self.ctx,
                pd.as_c(),
                &mut progress.size,
                &mut progress.offset,
            )
        };
        (rc == 0).then_some(progress)
    }

    fn attach_file_ops(&mut self, pd: PdIndex, ops: Box<dyn FileOps>) -> Result<()> {
        let ops = OwnedFileOps::new(ops, FileRole::Sender);
        let rc = unsafe { libosdp_sys::osdp_file_register_ops(self.ctx, pd.as_c(), &ops.as_raw()) };
        if rc < 0 {
            return Err(Error::Setup);
        }
        self.file_ops.push(ops);
        Ok(())
    }
}

impl Drop for ControlPanel {
    fn drop(&mut self) {
        unsafe { libosdp_sys::osdp_cp_teardown(self.ctx) }
    }
}
