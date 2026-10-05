//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::{OsdpCommand, OsdpEvent, PdIndex};
use core::ffi::c_void;

pub(crate) type CpEventCallback = alloc::boxed::Box<dyn FnMut(PdIndex, OsdpEvent) -> i32 + Send>;
pub(crate) type PdCommandCallback = alloc::boxed::Box<dyn FnMut(&mut OsdpCommand) -> i32 + Send>;

pub(crate) unsafe extern "C" fn cp_event_trampoline(
    data: *mut c_void,
    pd: i32,
    event: *mut libosdp_sys::osdp_event,
) -> i32 {
    match unsafe { core::ptr::read_unaligned(event).try_into() } {
        Ok(event) => {
            let callback = unsafe { &mut *(data as *mut CpEventCallback) };
            callback(PdIndex::new(pd as u8), event)
        }
        Err(_) => -1, // Failed to parse the event data -> Send NAK
    }
}

pub(crate) unsafe extern "C" fn pd_command_trampoline(
    data: *mut c_void,
    cmd_ptr: *mut libosdp_sys::osdp_cmd,
) -> i32 {
    match unsafe { core::ptr::read_unaligned(cmd_ptr).try_into() } {
        Ok(mut cmd) => {
            let callback = unsafe { &mut *(data as *mut PdCommandCallback) };
            let res = callback(&mut cmd);
            unsafe { core::ptr::write_unaligned(cmd_ptr, cmd.into()) };
            res
        }
        Err(_) => -1, // Failed to parse the command data -> Send NAK
    }
}

/// A callback allocation shared with LibOSDP; freed when this is dropped,
/// which must happen only after LibOSDP stops using it.
pub(crate) struct OwnedCallback<T>(*mut T);

impl<T> OwnedCallback<T> {
    pub(crate) fn new(callback: T) -> Self {
        Self(alloc::boxed::Box::into_raw(alloc::boxed::Box::new(callback)))
    }

    pub(crate) fn data(&self) -> *mut c_void {
        self.0 as *mut c_void
    }
}

impl<T> Drop for OwnedCallback<T> {
    fn drop(&mut self) {
        drop(unsafe { alloc::boxed::Box::from_raw(self.0) });
    }
}
