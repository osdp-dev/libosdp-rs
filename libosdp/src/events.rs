//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! OSDP PDs have to send messages to it's controlling unit - CP to intimate it
//! about various events that originate there (such as key press, card reads,
//! etc.,). They do this by creating an "event" and sending it to the CP. This
//! module is responsible to handling such events though [`Event`].

use crate::Error;
use alloc::vec::Vec;

use super::ConvertEndian;

type Result<T> = core::result::Result<T, Error>;

#[cfg(feature = "defmt")]
use defmt::panic;

/// Various card formats that a PD can support. This is sent to CP when a PD
/// must report a card read
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CardFormat {
    /// Card format is not specified
    #[default]
    Unspecified,

    /// Wiegand format
    Wiegand,
}

impl TryFrom<libosdp_sys::osdp_event_cardread_format_e> for CardFormat {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_event_cardread_format_e) -> Result<Self> {
        match value {
            libosdp_sys::osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_UNSPECIFIED => {
                Ok(CardFormat::Unspecified)
            }
            libosdp_sys::osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_WIEGAND => {
                Ok(CardFormat::Wiegand)
            }
            _ => Err(Error::Malformed("card format")),
        }
    }
}

impl From<CardFormat> for libosdp_sys::osdp_event_cardread_format_e {
    fn from(val: CardFormat) -> Self {
        match val {
            CardFormat::Unspecified => {
                libosdp_sys::osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_UNSPECIFIED
            }
            CardFormat::Wiegand => {
                libosdp_sys::osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_WIEGAND
            }
        }
    }
}

/// Event that describes card read activity on the PD
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CardRead {
    /// Reader (another device connected to this PD) which caused this event
    ///
    /// 0 - self
    /// 1 - fist connected reader
    /// 2 - second connected reader
    /// ....
    pub reader_no: i32,

    /// Format of the card that was read
    pub format: CardFormat,

    /// The direction of the PD where the card read happened (some PDs have two
    /// physical card readers to put on either side of a door).
    ///
    /// false - Forward
    /// true - Backward
    pub direction: bool,

    /// Number of valid data bits in [`CardRead::data`] when the card
    /// format is not [`CardFormat::Ascii`]. For [`CardFormat::Ascii`], this
    /// field is set to zero.
    pub nr_bits: usize,

    /// Card data; bytes or bits depending on [`CardFormat`]
    pub data: Vec<u8>,
}

impl CardRead {
    /// Create an raw data card read event for self and direction set to forward
    pub fn new_raw(data: Vec<u8>) -> Self {
        Self {
            reader_no: 0,
            format: CardFormat::Unspecified,
            direction: false,
            nr_bits: data.len() * 8,
            data,
        }
    }

    /// Create a Wiegand card read event for self and direction set to forward
    pub fn new_wiegand(nr_bits: usize, data: Vec<u8>) -> Result<Self> {
        if nr_bits > data.len() * 8 {
            return Err(Error::InvalidArgument("more Wiegand bits than data"));
        }
        Ok(Self {
            reader_no: 0,
            format: CardFormat::Wiegand,
            direction: false,
            nr_bits,
            data,
        })
    }
}

impl TryFrom<libosdp_sys::osdp_event_cardread> for CardRead {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_event_cardread) -> Result<Self> {
        let format = value.format.try_into()?;
        let len = value.length as usize;
        let (nr_bits, nr_bytes) = (len, len.div_ceil(8));

        Ok(CardRead {
            reader_no: value.reader_no,
            format,
            direction: value.direction == 1,
            nr_bits,
            data: value.data[0..nr_bytes].to_vec(),
        })
    }
}

impl From<CardRead> for libosdp_sys::osdp_event_cardread {
    fn from(value: CardRead) -> Self {
        let mut data = [0; libosdp_sys::OSDP_EVENT_CARDREAD_MAX_DATALEN as usize];
        let length = value.nr_bits as u16;
        data[..value.data.len()].copy_from_slice(&value.data[..]);
        libosdp_sys::osdp_event_cardread {
            reader_no: value.reader_no,
            format: value.format.into(),
            direction: value.direction as i32,
            length,
            data,
        }
    }
}

