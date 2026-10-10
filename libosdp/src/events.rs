//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! OSDP PDs have to send messages to it's controlling unit - CP to intimate it
//! about various events that originate there (such as key press, card reads,
//! etc.,). They do this by creating an "event" and sending it to the CP. This
//! module is responsible to handling such events though [`Event`].

use crate::{copy_into, BioMatchReply, BioReadReply, Error, PivReply, Status, TrsReply};
use alloc::vec::Vec;

use super::ConvertEndian;

type Result<T> = core::result::Result<T, Error>;

#[cfg(feature = "defmt")]
use defmt::panic;

/// Various card formats that a PD can support. This is sent to CP when a PD
/// must report a card read
///
/// LibOSDP does not support OSDP's ASCII card format (osdp_FMT): a PD
/// never sends it and a CP ignores it, so this type has no variant for it
/// and a [`CardRead`]'s length is always in bits.
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

    /// Read direction: `false` forward, `true` reverse. Only osdp_FMT
    /// carries one, and LibOSDP does not support it: a PD ignores this
    /// field when it sends a card read, and a CP always reports `false`.
    pub direction: bool,

    /// Number of valid data bits in [`CardRead::data`].
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
        let nr_bits = value.length as usize;
        let data = value
            .data
            .get(..nr_bits.div_ceil(8))
            .ok_or(Error::Malformed("card data length"))?;

        Ok(CardRead {
            reader_no: value.reader_no,
            format,
            direction: value.direction == 1,
            nr_bits,
            data: data.to_vec(),
        })
    }
}

impl TryFrom<CardRead> for libosdp_sys::osdp_event_cardread {
    type Error = Error;

    fn try_from(value: CardRead) -> Result<Self> {
        let data =
            copy_into::<{ libosdp_sys::OSDP_EVENT_CARDREAD_MAX_DATALEN as usize }>(&value.data)?;
        if value.nr_bits > value.data.len() * 8 {
            return Err(Error::InvalidArgument("more card bits than data"));
        }
        Ok(libosdp_sys::osdp_event_cardread {
            reader_no: value.reader_no,
            format: value.format.into(),
            direction: value.direction as i32,
            length: value.nr_bits as u16,
            data,
        })
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

impl TryFrom<libosdp_sys::osdp_event_keypress> for KeyPress {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_event_keypress) -> Result<Self> {
        let data = usize::try_from(value.length)
            .ok()
            .and_then(|n| value.data.get(..n))
            .ok_or(Error::Malformed("key press length"))?;
        Ok(KeyPress {
            reader_no: value.reader_no,
            data: data.to_vec(),
        })
    }
}

impl TryFrom<KeyPress> for libosdp_sys::osdp_event_keypress {
    type Error = Error;

    fn try_from(value: KeyPress) -> Result<Self> {
        let data = copy_into::<{ libosdp_sys::OSDP_EVENT_KEYPRESS_MAX_DATALEN as usize }>(&value.data)?;
        Ok(libosdp_sys::osdp_event_keypress {
            reader_no: value.reader_no,
            length: value.data.len() as i32,
            data,
        })
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

impl TryFrom<libosdp_sys::osdp_event_mfgrep> for MfgReply {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_event_mfgrep) -> Result<Self> {
        let data = value
            .data
            .get(..value.length as usize)
            .ok_or(Error::Malformed("manufacturer reply length"))?;
        let bytes = value.vendor_code.to_le_bytes();
        Ok(MfgReply {
            vendor_code: (bytes[0], bytes[1], bytes[2]),
            data: data.to_vec(),
        })
    }
}

impl TryFrom<MfgReply> for libosdp_sys::osdp_event_mfgrep {
    type Error = Error;

    fn try_from(value: MfgReply) -> Result<Self> {
        let data = copy_into::<{ libosdp_sys::OSDP_EVENT_MFGREP_MAX_DATALEN as usize }>(&value.data)?;
        Ok(libosdp_sys::osdp_event_mfgrep {
            vendor_code: value.vendor_code.as_le(),
            length: value.data.len() as u8,
            data,
        })
    }
}

/// A manufacturer specific status or error reply. Unlike [`MfgReply`], OSDP
/// gives these no vendor code: the data is entirely vendor defined, to be
/// read against the vendor code in the PD's [`PdId`](crate::PdId).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MfgStatus {
    /// Reply data, if any
    pub data: Vec<u8>,
}

impl TryFrom<libosdp_sys::osdp_event_mfgstat> for MfgStatus {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_event_mfgstat) -> Result<Self> {
        let data = value
            .data
            .get(..value.length as usize)
            .ok_or(Error::Malformed("manufacturer status length"))?;
        Ok(MfgStatus {
            data: data.to_vec(),
        })
    }
}

