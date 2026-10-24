//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

//! Link health counters LibOSDP keeps for each PD.

/// What happened on the link to one PD since the counters were last taken.
/// Each counter stops at `u32::MAX` rather than wrapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Metrics {
    /// Packets handed whole to the channel.
    pub packets_sent: u32,
    /// Packets received with a well-formed frame, including those that
    /// then failed their check.
    pub packets_received: u32,
    /// Packets received whose CRC or checksum did not match.
    pub check_errors: u32,
    /// NAKs: received from the PD on a CP, sent to the CP on a PD.
    pub naks: u32,
    /// Secure channel sessions that became active: on a CP, when it
    /// accepts the PD's initial R-MAC; on a PD, when it sends it. A
    /// handshake that never completes is not counted.
    pub sc_handshakes: u32,
    /// Active secure channel sessions torn down, for any reason: a bad
    /// MAC, a session timeout, a key change, a link reset, or, on a CP,
    /// the PD going offline or being disabled. A handshake that never
    /// completes is not counted. Provisioning a PD from SCBK-D counts two
    /// handshakes and one failure: the SCBK-D session ends when the new
    /// key is set.
    pub sc_failures: u32,
    /// Commands: on a CP, sent to the PD; on a PD, handed to the
    /// application and accepted by it.
    pub commands: u32,
    /// Events and notifications: on a CP, handed to the application; on
    /// a PD, events whose completion was reported to the application,
    /// and notifications handed to it.
    pub events: u32,
}

impl From<libosdp_sys::osdp_metrics> for Metrics {
    fn from(value: libosdp_sys::osdp_metrics) -> Self {
        Metrics {
            packets_sent: value.packets_sent,
            packets_received: value.packets_received,
            check_errors: value.packet_check_errors,
            naks: value.nak_count,
            sc_handshakes: value.sc_handshake_count,
            sc_failures: value.sc_failure_count,
            commands: value.command_count,
            events: value.event_count,
        }
    }
}

/// Take, and reset, the counters of the PD at `pd` in `ctx`; zeros when
/// LibOSDP knows no such PD.
pub(crate) fn take_metrics(ctx: *mut core::ffi::c_void, pd: core::ffi::c_int) -> Metrics {
    let mut raw = libosdp_sys::osdp_metrics {
        packets_sent: 0,
        packets_received: 0,
        packet_check_errors: 0,
        nak_count: 0,
        sc_handshake_count: 0,
        sc_failure_count: 0,
        command_count: 0,
        event_count: 0,
    };
    if unsafe { libosdp_sys::osdp_get_metrics(ctx, pd, &mut raw) } < 0 {
        return Metrics::default();
    }
    raw.into()
}
