//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::{Channel, ChannelError};
use alloc::{boxed::Box, vec};
use core::ffi::c_void;

unsafe extern "C" fn raw_read(data: *mut c_void, buf: *mut u8, len: i32) -> i32 {
    let channel: *mut Box<dyn Channel> = data as *mut _;
    let channel = channel.as_mut().unwrap();
    let mut read_buf = vec![0u8; len as usize];
    match channel.read(&mut read_buf) {
        Ok(n) => {
            let src_ptr = read_buf.as_mut_ptr();
            core::ptr::copy_nonoverlapping(src_ptr, buf, len as usize);
            n as i32
        }
        Err(ChannelError::WouldBlock) => 0,
        Err(_) => -1,
    }
}

unsafe extern "C" fn raw_write(data: *mut c_void, buf: *mut u8, len: i32) -> i32 {
    let channel: *mut Box<dyn Channel> = data as *mut _;
    let channel = channel.as_mut().unwrap();
    let mut write_buf = vec![0u8; len as usize];
    core::ptr::copy_nonoverlapping(buf, write_buf.as_mut_ptr(), len as usize);
    match channel.as_mut().write(&write_buf) {
        Ok(n) => n as i32,
        Err(ChannelError::WouldBlock) => 0,
        Err(_) => -1,
    }
}

unsafe extern "C" fn raw_flush(data: *mut c_void) {
    let channel: *mut Box<dyn Channel> = data as *mut _;
    let channel = channel.as_mut().unwrap();
    let _ = channel.as_mut().flush();
}

unsafe extern "C" fn raw_close(data: *mut c_void) {
    let channel: *mut Box<dyn Channel> = data as *mut _;
    let channel = channel.as_mut().unwrap();
    let _ = channel.as_mut().close();
}

/// A channel whose box is shared with LibOSDP; freed when this is dropped,
/// which must happen only after the context is torn down.
pub(crate) struct OwnedChannel(*mut Box<dyn Channel>);

// The box holds a `dyn Channel`, and `Channel: Send`.
unsafe impl Send for OwnedChannel {}

impl OwnedChannel {
    pub(crate) fn new(channel: Box<dyn Channel>) -> Self {
        Self(Box::into_raw(Box::new(channel)))
    }

    pub(crate) fn data(&self) -> *mut c_void {
        self.0 as *mut c_void
    }

    pub(crate) fn as_raw(&self, id: i32) -> libosdp_sys::osdp_channel {
        libosdp_sys::osdp_channel {
            id,
            data: self.data(),
            recv: Some(raw_read),
            recv_pkt: None,
            send: Some(raw_write),
            flush: Some(raw_flush),
            release_pkt: None,
            close: Some(raw_close),
        }
    }
}

impl Drop for OwnedChannel {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.0) });
    }
}

impl core::fmt::Debug for OwnedChannel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("OwnedChannel").finish()
    }
}
