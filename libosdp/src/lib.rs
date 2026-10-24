//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
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
//! by using a [`PdInfoBuilder`].
//!
//! In CP mode, add each PD's `PdInfoBuilder` to a [`ControlPanelBuilder`], which hands back the
//! [`PdIndex`] that addresses it, and build a [`ControlPanel`]. In PD mode, pass the one
//! `PdInfoBuilder` to [`PeripheralDevice::new`]. Both these contexts have a non-blocking
//! method `refresh()` that needs to called as frequently as your app can permit. To meet the OSDP
//! specified timing requirements, your app must call this method at least once every 50ms.
//!
//! After this point, the CP context can,
//!   - send commands to any one of the PDs (to control LEDs, Buzzers, Input/Output pins, etc.,)
//!   - hand over, after each `refresh()`, the events that PDs sent
//!
//! and the PD context can,
//!   - notify it's controlling CP about an event (card read, key press, tamper, etc.,)
//!   - answer the commands issued by the CP through a [`CommandHandler`]
//!
//! You can find a template implementation for CP app [here][3] and PD app [here][4].
//! With the `async` feature, see [async_cp.rs][7] and [async_pd.rs][8].
//!
//! ## Cargo features
//!
//!   - `std` (default): build against the standard library, implement
//!     `std::error::Error` for the error types, and turn on `log`. Without
//!     it the crate is `no_std` and needs only `alloc`.
//!   - `log`: forward LibOSDP's log messages to the `log` crate.
//!   - `defmt`: forward LibOSDP's log messages to `defmt` instead, and
//!     derive `defmt::Format` for most public types.
//!   - `serde`: derive `Serialize` and `Deserialize` for the public data
//!     types.
//!   - `async`: await a [`Submission`] for how it ended, read CP events
//!     and PD notifications as a `Stream`, and keep a context refreshed
//!     with `run`, on any local executor. Wakers run only after the call
//!     that queued or completed something returns, never inside a
//!     LibOSDP callback.
//!   - `trs`: build LibOSDP's transparent reader support, which
//!     [`Command::Trs`] and [`Event::Trs`] need. Without it, LibOSDP refuses
//!     those commands with [`Error::Rejected`].
//!
//! [1]: https://doc.osdp.dev/protocol/
//! [2]: https://www.securityindustry.org/industry-standards/open-supervised-device-protocol/
//! [3]: https://docs.rs/crate/libosdp/latest/source/examples/cp.rs
//! [4]: https://docs.rs/crate/libosdp/latest/source/examples/pd.rs
//! [5]: https://doc.osdp.dev/libosdp/compatibility
//! [6]: https://doc.osdp.dev/libosdp/secure-channel
//! [7]: https://docs.rs/crate/libosdp/latest/source/examples/async_cp.rs
//! [8]: https://docs.rs/crate/libosdp/latest/source/examples/async_pd.rs

#![warn(missing_debug_implementations)]
#![warn(rust_2018_idioms)]
#![warn(missing_docs)]

extern crate alloc;

mod bio;
mod busy;
mod channel;
mod commands;
mod cp;
mod error;
mod events;
mod ffi;
mod file;
mod metrics;
mod notification;
mod pd;
mod pdcap;
mod pdid;
mod pdinfo;
mod piv;
mod queue;
mod status;
#[cfg(feature = "async")]
mod stream;
mod submit;
mod trs;
#[cfg(feature = "async")]
mod wake;

// Re-export for convenience
pub use channel::*;
pub use commands::*;
pub use events::*;
pub use file::{FileOps, FileProgress, Finalize, OpenMode};
pub use pdcap::{CapFunction, Capability};
pub use pdid::*;
pub use pdinfo::*;
#[cfg(feature = "async")]
pub use stream::{EventStream, NotificationStream};

pub use bio::{
    BioFormat, BioMatchCommand, BioMatchReply, BioReadCommand, BioReadReply, BioStatus, BioType,
};
pub use cp::{ControlPanel, ControlPanelBuilder, CpEvent, PdIndex};
pub use error::{CompletionError, Error, FileError, Nak};
pub use metrics::Metrics;
pub use notification::{MpKind, MpOutcome, MpPhase, Multipart, Notification, TrsSessionStatus};
pub use pd::{CommandHandler, PeripheralDevice, Responder};
pub use piv::{Challenge, PivReply, PivRequest};
pub use status::{ReaderStatus, Status, StatusKind};
pub use submit::Submission;
pub use trs::{
    CardData, CardInfo, CardProtocol, CardStatus, PinBlock, PinEndOn, PinEntry, PinFormat,
    PinLengthField, PinResult, TrsCommand, TrsReply, TrsScan,
};

