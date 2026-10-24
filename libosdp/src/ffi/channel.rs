//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

use crate::{Channel, ChannelError};
use alloc::boxed::Box;
use core::ffi::c_void;

unsafe extern "C" fn raw_read(data: *mut c_void, buf: *mut u8, len: i32) -> i32 {
    if buf.is_null() || len < 0 {
        return -1;
    }
    let channel = unsafe { &mut *(data as *mut Box<dyn Channel>) };
    let buf = unsafe { core::slice::from_raw_parts_mut(buf, len as usize) };
    match channel.read(buf) {
        Ok(n) => {
            if n > buf.len() {
                return -1;
            }
            n as i32
        }
        Err(ChannelError::WouldBlock) => 0,
        Err(_) => -1,
    }
}

unsafe extern "C" fn raw_write(data: *mut c_void, buf: *mut u8, len: i32) -> i32 {
    if buf.is_null() || len < 0 {
        return -1;
    }
    let channel = unsafe { &mut *(data as *mut Box<dyn Channel>) };
    let buf = unsafe { core::slice::from_raw_parts(buf, len as usize) };
    match channel.write(buf) {
        Ok(()) => len,
        Err(ChannelError::WouldBlock) => 0,
        Err(_) => -1,
    }
}

unsafe extern "C" fn raw_flush(data: *mut c_void) {
    let channel = unsafe { &mut *(data as *mut Box<dyn Channel>) };
    channel.flush();
}

unsafe extern "C" fn raw_close(data: *mut c_void) {
    let channel = unsafe { &mut *(data as *mut Box<dyn Channel>) };
    channel.close();
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

    pub(crate) fn as_raw(&self) -> libosdp_sys::osdp_channel {
        libosdp_sys::osdp_channel {
            data: self.data(),
            recv: Some(raw_read),
            send: Some(raw_write),
            flush: Some(raw_flush),
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

#[cfg(test)]
mod tests {
    use super::*;

    struct TwoBytes;

    impl Channel for TwoBytes {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, ChannelError> {
            buf[..2].copy_from_slice(&[0x11, 0x22]);
            Ok(2)
        }
        fn write(&mut self, _: &[u8]) -> Result<(), ChannelError> {
            Ok(())
        }
    }

    struct OverReportingRead;

    impl Channel for OverReportingRead {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, ChannelError> {
            Ok(buf.len() + 1)
        }
        fn write(&mut self, _: &[u8]) -> Result<(), ChannelError> {
            Ok(())
        }
    }

    struct PanicOnUse;

    impl Channel for PanicOnUse {
        fn read(&mut self, _: &mut [u8]) -> Result<usize, ChannelError> {
            panic!("read should not be called with a bad buffer")
        }
        fn write(&mut self, _: &[u8]) -> Result<(), ChannelError> {
            panic!("write should not be called with a bad buffer")
        }
    }

    struct Full;

    impl Channel for Full {
        fn read(&mut self, _: &mut [u8]) -> Result<usize, ChannelError> {
            Err(ChannelError::WouldBlock)
        }
        fn write(&mut self, _: &[u8]) -> Result<(), ChannelError> {
            Err(ChannelError::WouldBlock)
        }
    }

    #[test]
    fn read_leaves_bytes_past_the_count_untouched() {
        let channel = OwnedChannel::new(Box::new(TwoBytes));
        let mut buf = [0xEEu8; 8];
        let n = unsafe { raw_read(channel.data(), buf.as_mut_ptr(), buf.len() as i32) };
        assert_eq!(n, 2);
        assert_eq!(buf, [0x11, 0x22, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE]);
    }

    #[test]
    fn read_rejects_over_reported_count() {
        let channel = OwnedChannel::new(Box::new(OverReportingRead));
        let mut buf = [0xEEu8; 8];
        let n = unsafe { raw_read(channel.data(), buf.as_mut_ptr(), buf.len() as i32) };
        assert_eq!(n, -1);
    }

    #[test]
    fn read_rejects_negative_len() {
        let channel = OwnedChannel::new(Box::new(PanicOnUse));
        let mut buf = [0xEEu8; 8];
        let n = unsafe { raw_read(channel.data(), buf.as_mut_ptr(), -1) };
        assert_eq!(n, -1);
    }

    #[test]
    fn write_reports_the_whole_packet_queued() {
        let channel = OwnedChannel::new(Box::new(TwoBytes));
        let mut buf = [0x55u8; 8];
        let n = unsafe { raw_write(channel.data(), buf.as_mut_ptr(), buf.len() as i32) };
        assert_eq!(n, 8);
    }

    #[test]
    fn write_that_would_block_asks_for_a_retry() {
        let channel = OwnedChannel::new(Box::new(Full));
        let mut buf = [0x55u8; 8];
        let n = unsafe { raw_write(channel.data(), buf.as_mut_ptr(), buf.len() as i32) };
        assert_eq!(n, 0);
    }

    #[test]
    fn write_rejects_a_bad_buffer() {
        let channel = OwnedChannel::new(Box::new(PanicOnUse));
        let mut buf = [0x55u8; 8];
        let n = unsafe { raw_write(channel.data(), core::ptr::null_mut(), 8) };
        assert_eq!(n, -1);
        let n = unsafe { raw_write(channel.data(), buf.as_mut_ptr(), -1) };
        assert_eq!(n, -1);
    }
}
