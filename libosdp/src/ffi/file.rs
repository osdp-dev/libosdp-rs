//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::{FileOps, Finalize, OpenMode};
use alloc::boxed::Box;
use core::ffi::c_void;
#[cfg(feature = "defmt")]
use defmt::error;
#[cfg(all(feature = "log", not(feature = "defmt")))]
use log::error;

/// Which end of a transfer a context's file ops serve. A CP only ever sends
/// files and a PD only ever receives them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileRole {
    Sender,
    Receiver,
}

/// What LibOSDP's `arg` points to.
struct FileOpsArg {
    ops: Box<dyn FileOps>,
    role: FileRole,
}

/// # Safety
///
/// `data` is the `arg` of an [`OwnedFileOps`] that is still alive, and no
/// other reference to it is live.
unsafe fn file_ops_arg<'a>(data: *mut c_void) -> &'a mut FileOpsArg {
    unsafe { &mut *(data as *mut FileOpsArg) }
}

unsafe extern "C" fn file_open(data: *mut c_void, file_id: i32, size: *mut u32) -> i32 {
    if size.is_null() {
        return -1;
    }
    let arg = unsafe { file_ops_arg(data) };
    let mode = match arg.role {
        FileRole::Sender => OpenMode::Read,
        FileRole::Receiver => OpenMode::Write {
            size: unsafe { *size },
        },
    };
    match arg.ops.open(file_id, mode) {
        Ok(file_size) => {
            if mode == OpenMode::Read {
                unsafe { *size = file_size };
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
    let arg = unsafe { file_ops_arg(data) };
    let buf = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, size as usize) };
    match arg.ops.read(buf, offset) {
        Ok(len) if len <= buf.len() => len as i32,
        Ok(_len) => {
            #[cfg(any(feature = "log", feature = "defmt"))]
            error!(
                "file_read: callback returned {} bytes for buffer of size {}",
                _len,
                buf.len()
            );
            -1
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
    if buf.is_null() {
        return -1;
    }
    let arg = unsafe { file_ops_arg(data) };
    let buf = unsafe { core::slice::from_raw_parts(buf as *const u8, size as usize) };
    match arg.ops.write(buf, offset) {
        Ok(len) if len <= buf.len() => len as i32,
        Ok(_len) => {
            #[cfg(any(feature = "log", feature = "defmt"))]
            error!("file_write: callback wrote {} bytes of {}", _len, buf.len());
            -1
        }
        Err(_e) => {
            #[cfg(any(feature = "log", feature = "defmt"))]
            error!("file_write: {:?}", _e);
            -1
        }
    }
}

unsafe extern "C" fn file_close(data: *mut c_void) -> i32 {
    let arg = unsafe { file_ops_arg(data) };
    match arg.ops.close() {
        Ok(_) => 0,
        Err(_e) => {
            #[cfg(any(feature = "log", feature = "defmt"))]
            error!("file_close: {:?}", _e);
            -1
        }
    }
}

unsafe extern "C" fn file_finalize(
    data: *mut c_void,
    outcome: *mut libosdp_sys::osdp_mp_outcome,
    delay_ms: *mut u16,
) -> bool {
    if outcome.is_null() || delay_ms.is_null() {
        return true;
    }
    let arg = unsafe { file_ops_arg(data) };
    match arg.ops.finalize() {
        Finalize::Done(done) => {
            unsafe { *outcome = done.into() };
            true
        }
        Finalize::Pending { retry_after } => {
            let millis = u16::try_from(retry_after.as_millis()).unwrap_or(u16::MAX);
            unsafe { *delay_ms = millis };
            false
        }
    }
}

/// File ops whose box is shared with LibOSDP; freed when this is dropped,
/// which must happen only after LibOSDP stops using it.
pub(crate) struct OwnedFileOps(*mut FileOpsArg);

// The box holds a dyn FileOps, and FileOps: Send.
unsafe impl Send for OwnedFileOps {}

impl OwnedFileOps {
    pub(crate) fn new(ops: Box<dyn FileOps>, role: FileRole) -> Self {
        Self(Box::into_raw(Box::new(FileOpsArg { ops, role })))
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
            finalize: match unsafe { &*self.0 }.role {
                FileRole::Sender => None,
                FileRole::Receiver => Some(file_finalize),
            },
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
    use crate::FileError;

    #[derive(Default)]
    struct Scripted {
        expect_open: Option<OpenMode>,
        read: Option<core::result::Result<usize, FileError>>,
        written: Option<core::result::Result<usize, FileError>>,
    }

    impl FileOps for Scripted {
        fn open(&mut self, _: i32, mode: OpenMode) -> core::result::Result<u32, FileError> {
            match self.expect_open {
                Some(expected) if expected != mode => Err(FileError::Io),
                _ => Ok(1234),
            }
        }
        fn read(&mut self, buf: &mut [u8], _: u32) -> core::result::Result<usize, FileError> {
            self.read.unwrap_or(Ok(buf.len()))
        }
        fn write(&mut self, buf: &[u8], _: u32) -> core::result::Result<usize, FileError> {
            self.written.unwrap_or(Ok(buf.len()))
        }
        fn close(&mut self) -> core::result::Result<(), FileError> {
            Ok(())
        }
    }

    fn owned(ops: Scripted, role: FileRole) -> OwnedFileOps {
        OwnedFileOps::new(Box::new(ops), role)
    }

    #[test]
    fn open_reads_on_the_sender_and_writes_on_the_receiver() {
        let sender = owned(
            Scripted {
                expect_open: Some(OpenMode::Read),
                ..Scripted::default()
            },
            FileRole::Sender,
        );
        let mut size = 0u32;
        assert_eq!(unsafe { file_open(sender.data(), 1, &mut size) }, 0);
        assert_eq!(size, 1234);

        let receiver = owned(
            Scripted {
                expect_open: Some(OpenMode::Write { size: 0 }),
                ..Scripted::default()
            },
            FileRole::Receiver,
        );
        let mut size = 0u32;
        assert_eq!(unsafe { file_open(receiver.data(), 1, &mut size) }, 0);
        assert_eq!(size, 0);
    }

    #[test]
    fn failed_read_reports_error_and_writes_nothing() {
        let ops = owned(
            Scripted {
                read: Some(Err(FileError::Io)),
                ..Scripted::default()
            },
            FileRole::Sender,
        );
        let mut buf = [0xEEu8; 8];
        let rc = unsafe { file_read(ops.data(), buf.as_mut_ptr().cast(), 8u32, 0u32) };
        assert_eq!(rc, -1);
        assert_eq!(buf, [0xEE; 8]);
    }

    #[test]
    fn file_read_rejects_over_reported_count() {
        let ops = owned(
            Scripted {
                read: Some(Ok(9)),
                ..Scripted::default()
            },
            FileRole::Sender,
        );
        let mut buf = [0xEEu8; 8];
        let rc = unsafe { file_read(ops.data(), buf.as_mut_ptr().cast(), 8u32, 0u32) };
        assert_eq!(rc, -1);
    }

    #[test]
    fn file_write_rejects_over_reported_count() {
        let ops = owned(
            Scripted {
                written: Some(Ok(9)),
                ..Scripted::default()
            },
            FileRole::Receiver,
        );
        let buf = [0x55u8; 8];
        let rc = unsafe { file_write(ops.data(), buf.as_ptr().cast(), 8u32, 0u32) };
        assert_eq!(rc, -1);
    }

    #[test]
    fn file_write_rejects_a_null_buffer() {
        let ops = owned(Scripted::default(), FileRole::Receiver);
        let rc = unsafe { file_write(ops.data(), core::ptr::null(), 8u32, 0u32) };
        assert_eq!(rc, -1);
    }

    struct Finalizing(Finalize);

    impl FileOps for Finalizing {
        fn open(&mut self, _: i32, _: OpenMode) -> core::result::Result<u32, FileError> {
            Ok(0)
        }
        fn read(&mut self, _: &mut [u8], _: u32) -> core::result::Result<usize, FileError> {
            Ok(0)
        }
        fn write(&mut self, buf: &[u8], _: u32) -> core::result::Result<usize, FileError> {
            Ok(buf.len())
        }
        fn close(&mut self) -> core::result::Result<(), FileError> {
            Ok(())
        }
        fn finalize(&mut self) -> Finalize {
            self.0
        }
    }

    #[test]
    fn finalize_reports_the_outcome_or_asks_again_later() {
        let mut outcome = libosdp_sys::osdp_mp_outcome_OSDP_MP_OUTCOME_OK;
        let mut delay = 0u16;
        let done = OwnedFileOps::new(
            Box::new(Finalizing(Finalize::Done(crate::MpOutcome::Invalid))),
            FileRole::Receiver,
        );
        assert!(unsafe { file_finalize(done.data(), &mut outcome, &mut delay) });
        assert_eq!(
            outcome,
            libosdp_sys::osdp_mp_outcome_OSDP_MP_OUTCOME_INVALID
        );

        let pending = OwnedFileOps::new(
            Box::new(Finalizing(Finalize::Pending {
                retry_after: core::time::Duration::from_secs(70),
            })),
            FileRole::Receiver,
        );
        assert!(!unsafe { file_finalize(pending.data(), &mut outcome, &mut delay) });
        assert_eq!(delay, u16::MAX);
    }

    #[test]
    fn only_a_receiver_is_asked_to_finalize() {
        let sender = OwnedFileOps::new(
            Box::new(Finalizing(Finalize::Done(crate::MpOutcome::Ok))),
            FileRole::Sender,
        );
        assert!(sender.as_raw().finalize.is_none());
        let receiver = OwnedFileOps::new(
            Box::new(Finalizing(Finalize::Done(crate::MpOutcome::Ok))),
            FileRole::Receiver,
        );
        assert!(receiver.as_raw().finalize.is_some());
    }
}
