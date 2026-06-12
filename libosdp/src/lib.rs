//
// Copyright (c) 2023-2024 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]
//! # LibOSDP - Open Supervised Device Protocol Library
//!
//! This is a cross-platform open source implementation of IEC 60839-11-5 Open Supervised Device
//! Protocol (OSDP). The protocol is intended to improve [interoperability][5] among access control
//! and security products. It supports [Secure Channel (SC)][6] for encrypted and authenticated
//! communication between configured devices.
//!
//! OSDP describes the communication protocol for interfacing one or more Peripheral Devices (PD)
//! to a Control Panel (CP) over a two-wire RS-485 multi-drop serial communication channel.
//! Nevertheless, this protocol can be used to transfer secure data over any stream based physical
//! channel. Read more about OSDP [here][1].
//!
//! This protocol is developed and maintained by [Security Industry Association][2] (SIA).
//!
//! ## Getting started
//!
//! A device complying with OSDP can either be a CP or a PD. There can be only one CP on a bus
//! which can talk to multiple PDs. LibOSDP allows your application to work either as a CP or a
//! PD so depending on what you want to do you have to do some things differently.
//!
//! LibOSDP creates the following constructs which allow interactions between devices on the OSDP
//! bus. These should not be confused with the protocol specified terminologies that may use the
//! same names. They are:
//!   - Channel - Something that allows two OSDP devices to talk to each other
//!   - Commands - A call for action from a control panel (CP) to peripheral device (PD)
//!   - Events - A call for action from peripheral device (PD) to control panel (CP)
//!
//! The app starts by defining a type that implements the [`Channel`] trait; this allows your
//! devices to communicate with other osdp devices on the bus. Then you describe the PD you are
//!   - talking to on the bus (in case of CP mode of operation) or,
//!   - going to behave as on the bus (in case of PD mode of operation)
//! by using the [`PdInfo`] struct.
//!
//! You can use the `PdInfo` (or a vector of `PdInfo` structs in case of CP mode) to create a
//! [`ControlPanel`] or [`PeripheralDevice`] context. Both these contexts have a non-blocking
//! method `refresh()` that needs to called as frequently as your app can permit. To meet the OSDP
//! specified timing requirements, your app must call this method at least once every 50ms.
//!
//! After this point, the CP context can,
//!   - send commands to any one of the PDs (to control LEDs, Buzzers, Input/Output pins, etc.,)
//!   - register a closure for events that are sent from a PD
//!
//! and the PD context can,
//!   - notify it's controlling CP about an event (card read, key press, tamper, etc.,)
//!   - register a closure for commands issued by the CP
//!
//! You can find a template implementation for CP app [here][3] and PD app [here][4].
//!
//! [1]: https://doc.osdp.dev/protocol/
//! [2]: https://www.securityindustry.org/industry-standards/open-supervised-device-protocol/
//! [3]: https://docs.rs/crate/libosdp/latest/source/examples/cp.rs
//! [4]: https://docs.rs/crate/libosdp/latest/source/examples/pd.rs
//! [5]: https://doc.osdp.dev/libosdp/compatibility
//! [6]: https://doc.osdp.dev/libosdp/secure-channel

#![warn(missing_debug_implementations)]
#![warn(rust_2018_idioms)]
#![warn(missing_docs)]

extern crate alloc;

mod channel;
mod commands;
mod cp;
mod events;
mod file;
mod pd;
mod pdcap;
mod pdid;
mod pdinfo;

// Re-export for convenience
pub use channel::*;
pub use commands::*;
pub use events::*;
pub use file::*;
pub use pdcap::*;
pub use pdid::*;
pub use pdinfo::*;

#[allow(unused_imports)]
use alloc::{borrow::ToOwned, boxed::Box, format, string::String};

#[cfg(feature = "std")]
use thiserror::Error;

pub use cp::{ControlPanel, ControlPanelBuilder};
pub use pd::PeripheralDevice;

