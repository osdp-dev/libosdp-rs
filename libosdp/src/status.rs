//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! A PD reports the state of its inputs, outputs, readers, and its own
//! tamper and power, one entry per entity; a CP asks for one kind at a time.

use crate::{copy_into, Error};
use alloc::vec::Vec;

type Result<T> = core::result::Result<T, Error>;

const MAX_ENTRIES: usize = libosdp_sys::OSDP_STATUS_REPORT_MAX_LEN as usize;

/// Which status a CP asks a PD for with [`Command::Status`](crate::Command).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum StatusKind {
    /// The PD's inputs.
    Input,
    /// The PD's outputs.
    Output,
    /// The PD's own tamper and power state.
    Local,
    /// The readers attached to the PD.
    Reader,
}

impl TryFrom<libosdp_sys::osdp_status_report_type> for StatusKind {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_status_report_type) -> Result<Self> {
        match value {
            libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_INPUT => Ok(StatusKind::Input),
            libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_OUTPUT => {
                Ok(StatusKind::Output)
            }
            libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_LOCAL => Ok(StatusKind::Local),
            libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_READER => {
                Ok(StatusKind::Reader)
            }
            _ => Err(Error::Malformed("status report type")),
        }
    }
}

impl From<StatusKind> for libosdp_sys::osdp_status_report_type {
    fn from(value: StatusKind) -> Self {
        match value {
            StatusKind::Input => libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_INPUT,
            StatusKind::Output => libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_OUTPUT,
            StatusKind::Local => libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_LOCAL,
            StatusKind::Reader => libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_READER,
        }
    }
}

impl From<StatusKind> for libosdp_sys::osdp_status_report {
    fn from(value: StatusKind) -> Self {
        libosdp_sys::osdp_status_report {
            type_: value.into(),
            nr_entries: 0,
            report: [0; MAX_ENTRIES],
        }
    }
}

/// The state of a reader attached to a PD.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ReaderStatus {
    /// Working normally.
    Normal,
    /// Not connected to the PD.
    NotConnected,
    /// Tampered with.
    Tamper,
    /// A state OSDP does not define.
    Unknown(u8),
}

impl From<u8> for ReaderStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => ReaderStatus::Normal,
            1 => ReaderStatus::NotConnected,
            2 => ReaderStatus::Tamper,
            other => ReaderStatus::Unknown(other),
        }
    }
}

impl From<ReaderStatus> for u8 {
    fn from(value: ReaderStatus) -> Self {
        match value {
            ReaderStatus::Normal => 0,
            ReaderStatus::NotConnected => 1,
            ReaderStatus::Tamper => 2,
            ReaderStatus::Unknown(other) => other,
        }
    }
}

/// A PD's status of one [`StatusKind`], one entry per entity.
///
/// A PD reports every entity of the kind, in order: as many inputs, outputs
/// or readers as it advertises in its contact status monitoring, output
/// control or readers capability. LibOSDP refuses a report with any other
/// count.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Status {
    /// One byte per input; 0 is inactive and 1 active.
    Input(Vec<u8>),
    /// One byte per output; 0 is inactive and 1 active.
    Output(Vec<u8>),
    /// The PD's own state.
    Local {
        /// The PD has been tampered with.
        tamper: bool,
        /// The PD's power has failed.
        power: bool,
    },
    /// One entry per attached reader.
    Reader(Vec<ReaderStatus>),
}

impl Status {
    /// The kind of status this reports.
    pub fn kind(&self) -> StatusKind {
        match self {
            Status::Input(_) => StatusKind::Input,
            Status::Output(_) => StatusKind::Output,
            Status::Local { .. } => StatusKind::Local,
            Status::Reader(_) => StatusKind::Reader,
        }
    }
}

impl TryFrom<libosdp_sys::osdp_status_report> for Status {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_status_report) -> Result<Self> {
        let entries = usize::try_from(value.nr_entries)
            .ok()
            .and_then(|n| value.report.get(..n))
            .ok_or(Error::Malformed("status entry count"))?;
        Ok(match StatusKind::try_from(value.type_)? {
            StatusKind::Input => Status::Input(entries.to_vec()),
            StatusKind::Output => Status::Output(entries.to_vec()),
            StatusKind::Local => match entries {
                [tamper, power] => Status::Local {
                    tamper: *tamper != 0,
                    power: *power != 0,
                },
                _ => return Err(Error::Malformed("status entry count")),
            },
            StatusKind::Reader => {
                Status::Reader(entries.iter().map(|&e| ReaderStatus::from(e)).collect())
            }
        })
    }
}

impl TryFrom<Status> for libosdp_sys::osdp_status_report {
    type Error = Error;

    fn try_from(value: Status) -> Result<Self> {
        let kind = value.kind();
        let entries: Vec<u8> = match value {
            Status::Input(entries) | Status::Output(entries) => entries,
            Status::Local { tamper, power } => alloc::vec![tamper as u8, power as u8],
            Status::Reader(readers) => readers.into_iter().map(u8::from).collect(),
        };
        let report = copy_into::<MAX_ENTRIES>(&entries)?;
        Ok(libosdp_sys::osdp_status_report {
            type_: kind.into(),
            nr_entries: entries.len() as i32,
            report,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn round_trip(status: Status) -> Status {
        let raw: libosdp_sys::osdp_status_report = status.try_into().unwrap();
        raw.try_into().unwrap()
    }

    #[test]
    fn every_kind_round_trips() {
        for status in [
            Status::Input(vec![0, 1, 1]),
            Status::Output(vec![1]),
            Status::Local {
                tamper: true,
                power: false,
            },
            Status::Reader(vec![
                ReaderStatus::Normal,
                ReaderStatus::Tamper,
                ReaderStatus::Unknown(9),
            ]),
        ] {
            assert_eq!(round_trip(status.clone()), status);
        }
    }

    #[test]
    fn local_status_is_two_entries_tamper_first() {
        let raw: libosdp_sys::osdp_status_report = Status::Local {
            tamper: true,
            power: false,
        }
        .try_into()
        .unwrap();
        assert_eq!(raw.nr_entries, 2);
        assert_eq!(raw.report[..2], [1, 0]);
    }

    #[test]
    fn more_entries_than_libosdp_holds_are_refused() {
        let raw: Result<libosdp_sys::osdp_status_report> =
            Status::Input(vec![0; MAX_ENTRIES + 1]).try_into();
        assert!(matches!(raw, Err(Error::TooLong { max: MAX_ENTRIES })));
    }

    #[test]
    fn entry_counts_past_the_report_are_malformed() {
        let mut raw: libosdp_sys::osdp_status_report = Status::Output(vec![1]).try_into().unwrap();
        raw.nr_entries = MAX_ENTRIES as i32 + 1;
        assert_eq!(
            Status::try_from(raw),
            Err(Error::Malformed("status entry count"))
        );
        let mut local: libosdp_sys::osdp_status_report = StatusKind::Local.into();
        for nr_entries in [1, 3] {
            local.nr_entries = nr_entries;
            assert_eq!(
                Status::try_from(local),
                Err(Error::Malformed("status entry count"))
            );
        }
    }
}
