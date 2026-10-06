//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! OSDP specification defines end-point devices as PDs. These devices are
//! responsible for controlling various hardware peripherals (such as LEDs,
//! buzzers, Displays, GPIOs, etc.,) and exposing them in a portable manner to
//! the controlling counter-part, the CP.
//!
//! PD receives commands from the CP and also generates events for activity that
//! happens on the PD itself (such as card read, key press, etc.,) snd sends it
//! to the CP.

use crate::{
    busy::Busy, queue::DrainQueue, Channel, Command, Error, Event, FileOps, FileProgress,
    Metrics, Nak, Notification, PdInfoBuilder, Submission,
};
use crate::ffi::{
    callback::{pd_command_trampoline, CallbackSlot, PdCommandSlot, Shared},
    channel::OwnedChannel,
    file::{FileRole, OwnedFileOps},
    log::pd_log_handler,
    submit::{pd_event_complete, RawSubmission},
};
use alloc::boxed::Box;
use core::{
    cell::Cell,
    marker::PhantomData,
    num::NonZeroUsize,
};

type Result<T> = core::result::Result<T, Error>;

const NOTIFICATION_QUEUE_CAPACITY: NonZeroUsize = match NonZeroUsize::new(32) {
    Some(capacity) => capacity,
    None => unreachable!(),
};

/// Answers the commands a CP sends to a [`PeripheralDevice`].
///
/// The handler runs inside [`PeripheralDevice::refresh`], since LibOSDP
/// must accept or refuse a command before it replies to the CP, so it must
/// return promptly. A panic in it is not caught: unwinding out of LibOSDP's
/// callback aborts the process.
///
/// Any `FnMut(Command, &mut Responder<'_>) -> Result<(), Nak>` closure is a
/// handler; Rust needs its second argument annotated, as in
/// `|cmd, reply: &mut Responder<'_>| ...`. A handler is moved into the
/// context, which is `Send`, so it must be `Send` too:
///
/// ```compile_fail
/// # fn f(pd: &libosdp::PeripheralDevice) {
/// let seen = std::rc::Rc::new(std::cell::Cell::new(0));
/// pd.set_command_handler(move |_, _: &mut libosdp::Responder<'_>| {
///     seen.set(seen.get() + 1);
///     Ok(())
/// });
/// # }
/// ```
pub trait CommandHandler: Send + 'static {
    /// Accept `cmd` with `Ok`, or refuse it with a [`Nak`]. A command whose
    /// reply carries data -- a manufacturer command, a status query -- is
    /// answered through `reply` before returning.
    fn handle(&mut self, cmd: Command, reply: &mut Responder<'_>) -> core::result::Result<(), Nak>;
}

impl<F> CommandHandler for F
where
    F: FnMut(Command, &mut Responder<'_>) -> core::result::Result<(), Nak> + Send + 'static,
{
    fn handle(&mut self, cmd: Command, reply: &mut Responder<'_>) -> core::result::Result<(), Nak> {
        self(cmd, reply)
    }
}

/// Answers the command a [`CommandHandler`] is handling.
pub struct Responder<'a> {
    ctx: *mut libosdp_sys::osdp_t,
    cmd: *mut libosdp_sys::osdp_cmd,
    _handling: PhantomData<&'a mut libosdp_sys::osdp_cmd>,
}