/// Copy `data` into a zeroed array of `N`, or refuse it if it does not fit.
pub(crate) fn copy_into<const N: usize>(data: &[u8]) -> Result<[u8; N], Error> {
    let mut array = [0; N];
    array
        .get_mut(..data.len())
        .ok_or(Error::TooLong { max: N })?
        .copy_from_slice(data);
    Ok(array)
}

/// Packs bytes into the little-endian `u32` LibOSDP expects.
pub(crate) trait ConvertEndian {
    fn as_le(&self) -> u32;
}

bitflags::bitflags! {
    /// OSDP setup flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Flag: u32 {
        /// Make security conscious assumptions where possible. Fail where these
        /// assumptions don't hold. The following restrictions are enforced in
        /// this mode:
        ///
        /// - Don't allow use of SCBK-D
        /// - Assume that a KEYSET was successful at an earlier time
        /// - CP does not allow user requested broadcast commands
        ///
        /// Cannot be combined with [`Flag::InstallMode`]: setup fails with
        /// [`Error::Setup`].
        const EnforceSecure = libosdp_sys::OSDP_FLAG_ENFORCE_SECURE;

        /// When set, the PD would allow one session of secure channel to be
        /// setup with SCBK-D.
        ///
        /// In this mode, the PD is in a vulnerable state, the application is
        /// responsible for making sure that the device enters this mode only
        /// during controlled/provisioning-time environments.
        ///
        /// Cannot be combined with [`Flag::EnforceSecure`]: setup fails with
        /// [`Error::Setup`].
        const InstallMode = libosdp_sys::OSDP_FLAG_INSTALL_MODE;

        /// When set, CP will not error and fail when the PD sends an unknown,
        /// unsolicited response. In PD mode this flag has no use.
        const IgnoreUnsolicited = libosdp_sys::OSDP_FLAG_IGN_UNSOLICITED;

        /// Report what LibOSDP observes about the link -- secure channel
        /// and online state changes, multi-part progress, the PD's
        /// identity -- as [`Notification`]s.
        const EnableNotification = libosdp_sys::OSDP_FLAG_ENABLE_NOTIFICATION;

        /// Capture the packets this device sees to a pcap file, finished
        /// when the context is dropped. Takes effect only when libosdp-sys
        /// is built with its `packet_trace` or `data_trace` feature.
        const CapturePackets = libosdp_sys::OSDP_FLAG_CAPTURE_PACKETS;

        /// Accept a secure channel message with an empty encrypted data
        /// block, which some non-conforming CPs send. In CP mode this flag
        /// has no use.
        const AllowEmptyEncryptedDataBlock =
            libosdp_sys::OSDP_FLAG_ALLOW_EMPTY_ENCRYPTED_DATA_BLOCK;

        /// Let a biometric read reply span several packets, so templates
        /// larger than one packet can be sent. This is not part of OSDP, so
        /// both the CP and the PD must set it.
        const BioReadMultipart = libosdp_sys::OSDP_FLAG_BIOREADR_MULTIPART;
    }
}

impl core::str::FromStr for Flag {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "EnforceSecure" => Ok(Flag::EnforceSecure),
            "InstallMode" => Ok(Flag::InstallMode),
            "IgnoreUnsolicited" => Ok(Flag::IgnoreUnsolicited),
            "EnableNotification" => Ok(Flag::EnableNotification),
            "CapturePackets" => Ok(Flag::CapturePackets),
            "AllowEmptyEncryptedDataBlock" => Ok(Flag::AllowEmptyEncryptedDataBlock),
            "BioReadMultipart" => Ok(Flag::BioReadMultipart),
            _ => Err(Error::InvalidArgument("unknown flag name")),
        }
    }
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

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    fn assert_serde<T: serde::Serialize + serde::de::DeserializeOwned>() {}

    #[test]
    fn commands_and_events_are_serde() {
        assert_serde::<crate::Command>();
        assert_serde::<crate::Event>();
        assert_serde::<crate::Notification>();
    }
}
