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

impl From<Box<dyn Channel>> for libosdp_sys::osdp_channel {
    fn from(val: Box<dyn Channel>) -> Self {
        let id = val.get_id();
        let data = Box::into_raw(Box::new(val));
        libosdp_sys::osdp_channel {
            id,
            data: data as *mut c_void,
            recv: Some(raw_read),
            recv_pkt: None,
            send: Some(raw_write),
            flush: Some(raw_flush),
            release_pkt: None,
            close: Some(raw_close),
        }
    }
}
