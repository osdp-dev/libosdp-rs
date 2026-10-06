//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! OSDP provides a means to send files from CP to a Peripheral Device (PD).
//! This module adds the required components to achieve this effect.

use crate::MpOutcome;
use core::time::Duration;

type Result<T> = core::result::Result<T, crate::FileError>;

/// How far a file transfer has got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FileProgress {
    /// Size of the file, as the sender's [`FileOps::open`] reported it.
    pub size: u32,
    /// Bytes sent or received so far.
    pub offset: u32,
}

/// Which way a file is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum OpenMode {
    /// To be read and sent: a CP sending a file to a PD.
    Read,
    /// To be received and written: a PD receiving a file of `size` bytes
    /// from the CP.
    Write {
        /// Size of the incoming file, as the CP announced it.
        size: u32,
    },
}

/// Whether a PD has finished with a file it received.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Finalize {
    /// Finished, with this outcome for the CP.
    Done(MpOutcome),
    /// Still working -- verifying or installing the file, say. The CP is
    /// asked to check back after `retry_after`, a hint carried in whole
    /// milliseconds up to 65.535 s, and [`FileOps::finalize`] is called
    /// again then.
    Pending {
        /// How long the CP should wait before asking again.
        retry_after: Duration,
    },
}

/// File operations handler trait. Any type that implements this trait can be
/// registered with [`crate::ControlPanel::register_file_ops`] or
/// [`crate::PeripheralDevice::register_file_ops`].
///
/// LibOSDP calls these from inside the context's `refresh()`, from
/// [`ControlPanel::submit`](crate::ControlPanel::submit) when it starts a
/// transfer, and `close` from `register_file_ops` when it replaces ops with a
/// file open, and when the context is dropped, so they must return promptly.
///
/// Implementors are moved into a `Send` context, so they must be `Send`:
///
/// ```compile_fail
/// struct NotSend(std::rc::Rc<()>);
/// impl libosdp::FileOps for NotSend {
///     fn open(&mut self, _: i32, _: libosdp::OpenMode) -> Result<u32, libosdp::FileError> {
///         Ok(0)
///     }
///     fn read(&mut self, _: &mut [u8], _: u32) -> Result<usize, libosdp::FileError> {
///         Ok(0)
///     }
///     fn write(&mut self, _: &[u8], _: u32) -> Result<usize, libosdp::FileError> {
///         Ok(0)
///     }
///     fn close(&mut self) -> Result<(), libosdp::FileError> {
///         Ok(())
///     }
/// }
/// ```
pub trait FileOps: Send {
    /// Open the file with the pre-agreed File-ID `id`. When opening to
    /// [`OpenMode::Read`], return the size of the file; when opening to
    /// [`OpenMode::Write`], the returned size is not used.
    fn open(&mut self, id: i32, mode: OpenMode) -> Result<u32>;

    /// Read into `buf` from `offset` bytes into the open file; returns the
    /// number of bytes read, at most `buf.len()`.
    fn read(&mut self, buf: &mut [u8], offset: u32) -> Result<usize>;

    /// Write `buf` at `offset` bytes into the open file; returns the number
    /// of bytes written. Writing less than all of `buf` fails the transfer.
    fn write(&mut self, buf: &[u8], offset: u32) -> Result<usize>;

    /// Close the open file.
    fn close(&mut self) -> Result<()>;

    /// Decide how a received transfer ends; PD only. Called once the last
    /// byte has been written, and again while it returns
    /// [`Finalize::Pending`], each time from inside
    /// [`PeripheralDevice::refresh`](crate::PeripheralDevice::refresh), so
    /// it must return promptly: do long work elsewhere and report here
    /// whether it is done. While it is pending, a CP that missed a reply
    /// may send its last chunk again, and [`FileOps::write`] gets it again.
    ///
    /// By default, the transfer ends at once with [`MpOutcome::Ok`].
    fn finalize(&mut self) -> Finalize {
        Finalize::Done(MpOutcome::Ok)
    }
}
