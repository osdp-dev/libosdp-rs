//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Handles on the commands and events an application submits.

use crate::CompletionError;
use alloc::rc::Rc;
use core::{cell::Cell, marker::PhantomData};

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
    _context: PhantomData<&'a ()>,
}

impl Submission<'_> {
    pub(crate) fn new(slot: Rc<Slot>) -> Self {
        Self {
            slot,
            _context: PhantomData,
        }
    }

    /// `None` while LibOSDP still holds the submission, then how it ended.
    pub fn status(&self) -> Option<Result<(), CompletionError>> {
        self.slot.status.get()
    }
}

impl core::fmt::Debug for Submission<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Submission")
            .field("status", &self.status())
            .finish()
    }
}
