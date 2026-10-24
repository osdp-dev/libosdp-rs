//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

//! LibOSDP 4.0 queues submissions by reference and returns each one through
//! the completion callback exactly once -- including on flush and teardown --
//! so that callback is where the box handed to C is freed.

use crate::{submit::Slot, CompletionError};
use alloc::{boxed::Box, rc::Rc};
use core::ffi::c_void;

/// What LibOSDP holds for one submission: the C struct it queues, then the
/// slot its outcome is reported through. `raw` is the first field of a
/// `repr(C)` struct, so a pointer to it is a pointer to the whole box.
#[repr(C)]
pub(crate) struct RawSubmission<T> {
    raw: T,
    slot: Rc<Slot>,
}

impl<T> RawSubmission<T> {
    /// Box `raw` to hand to LibOSDP. The box is freed by the completion
    /// callback, or by [`RawSubmission::reclaim`] if LibOSDP refuses it.
    pub(crate) fn into_raw(raw: T) -> (*mut T, Rc<Slot>) {
        let slot = Rc::new(Slot::default());
        let boxed = Box::new(RawSubmission {
            raw,
            slot: slot.clone(),
        });
        (Box::into_raw(boxed).cast(), slot)
    }

    /// # Safety
    ///
    /// `raw` came from [`RawSubmission::into_raw`] and LibOSDP refused it.
    pub(crate) unsafe fn reclaim(raw: *mut T) {
        drop(unsafe { Box::from_raw(raw.cast::<RawSubmission<T>>()) });
    }

    /// # Safety
    ///
    /// `raw` came from [`RawSubmission::into_raw`] and LibOSDP is done with
    /// it; `wakeups` is null or points at the context's
    /// [`Wakeups`](crate::wake::Wakeups).
    unsafe fn complete(
        raw: *mut T,
        status: libosdp_sys::osdp_completion_status,
        wakeups: *mut c_void,
    ) {
        let boxed = unsafe { Box::from_raw(raw.cast::<RawSubmission<T>>()) };
        boxed.slot.complete(completion(status));
        #[cfg(feature = "async")]
        if let (Some(waker), false) = (boxed.slot.take_waker(), wakeups.is_null()) {
            unsafe { &*(wakeups as *const crate::wake::Wakeups) }.defer(waker);
        }
        #[cfg(not(feature = "async"))]
        let _ = wakeups;
    }
}

fn completion(status: libosdp_sys::osdp_completion_status) -> Result<(), CompletionError> {
    match status {
        libosdp_sys::osdp_completion_status_OSDP_COMPLETION_OK => Ok(()),
        libosdp_sys::osdp_completion_status_OSDP_COMPLETION_FLUSHED => {
            Err(CompletionError::Flushed)
        }
        libosdp_sys::osdp_completion_status_OSDP_COMPLETION_ABORTED => {
            Err(CompletionError::Aborted)
        }
        _ => Err(CompletionError::Failed),
    }
}

pub(crate) unsafe extern "C" fn cp_command_complete(
    arg: *mut c_void,
    _pd: ::core::ffi::c_int,
    cmd: *mut libosdp_sys::osdp_cmd,
    status: libosdp_sys::osdp_completion_status,
) {
    if cmd.is_null() {
        return;
    }
    unsafe { RawSubmission::complete(cmd, status, arg) }
}

pub(crate) unsafe extern "C" fn pd_event_complete(
    arg: *mut c_void,
    event: *mut libosdp_sys::osdp_event,
    status: libosdp_sys::osdp_completion_status,
) {
    if event.is_null() {
        return;
    }
    unsafe { RawSubmission::complete(event, status, arg) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{submit::Submission, Command, KeySetCommand};

    fn keyset() -> libosdp_sys::osdp_cmd {
        Command::KeySet(KeySetCommand::new_scbk([0; 16]))
            .try_into()
            .unwrap()
    }

    #[test]
    fn completion_frees_the_box_of_a_dropped_handle() {
        let (raw, slot) = RawSubmission::into_raw(keyset());
        let weak = Rc::downgrade(&slot);
        drop(slot);
        unsafe {
            cp_command_complete(
                core::ptr::null_mut(),
                0,
                raw,
                libosdp_sys::osdp_completion_status_OSDP_COMPLETION_ABORTED,
            )
        };
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn completion_reports_to_a_live_handle() {
        let (raw, slot) = RawSubmission::into_raw(keyset());
        let handle: Submission<'_> = Submission::new(slot, None);
        assert_eq!(handle.status(), None);
        unsafe {
            cp_command_complete(
                core::ptr::null_mut(),
                0,
                raw,
                libosdp_sys::osdp_completion_status_OSDP_COMPLETION_FLUSHED,
            )
        };
        assert_eq!(handle.status(), Some(Err(CompletionError::Flushed)));
    }

    #[cfg(feature = "async")]
    #[test]
    fn completion_defers_the_waker_instead_of_waking() {
        use crate::wake::Wakeups;
        use alloc::{sync::Arc, task::Wake};
        use core::{
            future::Future,
            pin::Pin,
            sync::atomic::{AtomicUsize, Ordering},
            task::{Context, Waker},
        };

        struct Count(AtomicUsize);
        impl Wake for Count {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let count = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(count.clone());
        let wakeups = Wakeups::default();
        let (raw, slot) = RawSubmission::into_raw(keyset());
        let mut handle: Submission<'_> = Submission::new(slot, None);
        assert!(Pin::new(&mut handle)
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        unsafe {
            cp_command_complete(
                &wakeups as *const Wakeups as *mut c_void,
                0,
                raw,
                libosdp_sys::osdp_completion_status_OSDP_COMPLETION_OK,
            )
        };
        assert_eq!(count.0.load(Ordering::SeqCst), 0);
        wakeups.wake_all();
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
    }
}