impl TryFrom<MfgStatus> for libosdp_sys::osdp_event_mfgstat {
    type Error = Error;

    fn try_from(value: MfgStatus) -> Result<Self> {
        let data =
            copy_into::<{ libosdp_sys::OSDP_EVENT_MFGSTAT_MAX_DATALEN as usize }>(&value.data)?;
        Ok(libosdp_sys::osdp_event_mfgstat {
            length: value.data.len() as u8,
            data,
        })
    }
}

/// CP to intimate it about various events that originate there (such as key
/// press, card reads, etc.,). They do this by creating an “event” and sending
/// it to the CP. This module is responsible to handling such events though
/// Event.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Event {
    /// Event that describes card read activity on the PD
    CardRead(CardRead),

    /// Event to describe a key press activity on the PD
    KeyPress(KeyPress),

    /// Event to transport a Manufacturer specific command’s response
    MfgReply(MfgReply),

    /// A manufacturer specific status, reported on its own or in answer
    /// to a [`Command::Mfg`](crate::Command::Mfg)
    MfgStatus(MfgStatus),

    /// A manufacturer specific error, reported on its own or in answer to
    /// a [`Command::Mfg`](crate::Command::Mfg). In answer, it also fails
    /// the command's [`Submission`](crate::Submission) with
    /// [`CompletionError::Failed`](crate::CompletionError::Failed)
    MfgError(MfgStatus),

    /// A PD's scan, sent in answer to a
    /// [`Command::BioRead`](crate::Command::BioRead), inline or on a
    /// later poll
    BioRead(BioReadReply),

    /// A PD's match result, sent in answer to a
    /// [`Command::BioMatch`](crate::Command::BioMatch), inline or on a
    /// later poll
    BioMatch(BioMatchReply),

    /// A smart card's answer to a [`Command::PivData`](crate::Command::PivData)
    PivData(PivReply),

    /// A smart card's answer to a [`Command::GenAuth`](crate::Command::GenAuth)
    GenAuth(PivReply),

    /// A smart card's answer to a [`Command::CrAuth`](crate::Command::CrAuth)
    CrAuth(PivReply),

    /// A transparent reader reply: a card in the field, or a card's answer
    /// to a [`Command::Trs`](crate::Command::Trs)
    Trs(TrsReply),

    /// A PD's status, reported on a change or in answer to a
    /// [`Command::Status`](crate::Command::Status)
    Status(Status),
}

impl TryFrom<Event> for libosdp_sys::osdp_event {
    type Error = Error;

