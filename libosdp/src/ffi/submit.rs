//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! LibOSDP 4.0 queues submissions by reference and returns each one through
//! the completion callback exactly once -- including on flush and teardown --
//! so that callback is where the box handed to C is freed.

use alloc::boxed::Box;
use core::ffi::c_void;

pub(crate) unsafe extern "C" fn cp_command_complete(
    _arg: *mut c_void,
    _pd: ::core::ffi::c_int,
    cmd: *mut libosdp_sys::osdp_cmd,
    _status: libosdp_sys::osdp_completion_status,
) {
    if cmd.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(cmd) });
}

pub(crate) unsafe extern "C" fn pd_event_complete(
    _arg: *mut c_void,
    event: *mut libosdp_sys::osdp_event,
    _status: libosdp_sys::osdp_completion_status,
) {
    if event.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(event) });
}
