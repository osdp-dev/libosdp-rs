//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
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

/// How a submitted command or event ended, when it did not succeed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CompletionError {
    /// The peer refused it, the link failed while it was in flight, or the
    /// operation it started (a file transfer, say) did not finish.
    #[cfg_attr(feature = "std", error("failed"))]
    Failed,

    /// A flush dropped it before it was sent, or, for a transparent reader
    /// command, the card session it belonged to ended before it was sent:
    /// LibOSDP then drops the rest of the session's commands.
    #[cfg_attr(feature = "std", error("flushed before it was sent"))]
    Flushed,

    /// The context was torn down before it was sent. A [`Submission`]
    /// cannot outlive its context, so it never reports this.
    ///
    /// [`Submission`]: crate::Submission
    #[cfg_attr(feature = "std", error("aborted by teardown"))]
    Aborted,
}

/// Why a PD refuses a command. LibOSDP answers the CP with the matching
/// NAK; it reports protocol faults with the other NAK codes itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Nak {
    /// The requested biometric type is not supported.
    BioType,
    /// The requested biometric format is not supported.
    BioFormat,
    /// The command could not be processed.
    Record,
}

impl Nak {
    pub(crate) fn as_c(self) -> i32 {
        let code = match self {
            Nak::BioType => libosdp_sys::osdp_pd_nak_code_e_OSDP_PD_NAK_BIO_TYPE,
            Nak::BioFormat => libosdp_sys::osdp_pd_nak_code_e_OSDP_PD_NAK_BIO_FMT,
            Nak::Record => libosdp_sys::osdp_pd_nak_code_e_OSDP_PD_NAK_RECORD,
        };
        -(code as i32)
    }
}

/// Errors a [`FileOps`](crate::FileOps) implementation reports to LibOSDP.
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