    fn try_from(value: Event) -> Result<Self> {
        Ok(match value {
            Event::CardRead(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_CARDREAD,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    cardread: e.try_into()?,
                },
            },
            Event::KeyPress(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_KEYPRESS,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    keypress: e.try_into()?,
                },
            },
            Event::MfgReply(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_MFGREP,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    mfgrep: e.try_into()?,
                },
            },
            Event::MfgStatus(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_MFGSTATR,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    mfgstatr: e.try_into()?,
                },
            },
            Event::MfgError(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_MFGERRR,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    mfgerrr: e.try_into()?,
                },
            },
            Event::BioRead(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_BIOREADR,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    bioreadr: e.try_into()?,
                },
            },
            Event::BioMatch(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_BIOMATCHR,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    biomatchr: e.into(),
                },
            },
            Event::PivData(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_PIVDATAR,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    piv_reply: e.try_into()?,
                },
            },
            Event::GenAuth(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_GENAUTHR,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    piv_reply: e.try_into()?,
                },
            },
            Event::CrAuth(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_CRAUTHR,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    piv_reply: e.try_into()?,
                },
            },
            Event::Trs(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_TRS,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 { trs: e.try_into()? },
            },
            Event::Status(e) => libosdp_sys::osdp_event {
                _node: unsafe { core::mem::zeroed() },
                type_: libosdp_sys::osdp_event_type_OSDP_EVENT_STATUS,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_event__bindgen_ty_1 {
                    status: e.try_into()?,
                },
            },
        })
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
                Ok(Event::KeyPress(unsafe { value.__bindgen_anon_1.keypress }.try_into()?))
            }
            libosdp_sys::osdp_event_type_OSDP_EVENT_MFGREP => {
                Ok(Event::MfgReply(unsafe { value.__bindgen_anon_1.mfgrep }.try_into()?))
            }
            libosdp_sys::osdp_event_type_OSDP_EVENT_STATUS => {
                let data = unsafe { value.__bindgen_anon_1.status.try_into() }?;
                Ok(Event::Status(data))
            }
            libosdp_sys::osdp_event_type_OSDP_EVENT_MFGSTATR => Ok(Event::MfgStatus(
                unsafe { value.__bindgen_anon_1.mfgstatr }.try_into()?,
            )),
            libosdp_sys::osdp_event_type_OSDP_EVENT_MFGERRR => Ok(Event::MfgError(
                unsafe { value.__bindgen_anon_1.mfgerrr }.try_into()?,
            )),
            libosdp_sys::osdp_event_type_OSDP_EVENT_BIOREADR => Ok(Event::BioRead(
                unsafe { value.__bindgen_anon_1.bioreadr }.try_into()?,
            )),
            libosdp_sys::osdp_event_type_OSDP_EVENT_BIOMATCHR => Ok(Event::BioMatch(
                unsafe { value.__bindgen_anon_1.biomatchr }.into(),
            )),
            libosdp_sys::osdp_event_type_OSDP_EVENT_PIVDATAR => Ok(Event::PivData(
                unsafe { value.__bindgen_anon_1.piv_reply }.try_into()?,
            )),
            libosdp_sys::osdp_event_type_OSDP_EVENT_GENAUTHR => Ok(Event::GenAuth(
                unsafe { value.__bindgen_anon_1.piv_reply }.try_into()?,
            )),
            libosdp_sys::osdp_event_type_OSDP_EVENT_CRAUTHR => Ok(Event::CrAuth(
                unsafe { value.__bindgen_anon_1.piv_reply }.try_into()?,
            )),
            libosdp_sys::osdp_event_type_OSDP_EVENT_TRS => Ok(Event::Trs(
                unsafe { value.__bindgen_anon_1.trs }.try_into()?,
            )),
            _ => Err(Error::Malformed("event type")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CardRead, Event, KeyPress, MfgReply, MfgStatus};
    use crate::Error;
    use libosdp_sys::{
        osdp_event_cardread, osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_UNSPECIFIED,
        osdp_event_cardread_format_e_OSDP_CARD_FMT_RAW_WIEGAND,
    };

    #[test]
    fn test_event_cardread() {
        let event = CardRead::new_raw(vec![0x55, 0xAA]);
        let event_struct: osdp_event_cardread = event.clone().try_into().unwrap();

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
        let event_struct: osdp_event_cardread = event.clone().try_into().unwrap();

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

    #[test]
    fn lengths_past_libosdp_buffers_are_malformed() {
        let mut card: libosdp_sys::osdp_event = Event::CardRead(CardRead::new_raw(vec![1]))
            .try_into()
            .unwrap();
        card.__bindgen_anon_1.cardread.length = u16::MAX;
        let mut keys: libosdp_sys::osdp_event =
            Event::KeyPress(KeyPress::new(vec![1])).try_into().unwrap();
        keys.__bindgen_anon_1.keypress.length = -1;
        let mut mfg: libosdp_sys::osdp_event =
            Event::MfgReply(MfgReply::default()).try_into().unwrap();
        mfg.__bindgen_anon_1.mfgrep.length = u8::MAX;
        for (raw, reason) in [
            (card, "card data length"),
            (keys, "key press length"),
            (mfg, "manufacturer reply length"),
        ] {
            assert_eq!(Event::try_from(raw), Err(Error::Malformed(reason)));
        }
    }

    #[test]
    fn manufacturer_status_and_error_round_trip() {
        for event in [
            Event::MfgStatus(MfgStatus { data: vec![1, 2] }),
            Event::MfgError(MfgStatus { data: vec![] }),
        ] {
            let raw: libosdp_sys::osdp_event = event.clone().try_into().unwrap();
            assert_eq!(Event::try_from(raw), Ok(event));
        }
        let oversized: Result<libosdp_sys::osdp_event, Error> =
            Event::MfgError(MfgStatus { data: vec![0; 129] }).try_into();
        assert!(matches!(oversized, Err(Error::TooLong { max: 128 })));
        let mut raw: libosdp_sys::osdp_event =
            Event::MfgStatus(MfgStatus::default()).try_into().unwrap();
        raw.__bindgen_anon_1.mfgstatr.length = u8::MAX;
        assert_eq!(
            Event::try_from(raw),
            Err(Error::Malformed("manufacturer status length"))
        );
    }
}
