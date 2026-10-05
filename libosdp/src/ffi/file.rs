//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::OsdpFileOps;
use alloc::{boxed::Box, vec};
use core::ffi::c_void;
#[cfg(feature = "defmt")]
use defmt::error;
#[cfg(all(feature = "log", not(feature = "defmt")))]
use log::error;

unsafe extern "C" fn file_open(data: *mut c_void, file_id: i32, size: *mut i32) -> i32 {
    let ctx: *mut Box<dyn OsdpFileOps> = data as *mut _;
    let ctx = ctx.as_mut().unwrap();
    let read_only = *size == 0;
    match ctx.open(file_id, read_only) {
        Ok(file_size) => {
            if read_only {
                *size = file_size as i32;
            }
            0
        }
        Err(_e) => {
            #[cfg(any(feature = "log", feature = "defmt"))]
            error!("open: {:?}", _e);
            -1
        }
    }
}

unsafe extern "C" fn file_read(data: *mut c_void, buf: *mut c_void, size: i32, offset: i32) -> i32 {
    let ctx: *mut Box<dyn OsdpFileOps> = data as *mut _;
    let ctx = ctx.as_ref().unwrap();
    let mut read_buf = vec![0u8; size as usize];
    let len = match ctx.offset_read(&mut read_buf, offset as u64) {
        Ok(len) => len as i32,
        Err(_e) => {
            #[cfg(any(feature = "log", feature = "defmt"))]
            error!("file_read: {:?}", _e);
            -1
        }
    };
    core::ptr::copy_nonoverlapping(read_buf.as_mut_ptr(), buf as *mut u8, len as usize);
    len
}

unsafe extern "C" fn file_write(
    data: *mut c_void,
    buf: *const c_void,
    size: i32,
    offset: i32,
) -> i32 {
    let ctx: *mut Box<dyn OsdpFileOps> = data as *mut _;
    let ctx = ctx.as_ref().unwrap();
    let mut write_buf = vec![0u8; size as usize];
    core::ptr::copy_nonoverlapping(buf as *mut u8, write_buf.as_mut_ptr(), size as usize);
    match ctx.offset_write(&write_buf, offset as u64) {
        Ok(len) => len as i32,
        Err(_e) => {
            #[cfg(any(feature = "log", feature = "defmt"))]
            error!("file_write: {:?}", _e);
            -1
        }
    }
}

unsafe extern "C" fn file_close(data: *mut c_void) -> i32 {
    let ctx: *mut Box<dyn OsdpFileOps> = data as *mut _;
    let ctx = ctx.as_mut().unwrap();
    match ctx.close() {
        Ok(_) => 0,
        Err(_e) => {
            #[cfg(any(feature = "log", feature = "defmt"))]
            error!("file_close: {:?}", _e);
            -1
        }
    }
}

/// File ops whose box is shared with LibOSDP; freed when this is dropped,
/// which must happen only after LibOSDP stops using it.
pub(crate) struct OwnedFileOps(*mut Box<dyn OsdpFileOps>);

impl OwnedFileOps {
    pub(crate) fn new(ops: Box<dyn OsdpFileOps>) -> Self {
        Self(Box::into_raw(Box::new(ops)))
    }

    pub(crate) fn as_raw(&self) -> libosdp_sys::osdp_file_ops {
        libosdp_sys::osdp_file_ops {
            arg: self.0 as *mut c_void,
            open: Some(file_open),
            read: Some(file_read),
            write: Some(file_write),
            close: Some(file_close),
        }
    }
}

impl Drop for OwnedFileOps {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.0) });
    }
}

impl core::fmt::Debug for OwnedFileOps {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("OwnedFileOps").finish()
    }
}
