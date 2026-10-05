//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Errors returned by this crate and by the traits an application implements.

use crate::ChannelError;

/// Errors returned by this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// LibOSDP refused to set up the context.
    #[cfg_attr(feature = "std", error("LibOSDP refused to set up the context"))]
    Setup,

    /// An argument is outside what LibOSDP or OSDP accepts.
    #[cfg_attr(feature = "std", error("invalid argument: {0}"))]
    InvalidArgument(&'static str),

    /// LibOSDP refused the request.
    #[cfg_attr(feature = "std", error("LibOSDP refused the request"))]
    Rejected,

    /// The operation queried or cancelled is not in progress.
    #[cfg_attr(feature = "std", error("operation not in progress"))]
    NotInProgress,

    /// A payload does not fit the LibOSDP buffer that carries it.
    #[cfg_attr(feature = "std", error("payload longer than {max} bytes"))]
    TooLong {
        /// The most the buffer holds.
        max: usize,
    },

    /// LibOSDP handed over a value this crate cannot represent.
    #[cfg_attr(feature = "std", error("malformed {0}"))]
    Malformed(&'static str),

    /// The channel failed.
    #[cfg_attr(feature = "std", error("channel error: {0:?}"))]
    Channel(ChannelError),
}

impl From<ChannelError> for Error {
    fn from(value: ChannelError) -> Self {
        Error::Channel(value)
    }
}

impl From<core::convert::Infallible> for Error {
    fn from(value: core::convert::Infallible) -> Self {
        match value {}
    }
}

/// Errors a [`OsdpFileOps`](crate::OsdpFileOps) implementation reports to LibOSDP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FileError {
    /// No file is known by the requested id.
    #[cfg_attr(feature = "std", error("no file with this id"))]
    NotFound,

    /// No file is open.
    #[cfg_attr(feature = "std", error("no file is open"))]
    NotOpen,

    /// The storage behind the file failed.
    #[cfg_attr(feature = "std", error("file I/O failed"))]
    Io,
}

#[cfg(feature = "std")]
impl From<std::io::Error> for FileError {
    fn from(value: std::io::Error) -> Self {
        match value.kind() {
            std::io::ErrorKind::NotFound => FileError::NotFound,
            _ => FileError::Io,
        }
    }
}