impl Responder<'_> {
    pub(crate) fn new(ctx: *mut libosdp_sys::osdp_t, cmd: *mut libosdp_sys::osdp_cmd) -> Self {
        Self {
            ctx,
            cmd,
            _handling: PhantomData,
        }
    }

    /// Send `event` as the reply to the command being handled: a
    /// [`Event::MfgReply`], [`Event::MfgStatus`] or [`Event::MfgError`] to a
    /// [`Command::Mfg`], an [`Event::BioRead`], [`Event::BioMatch`],
    /// [`Event::PivData`], [`Event::GenAuth`] or [`Event::CrAuth`] to the
    /// matching command, an [`Event::Trs`] to a [`Command::Trs`], or a
    /// [`Event::Status`] of the requested kind to a [`Command::Status`].
    ///
    /// A status of another kind than the one queried is refused with
    /// [`Error::InvalidArgument`]. A status must carry one entry for every
    /// entity of its kind the PD advertises; LibOSDP refuses the CP's query
    /// with a NAK otherwise. Any other event joins the PD's event
    /// queue: LibOSDP sends it inline, as the reply to this command, only
    /// when it answers the command and is at the head of that queue.
    /// Otherwise it goes out as the response to a later poll.
    pub fn reply(&mut self, event: Event) -> Result<()> {
        let cmd = unsafe { &mut *self.cmd };
        if cmd.id == libosdp_sys::osdp_cmd_e_OSDP_CMD_STATUS {
            if let Event::Status(status) = event {
                let query = unsafe { &mut cmd.__bindgen_anon_1.status };
                if query.type_ != status.kind().into() {
                    return Err(Error::InvalidArgument("status reply of another kind"));
                }
                *query = status.try_into()?;
                return Ok(());
            }
        }
        let (raw, _) = RawSubmission::into_raw(libosdp_sys::osdp_event::try_from(event)?);
        if unsafe { libosdp_sys::osdp_pd_submit_event(self.ctx, raw) } < 0 {
            unsafe { RawSubmission::reclaim(raw) };
            return Err(Error::Rejected);
        }
        Ok(())
    }
}

impl core::fmt::Debug for Responder<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Responder").finish_non_exhaustive()
    }
}

/// OSDP Peripheral Device (PD) context
///
/// A context can move to another thread but cannot be shared between
/// threads:
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<libosdp::PeripheralDevice>();
/// ```
pub struct PeripheralDevice {
    ctx: *mut libosdp_sys::osdp_t,
    busy: Cell<bool>,
    #[allow(dead_code)]
    channel: OwnedChannel,
    command_handler: Shared<PdCommandSlot>,
    file_ops: Option<OwnedFileOps>,
}

impl core::fmt::Debug for PeripheralDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeripheralDevice").field("ctx", &self.ctx).finish()
    }
}

unsafe impl Send for PeripheralDevice {}

impl PeripheralDevice {
    /// Create a new Peripheral panel object for the PD described by the
    /// corresponding PdInfo struct; its
    /// [`file_ops`](PdInfoBuilder::file_ops), if any, receive the files the
    /// CP sends.
    pub fn new(mut info: PdInfoBuilder, channel: Box<dyn Channel>) -> Result<Self> {
        unsafe { libosdp_sys::osdp_set_log_callback(Some(pd_log_handler)) };
        let file_ops = info.take_file_ops();
        let channel = OwnedChannel::new(channel);
        let mut raw = channel.as_raw();
        let info: crate::PdInfoHandle = info.build().into();
        let ctx = unsafe { libosdp_sys::osdp_pd_setup(&mut raw, &*info) };
        if ctx.is_null() {
            return Err(Error::Setup);
        }
        let mut pd = Self {
            ctx,
            busy: Cell::new(false),
            channel,
            command_handler: Shared::new(PdCommandSlot {
                ctx,
                handler: CallbackSlot::new(),
                notifications: DrainQueue::new(NOTIFICATION_QUEUE_CAPACITY),
            }),
            file_ops: None,
        };
        let completions = unsafe {
            libosdp_sys::osdp_pd_set_event_completion_callback(
                ctx,
                Some(pd_event_complete),
                core::ptr::null_mut(),
            )
        };
        let commands = unsafe {
            libosdp_sys::osdp_pd_set_command_callback(
                ctx,
                Some(pd_command_trampoline),
                pd.command_handler.data(),
            )
        };
        if completions < 0 || commands < 0 {
            return Err(Error::Setup);
        }
        if let Some(ops) = file_ops {
            pd.attach_file_ops(ops)?;
        }
        Ok(pd)
    }

    /// This method is used to periodically refresh the underlying LibOSDP state
    /// and must be called from the application. To meet the OSDP timing
    /// guarantees, this function must be called at least once every 50ms. This
    /// method does not block and returns early if there is nothing to be done.
    ///
    /// Called from inside one of this context's callbacks, it does nothing.
    pub fn refresh(&self) {
        let Some(_busy) = Busy::enter(&self.busy) else {
            return;
        };
        unsafe { libosdp_sys::osdp_pd_refresh(self.ctx) }
    }

