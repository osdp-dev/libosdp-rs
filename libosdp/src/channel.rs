//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! The OSDP specification defines that communication between OSDP devices
//! happen over an RS-485 connection. For testing and development purpose this
//! can be limiting so LibOSDP defines a notion called "Channel" which is a
//! software representation (abstraction) of the physical transport medium.
//!
//! Since RS-485 is stream based protocol, we can think of it to be  something
//! that we can read from and write to (which in turn is Read and Write traits
//! in rust). This allows us to run OSDP devices over many IPC schemes such as
//! Unix socket and message queues.
//!
//! This module provides a way to define an OSDP channel and export it to
//! LibOSDP.

/// OSDP channel errors
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ChannelError {
    /// Channel is temporarily unavailable (could have blocked until it was
    /// ready but LibOSDP required channel to be non-blocking so return "I would
    /// have blocked" instead)
    WouldBlock,
    /// Channel failed irrecoverably.
    TransportError,
}

#[cfg(feature = "std")]
impl From<std::io::Error> for ChannelError {
    fn from(value: std::io::Error) -> Self {
        match value.kind() {
            std::io::ErrorKind::WouldBlock => ChannelError::WouldBlock,
            _ => ChannelError::TransportError,
        }
    }
}

#[cfg(not(feature = "std"))]
impl<E: embedded_io::Error + Sized> From<E> for ChannelError {
    fn from(value: E) -> Self {
        match value.kind() {
            _ => ChannelError::TransportError,
        }
    }
}

/// The Channel trait acts as an interface for all channel implementors. See
/// module description for the definition of a "channel" in LibOSDP.
pub trait Channel: Send {
    /// Pull as many bytes into buffer as possible without blocking; returns
    /// the number of bytes read.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ChannelError>;

    /// Queue all of `buf` for sending, or none of it, without blocking.
    /// [`ChannelError::WouldBlock`] asks LibOSDP to retry the same packet on
    /// its next refresh; any other error drops the packet.
    fn write(&mut self, buf: &[u8]) -> Result<(), ChannelError>;

    /// Discard whatever is buffered, in either direction.
    fn flush(&mut self) {}

    /// Called once, when the context that owns this channel is dropped.
    fn close(&mut self) {}
}

impl core::fmt::Debug for dyn Channel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Channel").finish_non_exhaustive()
    }
}
