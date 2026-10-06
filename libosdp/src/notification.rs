//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! What LibOSDP observes about a link -- as opposed to what a peer sends
//! over it -- reported to the application when [`Flag::EnableNotification`]
//! is set.
//!
//! [`Flag::EnableNotification`]: crate::Flag::EnableNotification

use crate::{Error, PdId};

type Result<T> = core::result::Result<T, Error>;

/// Which kind of multi-part operation a [`Multipart`] notification is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MpKind {
    /// A file transfer.
    FileTransfer,
    /// A PIV data request.
    Piv,
    /// A general authenticate request.
    GenAuth,
    /// A challenge/response authenticate request.
    CrAuth,
    /// A biometric read reply that spans several packets.
    BioRead,
}

impl MpKind {
    pub(crate) fn as_c(self) -> libosdp_sys::osdp_mp_msg_type {
        match self {
            MpKind::FileTransfer => libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_FILE_TRANSFER,
            MpKind::Piv => libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_PIV,
            MpKind::GenAuth => libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_GENAUTH,
            MpKind::CrAuth => libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_CRAUTH,
            MpKind::BioRead => libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_BIOREAD,
        }
    }
}

impl TryFrom<libosdp_sys::osdp_mp_msg_type> for MpKind {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_mp_msg_type) -> Result<Self> {
        match value {
            libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_FILE_TRANSFER => Ok(MpKind::FileTransfer),
            libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_PIV => Ok(MpKind::Piv),
            libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_GENAUTH => Ok(MpKind::GenAuth),
            libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_CRAUTH => Ok(MpKind::CrAuth),
            libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_BIOREAD => Ok(MpKind::BioRead),
            _ => Err(Error::Malformed("multi-part kind")),
        }
    }
}

/// How a multi-part operation ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MpOutcome {
    /// It completed.
    Ok,
    /// A file transfer completed and the PD is resetting to use the file.
    OkRebooting,
    /// It was cancelled, by either end, or the link failed.
    Aborted,
    /// The PD did not recognize the file's contents.
    Unrecognized,
    /// The PD rejected the file as malformed.
    Invalid,
}

impl TryFrom<libosdp_sys::osdp_mp_outcome> for MpOutcome {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_mp_outcome) -> Result<Self> {
        match value {
            libosdp_sys::osdp_mp_outcome_OSDP_MP_OUTCOME_OK => Ok(MpOutcome::Ok),
            libosdp_sys::osdp_mp_outcome_OSDP_MP_OUTCOME_OK_REBOOTING => Ok(MpOutcome::OkRebooting),
            libosdp_sys::osdp_mp_outcome_OSDP_MP_OUTCOME_ABORTED => Ok(MpOutcome::Aborted),
            libosdp_sys::osdp_mp_outcome_OSDP_MP_OUTCOME_UNRECOGNIZED => {
                Ok(MpOutcome::Unrecognized)
            }
            libosdp_sys::osdp_mp_outcome_OSDP_MP_OUTCOME_INVALID => Ok(MpOutcome::Invalid),
            _ => Err(Error::Malformed("multi-part outcome")),
        }
    }
}

/// Where a multi-part operation is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MpPhase {
    /// It started.
    Start,
    /// Another fragment went through.
    Progress,
    /// It ended.
    Done(MpOutcome),
}

/// Progress of a multi-part operation: a file transfer, a PIV or
/// authenticate request, or a multi-part biometric read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Multipart {
    /// Which kind of operation this is.
    pub kind: MpKind,
    /// Where it is.
    pub phase: MpPhase,
    /// Which object it moves: the file id of a file transfer, the PIV
    /// element of a PIV request.
    pub object_id: i32,
    /// Bytes moved so far.
    pub offset: u32,
    /// Bytes to move in all.
    pub total: u32,
}

/// The state of a transparent reader card session, or of the presence scan
/// that looks for cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TrsSessionStatus {
    /// The reader is in transparent mode; card commands flow.
    Opened,
    /// The session was closed as asked.
    Closed,
    /// The PD refused transparent mode, or the session was cut short.
    Failed,
    /// The reader refused a presence scan probe; probing continues, backing
    /// off.
    ScanSuspended,
    /// The reader cannot tell whether a card is still there, so a card
    /// leaving the field will not be reported.
    ScanNoDeparture,
}

impl TryFrom<libosdp_sys::osdp_trs_session_status_e> for TrsSessionStatus {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_trs_session_status_e) -> Result<Self> {
        match value {
            libosdp_sys::osdp_trs_session_status_e_OSDP_TRS_SESSION_OPENED => {
                Ok(TrsSessionStatus::Opened)
            }
            libosdp_sys::osdp_trs_session_status_e_OSDP_TRS_SESSION_CLOSED => {
                Ok(TrsSessionStatus::Closed)
            }
            libosdp_sys::osdp_trs_session_status_e_OSDP_TRS_SESSION_FAILED => {
                Ok(TrsSessionStatus::Failed)
            }
            libosdp_sys::osdp_trs_session_status_e_OSDP_TRS_SCAN_SUSPENDED => {
                Ok(TrsSessionStatus::ScanSuspended)
            }
            libosdp_sys::osdp_trs_session_status_e_OSDP_TRS_SCAN_NO_DEPARTURE => {
                Ok(TrsSessionStatus::ScanNoDeparture)
            }
            _ => Err(Error::Malformed("TRS session status")),
        }
    }
}

