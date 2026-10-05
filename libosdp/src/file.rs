//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! OSDP provides a means to send files from CP to a Peripheral Device (PD).
//! This module adds the required components to achieve this effect.

type Result<T> = core::result::Result<T, crate::FileError>;

/// File operations handler trait. Any type that implements this trait can be
/// registered with [`crate::ControlPanel::register_file_ops`] or
/// [`crate::PeripheralDevice::register_file_ops`].
///
/// Implementors are moved into a `Send` context, so they must be `Send`:
///
/// ```compile_fail
/// struct NotSend(std::rc::Rc<()>);
/// impl libosdp::FileOps for NotSend {
///     fn open(&mut self, _: i32, _: bool) -> Result<usize, libosdp::FileError> { Ok(0) }
///     fn offset_read(&self, _: &mut [u8], _: u64) -> Result<usize, libosdp::FileError> { Ok(0) }
///     fn offset_write(&self, _: &[u8], _: u64) -> Result<usize, libosdp::FileError> { Ok(0) }
///     fn close(&mut self) -> Result<(), libosdp::FileError> { Ok(()) }
/// }
/// ```
pub trait FileOps: Send {
    /// Open a file, with pre-agreed File-ID `id`; returns the size of the
    /// file that was opened.
    fn open(&mut self, id: i32, read_only: bool) -> Result<usize>;
    /// Read bytes into buffer `buf` from offset `off` of the file; returns
    /// number of bytes read.
    fn offset_read(&self, buf: &mut [u8], off: u64) -> Result<usize>;
    /// Write bytes from buffer `buf` at offset `off` of the file; returns
    /// number of bytes written.
    fn offset_write(&self, buf: &[u8], off: u64) -> Result<usize>;
    /// Close the currently open file.
    fn close(&mut self) -> Result<()>;
}