/// Event to describe a key press activity on the PD
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct KeyPress {
    /// Reader (another device connected to this PD) which caused this event
    ///
    /// 0 - self
    /// 1 - fist connected reader
    /// 2 - second connected reader
    /// ....
    pub reader_no: i32,

    /// Key data
    pub data: Vec<u8>,
}

impl KeyPress {
    /// Create key press event for the keys specified in `data`.
    pub fn new(data: Vec<u8>) -> Self {
        Self { reader_no: 0, data }
    }
}

impl From<libosdp_sys::osdp_event_keypress> for KeyPress {
    fn from(value: libosdp_sys::osdp_event_keypress) -> Self {
        let n = value.length as usize;
        let data = value.data[0..n].to_vec();
        KeyPress {
            reader_no: value.reader_no,
            data,
        }
    }
}

impl From<KeyPress> for libosdp_sys::osdp_event_keypress {
    fn from(value: KeyPress) -> Self {
        let mut data = [0; libosdp_sys::OSDP_EVENT_KEYPRESS_MAX_DATALEN as usize];
        data[..value.data.len()].copy_from_slice(&value.data[..]);
        libosdp_sys::osdp_event_keypress {
            reader_no: value.reader_no,
            length: value.data.len() as i32,
            data,
        }
    }
}

/// Event to transport a Manufacturer specific command's response.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MfgReply {
    /// 3-byte IEEE assigned OUI used as vendor code
    pub vendor_code: (u8, u8, u8),

    /// Reply data (if any)
    pub data: Vec<u8>,
}

impl From<libosdp_sys::osdp_event_mfgrep> for MfgReply {
    fn from(value: libosdp_sys::osdp_event_mfgrep) -> Self {
        let n = value.length as usize;
        let data = value.data[0..n].to_vec();
        let bytes = value.vendor_code.to_le_bytes();
        let vendor_code: (u8, u8, u8) = (bytes[0], bytes[1], bytes[2]);
        MfgReply {
            vendor_code,
            data,
        }
    }
}

impl From<MfgReply> for libosdp_sys::osdp_event_mfgrep {
    fn from(value: MfgReply) -> Self {
        let mut data = [0; libosdp_sys::OSDP_EVENT_MFGREP_MAX_DATALEN as usize];
        data[..value.data.len()].copy_from_slice(&value.data[..]);
        libosdp_sys::osdp_event_mfgrep {
            vendor_code: value.vendor_code.as_le(),
            length: value.data.len() as u8,
            data,
        }
    }
}

/// Status report type
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum StatusReportType {
    /// Input status report
    Input,
    /// Output status report
    Output,
    /// Reader tamper status; one byte per attached reader (0 normal, 1 not connected, 2 tamper)
    Reader,
    /// Local status report
    Local,
}

impl TryFrom<libosdp_sys::osdp_status_report_type> for StatusReportType {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_status_report_type) -> Result<Self> {
        match value {
            libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_INPUT => {
                Ok(StatusReportType::Input)
            }
            libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_OUTPUT => {
                Ok(StatusReportType::Output)
            }
            libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_READER => {
                Ok(StatusReportType::Reader)
            }
            libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_LOCAL => {
                Ok(StatusReportType::Local)
            }
            _ => Err(Error::Malformed("status report type")),
        }
    }
}

impl From<StatusReportType> for libosdp_sys::osdp_status_report_type {
    fn from(value: StatusReportType) -> Self {
        match value {
            StatusReportType::Input => {
                libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_INPUT
            }
            StatusReportType::Output => {
                libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_OUTPUT
            }
            StatusReportType::Reader => {
                libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_READER
            }
            StatusReportType::Local => {
                libosdp_sys::osdp_status_report_type_OSDP_STATUS_REPORT_LOCAL
            }
        }
    }
}

/// Event to describe various status changes on PD
///
/// This event is used by the PD to indicate status such as input, output,
/// tamper, etc.,. up to a maximum of 64 status bits can be reported. The values
/// of the least significant N bit of status are considered, where N is the
/// number of items as described in the corresponding capability codes,
/// - PdCapability::OutputControl
/// - PdCapability::ContactStatusMonitoring
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct StatusReport {
    /// The kind of event to report see `enum osdp_event_status_type_e`
    pub type_: StatusReportType,
    /// Number of valid entries in `report`
    pub nr_entries: usize,
    /// Status report
    #[cfg_attr(feature = "serde", serde(with = "serde_arrays"))]
    pub report: [u8; 64],
}