    /// Drop the events queued for the CP that have not been sent yet; each
    /// one ends with [`CompletionError::Flushed`](crate::CompletionError).
    /// Returns how many were dropped.
    pub fn flush(&self) -> usize {
        let dropped = unsafe { libosdp_sys::osdp_pd_flush_events(self.ctx) };
        usize::try_from(dropped).unwrap_or(0)
    }

    /// Queue `event` to send to the CP in reply to one of its polls; the
    /// returned handle reports how it ended, once [`PeripheralDevice::refresh`]
    /// has sent it.
    pub fn submit(&self, event: Event) -> Result<Submission<'_>> {
        let (raw, slot) = RawSubmission::into_raw(libosdp_sys::osdp_event::try_from(event)?);
        let rc = unsafe { libosdp_sys::osdp_pd_submit_event(self.ctx, raw) };
        if rc < 0 {
            unsafe { RawSubmission::reclaim(raw) };
            return Err(Error::Rejected);
        }
        Ok(Submission::new(slot, None))
    }

    /// Take the oldest notification that LibOSDP raised during
    /// [`PeripheralDevice::refresh`]; raised only with
    /// [`Flag::EnableNotification`](crate::Flag::EnableNotification) set.
    /// Drain them after every refresh: once 32 are queued, newer ones are
    /// dropped and counted by [`PeripheralDevice::dropped_notifications`].
    pub fn next_notification(&self) -> Option<Notification> {
        self.command_handler.get().notifications.pop()
    }

    /// How many notifications were dropped because the queue was full,
    /// counted since this context was built; it is never reset.
    pub fn dropped_notifications(&self) -> usize {
        self.command_handler.get().notifications.dropped()
    }

    /// Answer the CP's commands with `handler`, replacing the previous one.
    /// Until a handler is set, every command is refused with
    /// [`Nak::Record`].
    pub fn set_command_handler(&self, handler: impl CommandHandler) {
        self.command_handler
            .get()
            .handler
            .replace(Box::new(handler));
    }

    /// Take the link counters and reset them, so each call returns what
    /// happened since the previous one.
    pub fn take_metrics(&self) -> Metrics {
        crate::metrics::take_metrics(self.ctx, 0)
    }

    /// Check whether a CP is talking to this PD.
    pub fn is_online(&self) -> bool {
        let mut buf: u8 = 0;
        unsafe { libosdp_sys::osdp_get_status_mask(self.ctx, &mut buf as *mut u8) };
        buf != 0
    }

    /// Check whether the secure channel with the CP is active.
    pub fn is_secure(&self) -> bool {
        let mut buf: u8 = 0;
        unsafe { libosdp_sys::osdp_get_sc_status_mask(self.ctx, &mut buf as *mut u8) };
        buf != 0
    }

    /// How far the file transfer from the CP has got, or `None` when no
    /// transfer is in progress: none was started, or the last one ended,
    /// however it ended.
    pub fn file_progress(&self) -> Option<FileProgress> {
        let mut progress = FileProgress::default();
        let rc = unsafe {
            libosdp_sys::osdp_get_file_tx_status(
                self.ctx,
                0,
                &mut progress.size,
                &mut progress.offset,
            )
        };
        (rc == 0).then_some(progress)
    }

    fn attach_file_ops(&mut self, ops: Box<dyn FileOps>) -> Result<()> {
        let ops = OwnedFileOps::new(ops, FileRole::Receiver);
        let rc = unsafe { libosdp_sys::osdp_file_register_ops(self.ctx, 0, &ops.as_raw()) };
        if rc < 0 {
            return Err(Error::Setup);
        }
        self.file_ops = Some(ops);
        Ok(())
    }
}

impl Drop for PeripheralDevice {
    fn drop(&mut self) {
        unsafe { libosdp_sys::osdp_pd_teardown(self.ctx) }
    }
}
