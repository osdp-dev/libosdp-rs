//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::{OsdpCommand, OsdpEvent};
use core::ffi::c_void;

extern "C" fn cp_event_trampoline<F>(data: *mut c_void, pd: i32, event: *mut libosdp_sys::osdp_event) -> i32
where
    F: FnMut(i32, OsdpEvent) -> i32,
{
    match unsafe { core::ptr::read_unaligned(event).try_into() } {
        Ok(event) => {
            let callback: &mut F = unsafe { &mut *(data as *mut F) };
            callback(pd, event)
        },
        Err(_) => -1,  // Failed to parse the event data -> Send NAK
    }
}

type CpEventCallback =
    unsafe extern "C" fn(data: *mut c_void, pd: i32, event: *mut libosdp_sys::osdp_event) -> i32;

pub(crate) fn get_cp_event_trampoline<F>(_closure: &F) -> CpEventCallback
where
    F: FnMut(i32, OsdpEvent) -> i32,
{
    cp_event_trampoline::<F>
}

type PdCommandCallback =
    unsafe extern "C" fn(data: *mut c_void, event: *mut libosdp_sys::osdp_cmd) -> i32;

extern "C" fn pd_command_trampoline<F>(data_ptr: *mut c_void, cmd_ptr: *mut libosdp_sys::osdp_cmd) -> i32
where
    F: FnMut(&mut OsdpCommand) -> i32,
{
    match unsafe { core::ptr::read_unaligned(cmd_ptr).try_into() } {
        Ok(mut cmd) => {
            let callback: &mut F = unsafe { &mut *(data_ptr as *mut F) };
            let res = callback(&mut cmd);
            unsafe {
                core::ptr::write_unaligned(cmd_ptr, cmd.into());
            }
            res
        },
        Err(_) => -1,  // Failed to parse the command data -> Send NAK
    }
}

pub(crate) fn get_pd_command_trampoline<F>(_closure: &F) -> PdCommandCallback
where
    F: FnMut(&mut OsdpCommand) -> i32,
{
    pd_command_trampoline::<F>
}
