//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! The Control Panel (CP) is responsible to connecting to and managing multiple Peripheral Devices
//! (PD) on the OSDP bus. It can send commands to and receive events from PDs.

use crate::{
    busy::Busy, CapFunction, Capability, Channel, Command, Error, Event, FileOps, FileProgress,
    Flag, PdId, PdInfoBuilder, Submission,
};
use crate::ffi::{
    callback::{cp_event_trampoline, CpEventQueue, Shared},
    channel::OwnedChannel,
    file::{FileRole, OwnedFileOps},
    log::cp_log_handler,
    submit::{cp_command_complete, RawSubmission},
};
use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use core::num::NonZeroUsize;
use core::cell::{Cell, RefCell};

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
    /// [`ControlPanel`].
    pub fn add_pd(&mut self, pd_info: PdInfoBuilder) -> Result<PdIndex> {
        if self.pd_info.len() == MAX_PDS_PER_BUS {
            return Err(Error::InvalidArgument("more than 126 PDs on one bus"));
        }
        self.pd_info.push(pd_info);
        Ok(PdIndex::new((self.pd_info.len() - 1) as u8))
    }

    /// Build the [`ControlPanel`] instance.
    pub fn build(self) -> Result<ControlPanel> {
        let info: Vec<crate::PdInfoHandle> =
            self.pd_info.into_iter().map(|pd| pd.build().into()).collect();
        let channel = OwnedChannel::new(self.channel);
        let raw = channel.as_raw();
        unsafe { libosdp_sys::osdp_set_log_callback(Some(cp_log_handler)) };
        let ctx = unsafe {
            libosdp_sys::osdp_cp_setup(&raw, info.len() as i32, info.as_ptr() as *const _)
        };
        if ctx.is_null() {
            return Err(Error::Setup);
        }
        unsafe {
            libosdp_sys::osdp_cp_set_command_completion_callback(
                ctx,
                Some(cp_command_complete),
                core::ptr::null_mut(),
            )
        };
        let events = Shared::new(CpEventQueue::new(self.event_queue_capacity));
        unsafe {
            libosdp_sys::osdp_cp_set_event_callback(ctx, Some(cp_event_trampoline), events.data())
        };
        Ok(ControlPanel {
            ctx,
            busy: Cell::new(false),
            channel,
            events,
            file_ops: RefCell::new(BTreeMap::new()),
        })
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
    events: Shared<CpEventQueue>,
    file_ops: RefCell<BTreeMap<PdIndex, OwnedFileOps>>,
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
        let Some(_busy) = Busy::enter(&self.busy) else {
            return;
        };
        unsafe { libosdp_sys::osdp_cp_refresh(self.ctx) }
    }

    /// Queue `cmd` for `pd`; the returned handle reports how it ended, once
    /// [`ControlPanel::refresh`] has run it.
    ///
    /// LibOSDP accepts commands only while `pd` is online, and returns
    /// [`Error::Rejected`] otherwise. An accepted command outlasts a link
    /// outage: it stays queued and is sent when the PD comes back, unless
    /// [`ControlPanel::flush`] drops it first. A [`Command::FileTx`] ends
    /// when the transfer does, not when the PD acknowledges the command.
    ///
    /// Refused with [`Error::Rejected`] from inside one of this context's
    /// callbacks, including application code that this method itself runs,
    /// such as [`FileOps::open`] when starting a file transfer.
    ///
    /// A command whose timings LibOSDP cannot express is refused with
    /// [`Error::InvalidArgument`] before it is queued.
    pub fn submit(&self, pd: PdIndex, cmd: Command) -> Result<Submission<'_>> {
        let Some(_busy) = Busy::enter(&self.busy) else {
            return Err(Error::Rejected);
        };
        let (raw, slot) = RawSubmission::into_raw(libosdp_sys::osdp_cmd::try_from(cmd)?);
        let rc = unsafe { libosdp_sys::osdp_cp_submit_command(self.ctx, pd.as_c(), raw) };
        if rc < 0 {
            unsafe { RawSubmission::reclaim(raw) };
            return Err(Error::Rejected);
        }
        Ok(Submission::new(slot))
    }

    /// Drop the commands queued for `pd` that have not been sent yet; each
    /// one ends with [`CompletionError::Flushed`](crate::CompletionError).
    /// Returns how many were dropped.
    pub fn flush(&self, pd: PdIndex) -> usize {
        let dropped = unsafe { libosdp_sys::osdp_cp_flush_commands(self.ctx, pd.as_c()) };
        usize::try_from(dropped).unwrap_or(0)
    }

    /// Take the oldest event that PDs reported during
    /// [`ControlPanel::refresh`]. Drain them after every refresh: once the
    /// queue is full, newer events are dropped and counted by
    /// [`ControlPanel::dropped_events`].
    pub fn next_event(&self) -> Option<(PdIndex, CpEvent)> {
        self.events.get().pop()
    }

    /// How many events were dropped because the queue was full, counted
    /// since this context was built; it is never reset.
    pub fn dropped_events(&self) -> usize {
        self.events.get().dropped()
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

    /// Register a file operations handler for `pd`. See [`crate::FileOps`]
    /// trait documentation for more details.
    ///
    /// Registering aborts a transfer in progress. It is refused with
    /// [`Error::Rejected`] from inside one of this context's callbacks,
    /// including application code that [`submit`](Self::submit)
    /// runs, such as [`FileOps::open`] when starting a file transfer.
    pub fn register_file_ops(&self, pd: PdIndex, fops: Box<dyn FileOps>) -> Result<()> {
        let Some(_busy) = Busy::enter(&self.busy) else {
            return Err(Error::Rejected);
        };
        let fops = OwnedFileOps::new(fops, FileRole::Sender);
        let raw = fops.as_raw();
        let rc = unsafe { libosdp_sys::osdp_file_register_ops(self.ctx, pd.as_c(), &raw) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            self.file_ops.borrow_mut().insert(pd, fops);
            Ok(())
        }
    }
}

impl Drop for ControlPanel {
    fn drop(&mut self) {
        unsafe { libosdp_sys::osdp_cp_teardown(self.ctx) }
    }
}
