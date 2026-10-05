//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::FileOps;
use alloc::{boxed::Box, vec};
use core::ffi::c_void;
#[cfg(feature = "defmt")]
use defmt::error;
#[cfg(all(feature = "log", not(feature = "defmt")))]
use log::error;

unsafe extern "C" fn file_open(data: *mut c_void, file_id: i32, size: *mut u32) -> i32 {
    let ctx: *mut Box<dyn FileOps> = data as *mut _;
    let ctx = ctx.as_mut().unwrap();
    let read_only = *size == 0;
    match ctx.open(file_id, read_only) {
        Ok(file_size) => {
            if read_only {
                *size = file_size as u32;
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

unsafe extern "C" fn file_read(data: *mut c_void, buf: *mut c_void, size: u32, offset: u32) -> i32 {
    if buf.is_null() {
        return -1;
    }
    let ctx = unsafe { &*(data as *const Box<dyn FileOps>) };
    let buf = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, size as usize) };
    match ctx.offset_read(buf, offset as u64) {
        Ok(len) => {
            if len > buf.len() {
                #[cfg(any(feature = "log", feature = "defmt"))]
                error!("file_read: callback returned {} bytes for buffer of size {}", len, buf.len());
                return -1;
            }
            len as i32
        }
        Err(_e) => {
            #[cfg(any(feature = "log", feature = "defmt"))]
            error!("file_read: {:?}", _e);
            -1
        }
    }
}

unsafe extern "C" fn file_write(
    data: *mut c_void,
    buf: *const c_void,
    size: u32,
    offset: u32,
) -> i32 {
    let ctx: *mut Box<dyn FileOps> = data as *mut _;
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
    let ctx: *mut Box<dyn FileOps> = data as *mut _;
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
pub(crate) struct OwnedFileOps(*mut Box<dyn FileOps>);

// The box holds a dyn FileOps, and FileOps: Send.
unsafe impl Send for OwnedFileOps {}

impl OwnedFileOps {
    pub(crate) fn new(ops: Box<dyn FileOps>) -> Self {
        Self(Box::into_raw(Box::new(ops)))
    }

    pub(crate) fn data(&self) -> *mut c_void {
        self.0 as *mut c_void
    }

    pub(crate) fn as_raw(&self) -> libosdp_sys::osdp_file_ops {
        libosdp_sys::osdp_file_ops {
            arg: self.data(),
            open: Some(file_open),
            read: Some(file_read),
            write: Some(file_write),
            close: Some(file_close),
            finalize: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingRead;

    impl FileOps for FailingRead {
        fn open(&mut self, _: i32, _: bool) -> core::result::Result<usize, crate::FileError> {
            Ok(0)
        }
        fn offset_read(&self, _: &mut [u8], _: u64) -> core::result::Result<usize, crate::FileError> {
            Err(crate::FileError::Io)
        }
        fn offset_write(&self, _: &[u8], _: u64) -> core::result::Result<usize, crate::FileError> {
            Ok(0)
        }
        fn close(&mut self) -> core::result::Result<(), crate::FileError> {
            Ok(())
        }
    }

    struct OverReportingRead;

    impl FileOps for OverReportingRead {
        fn open(&mut self, _: i32, _: bool) -> core::result::Result<usize, crate::FileError> {
            Ok(0)
        }
        fn offset_read(&self, buf: &mut [u8], _: u64) -> core::result::Result<usize, crate::FileError> {
            Ok(buf.len() + 1)
        }
        fn offset_write(&self, _: &[u8], _: u64) -> core::result::Result<usize, crate::FileError> {
            Ok(0)
        }
        fn close(&mut self) -> core::result::Result<(), crate::FileError> {
            Ok(())
        }
    }

    #[test]
    fn failed_read_reports_error_and_writes_nothing() {
        let ops = OwnedFileOps::new(Box::new(FailingRead));
        let mut buf = [0xEEu8; 8];
        let rc = unsafe { file_read(ops.data(), buf.as_mut_ptr().cast(), 8u32, 0u32) };
        assert_eq!(rc, -1);
        assert_eq!(buf, [0xEE; 8]);
    }

    #[test]
    fn file_read_rejects_over_reported_count() {
        let ops = OwnedFileOps::new(Box::new(OverReportingRead));
        let mut buf = [0xEEu8; 8];
        let rc = unsafe { file_read(ops.data(), buf.as_mut_ptr().cast(), 8u32, 0u32) };
        assert_eq!(rc, -1);
    }
}
