//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Handles on the commands and events an application submits.

use crate::{CompletionError, Error, MpKind, PdIndex};
use alloc::rc::Rc;
use core::{cell::Cell, ffi::c_void, marker::PhantomData};

/// Where LibOSDP's completion callback leaves the outcome of one submission
/// for its [`Submission`] to read.
#[derive(Default)]
pub(crate) struct Slot {
    status: Cell<Option<Result<(), CompletionError>>>,
}

impl Slot {
    pub(crate) fn complete(&self, status: Result<(), CompletionError>) {
        self.status.set(Some(status));
    }
}

/// A command or event that LibOSDP accepted.
///
/// The handle borrows the context it was submitted to and cannot leave its
/// thread, so it can only be read between that context's refreshes.
/// Dropping it does not withdraw the submission.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<libosdp::Submission<'static>>();
/// ```
pub struct Submission<'a> {
    slot: Rc<Slot>,
    operation: Option<Operation>,
    _context: PhantomData<&'a ()>,
}

/// The multi-part operation a CP command started, which
/// `osdp_cp_cancel()` names by its PD and kind.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Operation {
    pub(crate) ctx: *mut c_void,
    pub(crate) pd: PdIndex,
    pub(crate) kind: MpKind,
}

impl Submission<'_> {
    pub(crate) fn new(slot: Rc<Slot>, operation: Option<Operation>) -> Self {
        Self {
            slot,
            operation,
            _context: PhantomData,
        }
    }

    /// `None` while LibOSDP still holds the submission, then how it ended.
    pub fn status(&self) -> Option<Result<(), CompletionError>> {
        self.slot.status.get()
    }

    /// Ask LibOSDP to stop the multi-part operation -- a file transfer --
    /// that this submission started. The operation ends on a later
    /// refresh, and the submission with [`CompletionError::Failed`].
    ///
    /// Returns [`Error::NotInProgress`] once the operation has ended, and
    /// [`Error::InvalidArgument`] for a submission that started none.
    pub fn cancel(&self) -> Result<(), Error> {
        let Some(op) = self.operation else {
            return Err(Error::InvalidArgument("not a multi-part operation"));
        };
        if self.status().is_some() {
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

impl core::fmt::Debug for Submission<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Submission")
            .field("status", &self.status())
            .finish()
    }
}