impl TryFrom<libosdp_sys::osdp_status_report> for StatusReport {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_status_report) -> Result<Self> {
        Ok(StatusReport {
            type_: value.type_.try_into()?,
            nr_entries: value.nr_entries as usize,
            report: value.report,
        })
    }
}

impl From<StatusReport> for libosdp_sys::osdp_status_report {
    fn from(value: StatusReport) -> Self {
        libosdp_sys::osdp_status_report {
            type_: value.type_.into(),
            nr_entries: value.nr_entries as i32,
            report: value.report,
        }
    }
}

/// CP to intimate it about various events that originate there (such as key
/// press, card reads, etc.,). They do this by creating an “event” and sending
/// it to the CP. This module is responsible to handling such events though
/// Event.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Event {
    /// Event that describes card read activity on the PD
    CardRead(CardRead),

    /// Event to describe a key press activity on the PD
    KeyPress(KeyPress),

    /// Event to transport a Manufacturer specific command’s response
    MfgReply(MfgReply),

    /// Event to describe a input/output/tamper/power status change
    Status(StatusReport),
}

impl From<Event> for libosdp_sys::osdp_event {
    fn from(value: Event) -> Self {
        match value {
            Event::CardRead(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_CARDREAD,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    cardread: e.clone().into(),
                },
            },
            Event::KeyPress(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_KEYPRESS,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    keypress: e.clone().into(),
                },
            },
            Event::MfgReply(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_MFGREP,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    mfgrep: e.clone().into(),
                },
            },
            Event::Status(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_STATUS,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 { status: e.into() },
            },
        }
    }
}

impl TryFrom<libosdp_sys::osdp_event> for Event {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_event) -> Result<Self> {
        match value.type_ {
            libosdp_sys::osdp_event_type_OSDP_EVENT_CARDREAD => {
                let data = unsafe { value.__bindgen_anon_1.cardread.try_into() }?;
                Ok(Event::CardRead(data))
            }
            libosdp_sys::osdp_event_type_OSDP_EVENT_KEYPRESS => {
                Ok(Event::KeyPress(unsafe { value.__bindgen_anon_1.keypress.into() }))
            }
            libosdp_sys::osdp_event_type_OSDP_EVENT_MFGREP => {
                Ok(Event::MfgReply(unsafe { value.__bindgen_anon_1.mfgrep.into() }))
            }
            libosdp_sys::osdp_event_type_OSDP_EVENT_STATUS => {
                let data = unsafe { value.__bindgen_anon_1.status.try_into() }?;
                Ok(Event::Status(data))
            }
            _ => Err(Error::Malformed("event type")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CardRead;
    use libosdp_sys::{
        osdp_event_cardread, osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_UNSPECIFIED,
        osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_WIEGAND,
    };

    #[test]
    fn test_event_cardread() {
        let event = CardRead::new_raw(vec![0x55, 0xAA]);
        let event_struct: osdp_event_cardread = event.clone().into();

        assert_eq!(event_struct.length, 2 * 8);
        assert_eq!(event_struct.direction, 0);
        assert_eq!(
            event_struct.format,
            osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_UNSPECIFIED
        );
        assert_eq!(event_struct.data[0], 0x55);
        assert_eq!(event_struct.data[1], 0xAA);

        assert_eq!(event, event_struct.try_into().unwrap());

        let event = CardRead::new_wiegand(15, vec![0x55, 0xAA]).unwrap();
        let event_struct: osdp_event_cardread = event.clone().into();

        assert_eq!(event_struct.length, 15);
        assert_eq!(event_struct.direction, 0);
        assert_eq!(
            event_struct.format,
            osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_WIEGAND
        );
        assert_eq!(event_struct.data[0], 0x55);
        assert_eq!(event_struct.data[1], 0xAA);

        assert_eq!(event, event_struct.try_into().unwrap());
    }
}