/// OSDP public errors
#[derive(Debug, Default)]
#[cfg_attr(feature = "std", derive(Error))]
pub enum OsdpError {
    /// PD info error
    #[cfg_attr(feature = "std", error("Invalid PdInfo {0}"))]
    PdInfo(&'static str),

    /// Command build/send error
    #[cfg_attr(feature = "std", error("Invalid OsdpCommand"))]
    Command,

    /// Event build/send error
    #[cfg_attr(feature = "std", error("Invalid OsdpEvent"))]
    Event,

    /// PD/CP status query error
    #[cfg_attr(feature = "std", error("Failed to query {0} from device"))]
    Query(&'static str),

    /// File transfer errors
    #[cfg_attr(feature = "std", error("File transfer failed: {0}"))]
    FileTransfer(&'static str),

    /// CP/PD device setup failed.
    #[cfg_attr(feature = "std", error("Failed to setup device"))]
    Setup,

    /// String parse error
    #[cfg_attr(feature = "std", error("Type {0} parse error"))]
    Parse(String),

    /// OSDP channel error
    #[cfg_attr(feature = "std", error("Channel error: {0}"))]
    Channel(&'static str),

    /// String conversion error
    #[cfg_attr(feature = "std", error("PD info build error: {0}"))]
    PdInfoBuilder(&'static str),

    /// IO Error
    #[cfg(feature = "std")]
    #[error("IO Error")]
    IO(#[from] std::io::Error),
    /// IO Error
    #[cfg(not(feature = "std"))]
    IO(Box<dyn embedded_io::Error>),

    /// Unknown error
    #[default]
    #[cfg_attr(feature = "std", error("Unknown/Unspecified error"))]
    Unknown,
}

#[cfg(feature = "defmt")]
impl defmt::Format for OsdpError {
    fn format(&self, f: defmt::Formatter<'_>) {
        match self {
            OsdpError::PdInfo(e) => defmt::write!(f, "OsdpError::PdInfo({0})", e),
            OsdpError::Command => defmt::write!(f, "OsdpError::Command"),
            OsdpError::Event => defmt::write!(f, "OsdpError::Event"),
            OsdpError::Query(e) => defmt::write!(f, "OsdpError::Query({0})", e),
            OsdpError::FileTransfer(e) => defmt::write!(f, "OsdpError::FileTransfer({0})", e),
            OsdpError::Setup => defmt::write!(f, "OsdpError::Setup"),
            OsdpError::Parse(e) => defmt::write!(f, "OsdpError::Parse({0})", e.as_str()),
            OsdpError::Channel(e) => defmt::write!(f, "OsdpError::Channel({0})", e),
            OsdpError::PdInfoBuilder(e) => defmt::write!(f, "OsdpError::PdInfoBuilder({0})", e),
            OsdpError::IO(_) => defmt::write!(f, "OsdpError::IO"), // Error cannot be formatted, because there is no way to set defmt::Format as a bound
            OsdpError::Unknown => defmt::write!(f, "OsdpError::Unknown"),
        }
    }
}

impl From<core::convert::Infallible> for OsdpError {
    fn from(_: core::convert::Infallible) -> Self {
        unreachable!()
    }
}

impl From<ChannelError> for OsdpError {
    fn from(value: ChannelError) -> OsdpError {
        match value {
            ChannelError::WouldBlock => OsdpError::Channel("WouldBlock"),
            ChannelError::TransportError => OsdpError::Channel("TransportError"),
        }
    }
}

/// Trait to convert between BigEndian and LittleEndian types
pub trait ConvertEndian {
    /// Return `Self` as BigEndian
    fn as_be(&self) -> u32;
    /// Return `Self` as LittleEndian
    fn as_le(&self) -> u32;
}

bitflags::bitflags! {
    /// OSDP setup flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct OsdpFlag: u32 {
        /// Make security conscious assumptions where possible. Fail where these
        /// assumptions don't hold. The following restrictions are enforced in
        /// this mode:
        ///
        /// - Don't allow use of SCBK-D (implies no INSTALL_MODE)
        /// - Assume that a KEYSET was successful at an earlier time
        /// - Disallow master key based SCBK derivation
        const EnforceSecure = libosdp_sys::OSDP_FLAG_ENFORCE_SECURE;

        /// When set, the PD would allow one session of secure channel to be
        /// setup with SCBK-D.
        ///
        /// In this mode, the PD is in a vulnerable state, the application is
        /// responsible for making sure that the device enters this mode only
        /// during controlled/provisioning-time environments.
        const InstallMode = libosdp_sys::OSDP_FLAG_INSTALL_MODE;

        /// When set, CP will not error and fail when the PD sends an unknown,
        /// unsolicited response. In PD mode this flag has no use.
        const IgnoreUnsolicited = libosdp_sys::OSDP_FLAG_IGN_UNSOLICITED;
    }
}

impl core::str::FromStr for OsdpFlag {
    type Err = OsdpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "EnforceSecure" => Ok(OsdpFlag::EnforceSecure),
            "InstallMode" => Ok(OsdpFlag::InstallMode),
            "IgnoreUnsolicited" => Ok(OsdpFlag::IgnoreUnsolicited),
            _ => Err(OsdpError::Parse(format!("OsdpFlag: {s}"))),
        }
    }
}

#[allow(dead_code)]
fn cstr_to_string(s: *const ::core::ffi::c_char) -> String {
    let s = unsafe { core::ffi::CStr::from_ptr(s) };
    s.to_str().unwrap().to_owned()
}

/// Get LibOSDP version
pub fn get_version() -> &'static str {
    let s = unsafe { libosdp_sys::osdp_get_version() };
    let s = unsafe { core::ffi::CStr::from_ptr(s) };
    s.to_str().unwrap()
}

/// Get LibOSDP source info string
pub fn get_source_info() -> &'static str {
    let s = unsafe { libosdp_sys::osdp_get_source_info() };
    let s = unsafe { core::ffi::CStr::from_ptr(s) };
    s.to_str().unwrap()
}