/// Something LibOSDP observed about a link. A CP hands these over through
/// [`ControlPanel::next_event`](crate::ControlPanel::next_event), a PD
/// through
/// [`PeripheralDevice::next_notification`](crate::PeripheralDevice::next_notification).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Notification {
    /// The secure channel came up or went down.
    ScStatus {
        /// The secure channel is up.
        active: bool,
        /// It is up on the install key, SCBK-D, rather than the PD's own.
        scbk_d: bool,
    },

    /// The peer went online or offline. On a CP, the PD is offline -- and
    /// refuses commands -- while it redoes the secure channel handshake.
    PdStatus {
        /// The peer is online.
        online: bool,
    },

    /// A multi-part operation started, moved on or ended.
    Multipart(Multipart),

    /// The CP read the PD's identity, and it differs from the last one it
    /// read for this PD, on first contact or when another device answers
    /// at the PD's address. CP only.
    PdId(PdId),

    /// A transparent reader card session or presence scan changed state.
    /// CP only.
    TrsStatus(TrsSessionStatus),
}

impl TryFrom<libosdp_sys::osdp_notification> for Notification {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_notification) -> Result<Self> {
        let payload = value.__bindgen_anon_1;
        let multipart = |phase| -> Result<Self> {
            let mp = unsafe { payload.mp };
            Ok(Notification::Multipart(Multipart {
                kind: mp.mp_type.try_into()?,
                phase,
                object_id: mp.object_id,
                offset: mp.offset,
                total: mp.total,
            }))
        };
        match value.type_ {
            libosdp_sys::osdp_notification_type_OSDP_NOTIFICATION_SC_STATUS => {
                let sc = unsafe { payload.sc_status };
                Ok(Notification::ScStatus {
                    active: sc.active,
                    scbk_d: sc.scbk_d,
                })
            }
            libosdp_sys::osdp_notification_type_OSDP_NOTIFICATION_PD_STATUS => {
                Ok(Notification::PdStatus {
                    online: unsafe { payload.pd_status }.online,
                })
            }
            libosdp_sys::osdp_notification_type_OSDP_NOTIFICATION_MP_START => {
                multipart(MpPhase::Start)
            }
            libosdp_sys::osdp_notification_type_OSDP_NOTIFICATION_MP_PROGRESS => {
                multipart(MpPhase::Progress)
            }
            libosdp_sys::osdp_notification_type_OSDP_NOTIFICATION_MP_DONE => {
                let outcome = unsafe { payload.mp }.outcome.try_into()?;
                multipart(MpPhase::Done(outcome))
            }
            libosdp_sys::osdp_notification_type_OSDP_NOTIFICATION_PD_ID => {
                Ok(Notification::PdId(unsafe { payload.pd_id }.into()))
            }
            libosdp_sys::osdp_notification_type_OSDP_NOTIFICATION_TRS_STATUS => Ok(
                Notification::TrsStatus(unsafe { payload.trs_status }.status.try_into()?),
            ),
            _ => Err(Error::Malformed("notification type")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(type_: libosdp_sys::osdp_notification_type) -> libosdp_sys::osdp_notification {
        let mut raw: libosdp_sys::osdp_notification = unsafe { core::mem::zeroed() };
        raw.type_ = type_;
        raw
    }

    #[test]
    fn multipart_done_carries_its_outcome() {
        let mut done = raw(libosdp_sys::osdp_notification_type_OSDP_NOTIFICATION_MP_DONE);
        done.__bindgen_anon_1.mp = libosdp_sys::osdp_mp_notification {
            mp_type: libosdp_sys::osdp_mp_msg_type_OSDP_MP_MSG_FILE_TRANSFER,
            object_id: 7,
            total: 100,
            offset: 40,
            outcome: libosdp_sys::osdp_mp_outcome_OSDP_MP_OUTCOME_ABORTED,
        };
        assert_eq!(
            Notification::try_from(done),
            Ok(Notification::Multipart(Multipart {
                kind: MpKind::FileTransfer,
                phase: MpPhase::Done(MpOutcome::Aborted),
                object_id: 7,
                offset: 40,
                total: 100,
            }))
        );
    }

    #[test]
    fn unknown_discriminants_are_malformed() {
        assert_eq!(
            Notification::try_from(raw(u8::MAX)),
            Err(Error::Malformed("notification type"))
        );
        let mut start = raw(libosdp_sys::osdp_notification_type_OSDP_NOTIFICATION_MP_START);
        start.__bindgen_anon_1.mp.mp_type = 0;
        assert_eq!(
            Notification::try_from(start),
            Err(Error::Malformed("multi-part kind"))
        );
    }
}
