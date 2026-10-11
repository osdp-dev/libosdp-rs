//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Handles on the commands and events an application submits.

use crate::{CompletionError, Error, MpKind, PdIndex, PivReply};
use alloc::rc::Rc;
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    marker::PhantomData,
};

/// Where LibOSDP's completion callback leaves the outcome of one submission
/// for its [`Submission`] to read, and the CP's event callback the reply to
/// a PIV or authenticate command.
#[derive(Default)]
pub(crate) struct Slot {
    status: Cell<Option<Result<(), CompletionError>>>,
    reply: RefCell<Option<PivReply>>,
    has_handle: Cell<bool>,
    #[cfg(feature = "async")]
    waker: crate::wake::WakerSlot,
}

impl Slot {
    pub(crate) fn complete(&self, status: Result<(), CompletionError>) {
        self.status.set(Some(status));
    }

    /// Whether the command is still running, and so still waiting for its
    /// reply: LibOSDP hands over the reply before it completes the command.
    pub(crate) fn is_pending(&self) -> bool {
        self.status.get().is_none()
    }

    /// Whether a [`Submission`] is still there to read a reply; the box
    /// LibOSDP holds keeps the slot itself alive until completion.
    pub(crate) fn has_handle(&self) -> bool {
        self.has_handle.get()
    }

    pub(crate) fn set_reply(&self, reply: PivReply) {
        *self.reply.borrow_mut() = Some(reply);
    }

    #[cfg(feature = "async")]
    pub(crate) fn take_waker(&self) -> Option<core::task::Waker> {
        self.waker.take()
    }
}

/// A command or event that LibOSDP accepted.
///
/// The handle borrows the context it was submitted to and cannot leave its
/// thread, so it can only be read between that context's refreshes.
/// Dropping it does not withdraw the submission. `R` is what it resolves
/// into: nothing, or the [`PivReply`] that
/// [`ControlPanel::piv_data`](crate::ControlPanel::piv_data) and the
/// authenticate commands are answered with.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<libosdp::Submission<'static>>();
/// ```
pub struct Submission<'a, R = ()> {
    slot: Rc<Slot>,
    operation: Option<Operation>,
    _context: PhantomData<(&'a (), fn() -> R)>,
}

/// The multi-part operation a CP command started, which
/// `osdp_cp_cancel()` names by its PD and kind.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Operation {
    pub(crate) ctx: *mut c_void,
    pub(crate) pd: PdIndex,
    pub(crate) kind: MpKind,
}

impl<R> Submission<'_, R> {
    pub(crate) fn new(slot: Rc<Slot>, operation: Option<Operation>) -> Self {
        slot.has_handle.set(true);
        Self {
            slot,
            operation,
            _context: PhantomData,
        }
    }

    /// Ask LibOSDP to stop the multi-part operation -- a file transfer, a
    /// PIV or authenticate command, or a multi-part biometric read -- that
    /// this submission started. The operation ends on a later refresh, and
    /// the submission with [`CompletionError::Failed`].
    ///
    /// Returns [`Error::NotInProgress`] once the operation has ended, and for
    /// a biometric read that fits one packet; [`Error::InvalidArgument`] for
    /// a submission that started none.
    pub fn cancel(&self) -> Result<(), Error> {
        let Some(op) = self.operation else {
            return Err(Error::InvalidArgument("not a multi-part operation"));
        };
        if !self.slot.is_pending() {
            return Err(Error::NotInProgress);
        }
        let rc = unsafe { libosdp_sys::osdp_cp_cancel(op.ctx, op.pd.as_c(), op.kind.as_c()) };
        if rc < 0 {
            Err(Error::NotInProgress)
        } else {
            Ok(())
        }
    }
}

impl Submission<'_> {
    /// `None` while LibOSDP still holds the submission, then how it ended.
    pub fn status(&self) -> Option<Result<(), CompletionError>> {
        self.slot.status.get()
    }
}

impl Submission<'_, PivReply> {
    /// `None` while the command is running, then the smart card's reply,
    /// or how the command failed.
    pub fn status(&self) -> Option<Result<PivReply, CompletionError>> {
        let status = self.slot.status.get()?;
        Some(status.and_then(|()| {
            self.slot
                .reply
                .borrow()
                .clone()
                .ok_or(CompletionError::Failed)
        }))
    }
}

impl<R> Drop for Submission<'_, R> {
    fn drop(&mut self) {
        self.slot.has_handle.set(false);
    }
}

impl<R> core::fmt::Debug for Submission<'_, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Submission")
            .field("status", &self.slot.status.get())
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "async")]
mod future {
    use super::Submission;
    use crate::{CompletionError, PivReply};
    use core::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    };

    /// Resolves once LibOSDP is done with the submission. Polling never
    /// drives LibOSDP: something must keep refreshing the context, such as
    /// [`ControlPanel::refresh`](crate::ControlPanel::refresh).
    impl Future for Submission<'_> {
        type Output = Result<(), CompletionError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            match self.status() {
                Some(status) => Poll::Ready(status),
                None => {
                    self.slot.waker.register(cx.waker());
                    Poll::Pending
                }
            }
        }
    }

    /// Resolves into the smart card's reply, or how the command failed.
    impl Future for Submission<'_, PivReply> {
        type Output = Result<PivReply, CompletionError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            match self.status() {
                Some(status) => Poll::Ready(status),
                None => {
                    self.slot.waker.register(cx.waker());
                    Poll::Pending
                }
            }
        }
    }
}
