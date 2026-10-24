//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

//! Transparent Reader Support (TRS) lets a CP talk to a smart card through a
//! PD's reader: it opens a card session, sends the card APDUs, and closes
//! the session. LibOSDP builds it only with the `trs` feature.

use crate::commands::{from_count, to_count, SECOND};
use crate::{copy_into, Error};
use alloc::vec::Vec;
use core::time::Duration;

type Result<T> = core::result::Result<T, Error>;

const APDU_MAX: usize = libosdp_sys::OSDP_TRS_APDU_MAX_LEN as usize;
const CSN_MAX: usize = libosdp_sys::OSDP_TRS_CSN_MAX_LEN as usize;
const PROTOCOL_DATA_MAX: usize = libosdp_sys::OSDP_TRS_PROTOCOL_DATA_MAX_LEN as usize;
const MILLISECOND: Duration = Duration::from_millis(1);

fn to_apdu(data: &[u8]) -> Result<libosdp_sys::osdp_trs_apdu> {
    Ok(libosdp_sys::osdp_trs_apdu {
        length: data.len() as u16,
        data: copy_into::<APDU_MAX>(data)?,
    })
}

fn from_apdu(apdu: &libosdp_sys::osdp_trs_apdu) -> Result<Vec<u8>> {
    let data = apdu
        .data
        .get(..apdu.length as usize)
        .ok_or(Error::Malformed("APDU length"))?;
    Ok(data.to_vec())
}

/// How the reader encodes the PIN digits it inserts into the APDU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PinFormat {
    /// Binary digit values.
    Binary,
    /// Packed BCD.
    Bcd,
    /// ASCII characters.
    Ascii,
}

bitflags::bitflags! {
    /// What ends PIN entry.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct PinEndOn: u32 {
        /// The user entered the most digits allowed.
        const MaxDigits = libosdp_sys::osdp_trs_pin_complete_e_OSDP_TRS_PIN_COMPLETE_ON_MAX_DIGITS as u32;
        /// The user pressed the validation key.
        const Key = libosdp_sys::osdp_trs_pin_complete_e_OSDP_TRS_PIN_COMPLETE_ON_KEY as u32;
        /// Entry timed out.
        const Timeout = libosdp_sys::osdp_trs_pin_complete_e_OSDP_TRS_PIN_COMPLETE_ON_TIMEOUT as u32;
    }
}

/// Where in the APDU the reader puts the PIN, and how. Positions count bits
/// from the start of the APDU payload, and must be byte aligned (up to 120
/// bits) or within the first 15 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PinBlock {
    /// Digit encoding.
    pub format: PinFormat,
    /// Right-justify the PIN in the block, rather than left.
    pub right_justify: bool,
    /// Where the block starts.
    pub offset_bits: u16,
    /// Size of the block once formatted, as the card scheme defines it: 8
    /// for EMV and ISO 9564 PIN blocks.
    pub size_bytes: u8,
}

/// Where in the APDU the reader records how many digits were entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PinLengthField {
    /// Size of the field; not zero.
    pub size_bits: u8,
    /// Where the field starts.
    pub offset_bits: u16,
}

/// Prompt the user for a PIN on the reader, put it into `apdu`, and send the
/// result to the card (EMV secure PIN entry).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PinEntry {
    /// How long to wait for the first digit, in whole seconds up to 255;
    /// zero is the reader's default.
    pub first_digit_timeout: Duration,
    /// How long to wait for each further digit, in whole seconds up to 255.
    pub digit_timeout: Duration,
    /// Where the PIN goes.
    pub pin_block: PinBlock,
    /// Where the number of digits goes, if the APDU has a place for it.
    pub length_field: Option<PinLengthField>,
    /// Fewest digits accepted.
    pub min_digits: u8,
    /// Most digits accepted.
    pub max_digits: u8,
    /// What ends entry.
    pub end_on: PinEndOn,
    /// Number of messages to display.
    pub messages: u8,
    /// Language of the messages.
    pub language_id: u16,
    /// Which message to display.
    pub message_index: u8,
    /// T=1 protocol prologue.
    pub t1_prologue: [u8; 3],
    /// The C-APDU to send once the PIN is in it.
    pub apdu: Vec<u8>,
}

impl TryFrom<libosdp_sys::osdp_trs_pin_entry> for PinEntry {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_trs_pin_entry) -> Result<Self> {
        let format = match value.pin_block.format {
            libosdp_sys::osdp_trs_pin_format_e_OSDP_TRS_PIN_FORMAT_BINARY => PinFormat::Binary,
            libosdp_sys::osdp_trs_pin_format_e_OSDP_TRS_PIN_FORMAT_BCD => PinFormat::Bcd,
            libosdp_sys::osdp_trs_pin_format_e_OSDP_TRS_PIN_FORMAT_ASCII => PinFormat::Ascii,
            _ => return Err(Error::Malformed("PIN format")),
        };
        let length = value.pin_length_field;
        Ok(PinEntry {
            first_digit_timeout: from_count(value.timeout_initial, SECOND),
            digit_timeout: from_count(value.timeout_digit, SECOND),
            pin_block: PinBlock {
                format,
                right_justify: value.pin_block.right_justify,
                offset_bits: value.pin_block.offset_bits,
                size_bytes: value.pin_block.size_bytes,
            },
            length_field: (length.size_bits != 0).then_some(PinLengthField {
                size_bits: length.size_bits,
                offset_bits: length.offset_bits,
            }),
            min_digits: value.min_digits,
            max_digits: value.max_digits,
            end_on: PinEndOn::from_bits_retain(value.complete_on),
            messages: value.num_messages,
            language_id: value.language_id,
            message_index: value.msg_index,
            t1_prologue: value.teo_prologue,
            apdu: from_apdu(&value.apdu)?,
        })
    }
}

impl TryFrom<PinEntry> for libosdp_sys::osdp_trs_pin_entry {
    type Error = Error;

    fn try_from(value: PinEntry) -> Result<Self> {
        let format = match value.pin_block.format {
            PinFormat::Binary => libosdp_sys::osdp_trs_pin_format_e_OSDP_TRS_PIN_FORMAT_BINARY,
            PinFormat::Bcd => libosdp_sys::osdp_trs_pin_format_e_OSDP_TRS_PIN_FORMAT_BCD,
            PinFormat::Ascii => libosdp_sys::osdp_trs_pin_format_e_OSDP_TRS_PIN_FORMAT_ASCII,
        };
        let length = match value.length_field {
            Some(PinLengthField { size_bits: 0, .. }) => {
                return Err(Error::InvalidArgument("PIN length field size"))
            }
            Some(length) => length,
            None => PinLengthField {
                size_bits: 0,
                offset_bits: 0,
            },
        };
        Ok(libosdp_sys::osdp_trs_pin_entry {
            timeout_initial: to_count(value.first_digit_timeout, SECOND, "PIN entry timeout")?,
            timeout_digit: to_count(value.digit_timeout, SECOND, "PIN digit timeout")?,
            pin_block: libosdp_sys::osdp_trs_pin_entry__bindgen_ty_1 {
                format,
                right_justify: value.pin_block.right_justify,
                offset_bits: value.pin_block.offset_bits,
                size_bytes: value.pin_block.size_bytes,
            },
            pin_length_field: libosdp_sys::osdp_trs_pin_entry__bindgen_ty_2 {
                size_bits: length.size_bits,
                offset_bits: length.offset_bits,
            },
            min_digits: value.min_digits,
            max_digits: value.max_digits,
            complete_on: value.end_on.bits(),
            num_messages: value.messages,
            language_id: value.language_id,
            msg_index: value.message_index,
            teo_prologue: value.t1_prologue,
            apdu: to_apdu(&value.apdu)?,
        })
    }
}

/// A transparent reader command, sent as [`Command::Trs`](crate::Command).
///
/// A card session is the run of commands from a [`TrsCommand::Start`] to
/// its [`TrsCommand::Stop`], in submission order. Inside a session LibOSDP
/// refuses any other command, and outside one it refuses card commands.
/// To abandon a session, [`flush`](crate::ControlPanel::flush) the PD's
/// commands and submit a `Stop`. A session that ends early fails the
/// commands left in it with [`CompletionError::Flushed`].
///
/// LibOSDP opens and closes the session on the PD itself, so a PD's
/// [`CommandHandler`] only sees [`Apdu`](TrsCommand::Apdu),
/// [`EnterPin`](TrsCommand::EnterPin) and [`CardScan`](TrsCommand::CardScan).
///
/// [`CompletionError::Flushed`]: crate::CompletionError::Flushed
/// [`CommandHandler`]: crate::CommandHandler
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum TrsCommand {
    /// Open a card session.
    Start,
    /// Close the session.
    Stop,
    /// Send a C-APDU to the card. It must fit the packet size in use on
    /// the link, which is smaller than the 258 bytes LibOSDP's buffer
    /// holds; LibOSDP refuses a larger one when it is submitted.
    Apdu(Vec<u8>),
    /// Have the user enter a PIN for the card.
    EnterPin(PinEntry),
    /// Ask whether a card is in the field.
    CardScan,
}

impl TryFrom<libosdp_sys::osdp_trs_cmd> for TrsCommand {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_trs_cmd) -> Result<Self> {
        let payload = value.__bindgen_anon_1;
        match value.command {
            libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_START => Ok(TrsCommand::Start),
            libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_STOP => Ok(TrsCommand::Stop),
            libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_SEND_APDU => {
                Ok(TrsCommand::Apdu(from_apdu(&unsafe { payload.apdu })?))
            }
            libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_ENTER_PIN => Ok(TrsCommand::EnterPin(
                unsafe { payload.pin_entry }.try_into()?,
            )),
            libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_CARD_SCAN => Ok(TrsCommand::CardScan),
            _ => Err(Error::Malformed("TRS command")),
        }
    }
}

impl TryFrom<TrsCommand> for libosdp_sys::osdp_trs_cmd {
    type Error = Error;

    fn try_from(value: TrsCommand) -> Result<Self> {
        let mut raw: libosdp_sys::osdp_trs_cmd = unsafe { core::mem::zeroed() };
        raw.command = match value {
            TrsCommand::Start => libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_START,
            TrsCommand::Stop => libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_STOP,
            TrsCommand::Apdu(apdu) => {
                raw.__bindgen_anon_1.apdu = to_apdu(&apdu)?;
                libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_SEND_APDU
            }
            TrsCommand::EnterPin(entry) => {
                raw.__bindgen_anon_1.pin_entry = entry.try_into()?;
                libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_ENTER_PIN
            }
            TrsCommand::CardScan => libosdp_sys::osdp_trs_cmd_e_OSDP_TRS_CMD_CARD_SCAN,
        };
        Ok(raw)
    }
}

/// How a card talks to the reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CardProtocol {
    /// ISO 7816 contact, T=0 or T=1.
    Contact,
    /// ISO 14443 A or B contactless.
    Contactless,
}

/// Whether a card is in a reader's field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CardStatus {
    /// The reader has not said. OSDP has no encoding for it, so a PD
    /// cannot send it: a reply carrying it fails instead of reaching the CP.
    Unknown,
    /// No card.
    NotPresent,
    /// A card, on an interface the reader did not name.
    Present,
    /// A card on the contactless interface.
    PresentContactless,
    /// A card on the contact interface.
    PresentContact,
}

/// A card that entered a reader's field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CardInfo {
    /// Reader: 0 is the first.
    pub reader: u8,
    /// How the card talks.
    pub protocol: CardProtocol,
    /// Card serial number; up to 32 bytes.
    pub csn: Vec<u8>,
    /// The card's ATR (contact) or ATS/ATQB (contactless); up to 64 bytes.
    pub protocol_data: Vec<u8>,
}

/// A card's answer to a C-APDU.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CardData {
    /// Reader: 0 is the first.
    pub reader: u8,
    /// How the exchange went, as the reader defines it.
    pub status: u8,
    /// The R-APDU.
    pub apdu: Vec<u8>,
}

/// How a PIN entry went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PinResult {
    /// Reader: 0 is the first.
    pub reader: u8,
    /// How it went, as the reader defines it.
    pub status: u8,
    /// How many attempts the user made.
    pub tries: u8,
}

/// A transparent reader reply, received as [`Event::Trs`](crate::Event).
/// LibOSDP hands these over as ordinary events: a CP matches an R-APDU to
/// its C-APDU by order within the session.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum TrsReply {
    /// A card entered the field.
    CardInfo(CardInfo),
    /// Whether a card is in the field.
    CardPresent {
        /// Reader: 0 is the first.
        reader: u8,
        /// Whether a card is there, and on which interface.
        status: CardStatus,
    },
    /// The card answered a C-APDU.
    CardData(CardData),
    /// PIN entry ended.
    PinComplete(PinResult),
    /// The reader or card reported an error, as the reader defines it. The
    /// CP receives it as an event, and the submission of the command it
    /// answers ends with
    /// [`CompletionError::Failed`](crate::CompletionError::Failed).
    Error(u8),
}

fn bytes_to<const N: usize>(data: &[u8]) -> Result<(u8, [u8; N])> {
    Ok((data.len() as u8, copy_into(data)?))
}

impl TryFrom<libosdp_sys::osdp_trs_reply> for TrsReply {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_trs_reply) -> Result<Self> {
        let payload = value.__bindgen_anon_1;
        match value.reply {
            libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_CARD_INFO => {
                let info = unsafe { payload.card_info };
                let protocol = match info.protocol {
                    libosdp_sys::osdp_trs_card_protocol_e_OSDP_TRS_CARD_PROTOCOL_CONTACT => {
                        CardProtocol::Contact
                    }
                    libosdp_sys::osdp_trs_card_protocol_e_OSDP_TRS_CARD_PROTOCOL_CONTACTLESS => {
                        CardProtocol::Contactless
                    }
                    _ => return Err(Error::Malformed("card protocol")),
                };
                let csn = info.csn.get(..info.csn_len as usize);
                let data = info.protocol_data.get(..info.protocol_data_len as usize);
                let (Some(csn), Some(data)) = (csn, data) else {
                    return Err(Error::Malformed("card info length"));
                };
                Ok(TrsReply::CardInfo(CardInfo {
                    reader: info.reader,
                    protocol,
                    csn: csn.to_vec(),
                    protocol_data: data.to_vec(),
                }))
            }
            libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_CARD_PRESENT => {
                let present = unsafe { payload.card_present };
                let status = match present.status {
                    libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_STATUS_UNKNOWN => {
                        CardStatus::Unknown
                    }
                    libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_NOT_PRESENT => {
                        CardStatus::NotPresent
                    }
                    libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_PRESENT => {
                        CardStatus::Present
                    }
                    libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_PRESENT_CONTACTLESS => {
                        CardStatus::PresentContactless
                    }
                    libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_PRESENT_CONTACT => {
                        CardStatus::PresentContact
                    }
                    _ => return Err(Error::Malformed("card status")),
                };
                Ok(TrsReply::CardPresent {
                    reader: present.reader,
                    status,
                })
            }
            libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_CARD_DATA => {
                let data = unsafe { payload.card_data };
                Ok(TrsReply::CardData(CardData {
                    reader: data.reader,
                    status: data.status,
                    apdu: from_apdu(&data.apdu)?,
                }))
            }
            libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_PIN_COMPLETE => {
                let pin = unsafe { payload.pin_complete };
                Ok(TrsReply::PinComplete(PinResult {
                    reader: pin.reader,
                    status: pin.status,
                    tries: pin.tries,
                }))
            }
            libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_ERROR => {
                Ok(TrsReply::Error(unsafe { payload.error }.code))
            }
            _ => Err(Error::Malformed("TRS reply")),
        }
    }
}

impl TryFrom<TrsReply> for libosdp_sys::osdp_trs_reply {
    type Error = Error;

    fn try_from(value: TrsReply) -> Result<Self> {
        let mut raw: libosdp_sys::osdp_trs_reply = unsafe { core::mem::zeroed() };
        raw.reply = match value {
            TrsReply::CardInfo(info) => {
                let (csn_len, csn) = bytes_to::<CSN_MAX>(&info.csn)?;
                let (protocol_data_len, protocol_data) =
                    bytes_to::<PROTOCOL_DATA_MAX>(&info.protocol_data)?;
                raw.__bindgen_anon_1.card_info = libosdp_sys::osdp_trs_card_info {
                    reader: info.reader,
                    protocol: match info.protocol {
                        CardProtocol::Contact => {
                            libosdp_sys::osdp_trs_card_protocol_e_OSDP_TRS_CARD_PROTOCOL_CONTACT
                        }
                        CardProtocol::Contactless => {
                            libosdp_sys::osdp_trs_card_protocol_e_OSDP_TRS_CARD_PROTOCOL_CONTACTLESS
                        }
                    },
                    csn_len,
                    csn,
                    protocol_data_len,
                    protocol_data,
                };
                libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_CARD_INFO
            }
            TrsReply::CardPresent { reader, status } => {
                raw.__bindgen_anon_1.card_present = libosdp_sys::osdp_trs_card_present {
                    reader,
                    status: match status {
                        CardStatus::Unknown => {
                            libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_STATUS_UNKNOWN
                        }
                        CardStatus::NotPresent => {
                            libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_NOT_PRESENT
                        }
                        CardStatus::Present => {
                            libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_PRESENT
                        }
                        CardStatus::PresentContactless => {
                            libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_PRESENT_CONTACTLESS
                        }
                        CardStatus::PresentContact => {
                            libosdp_sys::osdp_trs_card_status_e_OSDP_TRS_CARD_PRESENT_CONTACT
                        }
                    },
                };
                libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_CARD_PRESENT
            }
            TrsReply::CardData(data) => {
                raw.__bindgen_anon_1.card_data = libosdp_sys::osdp_trs_card_data {
                    reader: data.reader,
                    status: data.status,
                    apdu: to_apdu(&data.apdu)?,
                };
                libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_CARD_DATA
            }
            TrsReply::PinComplete(pin) => {
                raw.__bindgen_anon_1.pin_complete = libosdp_sys::osdp_trs_pin_complete {
                    reader: pin.reader,
                    status: pin.status,
                    tries: pin.tries,
                };
                libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_PIN_COMPLETE
            }
            TrsReply::Error(code) => {
                raw.__bindgen_anon_1.error = libosdp_sys::osdp_trs_error { code };
                libosdp_sys::osdp_trs_reply_e_OSDP_TRS_REPLY_ERROR
            }
        };
        Ok(raw)
    }
}

/// Cadence of the background card presence scan, for readers that do not
/// report a smart card entering the field on their own; see
/// [`ControlPanel::trs_scan_enable`](crate::ControlPanel::trs_scan_enable).
/// Each time is a whole number of milliseconds up to 65.535 s, and
/// [`Duration::ZERO`], as in [`TrsScan::default`], selects LibOSDP's
/// default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TrsScan {
    /// Time in the reader's default mode between probes, where ordinary
    /// credential reads work; 200 ms by default. Shorter is more aggressive:
    /// it leaves less time for ordinary reads.
    pub mode0_dwell: Duration,
    /// Time spent in transparent mode per probe, watching for a card;
    /// 200 ms by default.
    pub mode1_dwell: Duration,
    /// How long transparent mode is held after a card is seen, for the
    /// application to open a session; 500 ms by default.
    pub hold: Duration,
}

impl TryFrom<TrsScan> for libosdp_sys::osdp_trs_scan_params {
    type Error = Error;

    fn try_from(value: TrsScan) -> Result<Self> {
        Ok(libosdp_sys::osdp_trs_scan_params {
            mode0_dwell_ms: to_count(value.mode0_dwell, MILLISECOND, "scan mode 0 dwell time")?,
            mode1_dwell_ms: to_count(value.mode1_dwell, MILLISECOND, "scan mode 1 dwell time")?,
            hold_ms: to_count(value.hold, MILLISECOND, "scan hold time")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn pin_entry() -> PinEntry {
        PinEntry {
            first_digit_timeout: Duration::from_secs(10),
            digit_timeout: Duration::from_secs(5),
            pin_block: PinBlock {
                format: PinFormat::Bcd,
                right_justify: false,
                offset_bits: 40,
                size_bytes: 8,
            },
            length_field: Some(PinLengthField {
                size_bits: 4,
                offset_bits: 40,
            }),
            min_digits: 4,
            max_digits: 12,
            end_on: PinEndOn::Key | PinEndOn::MaxDigits,
            messages: 1,
            language_id: 0x0409,
            message_index: 0,
            t1_prologue: [0; 3],
            apdu: vec![0x00, 0x20, 0x00, 0x80, 0x08],
        }
    }

    #[test]
    fn commands_round_trip() {
        for cmd in [
            TrsCommand::Start,
            TrsCommand::Apdu(vec![0x00, 0xA4, 0x04, 0x00]),
            TrsCommand::EnterPin(pin_entry()),
            TrsCommand::CardScan,
            TrsCommand::Stop,
        ] {
            let raw: libosdp_sys::osdp_trs_cmd = cmd.clone().try_into().unwrap();
            assert_eq!(TrsCommand::try_from(raw), Ok(cmd));
        }
    }

    #[test]
    fn a_pin_length_field_is_absent_or_sized() {
        let entry = PinEntry {
            length_field: None,
            ..pin_entry()
        };
        let raw: libosdp_sys::osdp_trs_cmd =
            TrsCommand::EnterPin(entry.clone()).try_into().unwrap();
        assert_eq!(TrsCommand::try_from(raw), Ok(TrsCommand::EnterPin(entry)));
        let entry = PinEntry {
            length_field: Some(PinLengthField {
                size_bits: 0,
                offset_bits: 40,
            }),
            ..pin_entry()
        };
        let raw: Result<libosdp_sys::osdp_trs_cmd> = TrsCommand::EnterPin(entry).try_into();
        assert!(matches!(
            raw,
            Err(Error::InvalidArgument("PIN length field size"))
        ));
    }

    #[test]
    fn replies_round_trip() {
        for reply in [
            TrsReply::CardInfo(CardInfo {
                reader: 0,
                protocol: CardProtocol::Contactless,
                csn: vec![1, 2, 3, 4],
                protocol_data: vec![0x3B, 0x8F],
            }),
            TrsReply::CardPresent {
                reader: 1,
                status: CardStatus::PresentContact,
            },
            TrsReply::CardData(CardData {
                reader: 0,
                status: 0,
                apdu: vec![0x90, 0x00],
            }),
            TrsReply::PinComplete(PinResult {
                reader: 0,
                status: 0,
                tries: 1,
            }),
            TrsReply::Error(7),
        ] {
            let raw: libosdp_sys::osdp_trs_reply = reply.clone().try_into().unwrap();
            assert_eq!(TrsReply::try_from(raw), Ok(reply));
        }
    }

    #[test]
    fn oversized_apdus_and_bad_timeouts_are_refused() {
        let raw: Result<libosdp_sys::osdp_trs_cmd> =
            TrsCommand::Apdu(vec![0; APDU_MAX + 1]).try_into();
        assert!(matches!(raw, Err(Error::TooLong { max: 258 })));
        let entry = PinEntry {
            digit_timeout: Duration::from_millis(1500),
            ..pin_entry()
        };
        let raw: Result<libosdp_sys::osdp_trs_cmd> = TrsCommand::EnterPin(entry).try_into();
        assert!(matches!(raw, Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn scan_times_are_whole_milliseconds() {
        let raw: libosdp_sys::osdp_trs_scan_params = TrsScan {
            hold: Duration::from_millis(750),
            ..TrsScan::default()
        }
        .try_into()
        .unwrap();
        assert_eq!(
            (raw.mode0_dwell_ms, raw.mode1_dwell_ms, raw.hold_ms),
            (0, 0, 750)
        );
        for scan in [
            TrsScan {
                mode0_dwell: Duration::from_micros(1500),
                ..TrsScan::default()
            },
            TrsScan {
                hold: Duration::from_secs(66),
                ..TrsScan::default()
            },
        ] {
            let raw: Result<libosdp_sys::osdp_trs_scan_params> = scan.try_into();
            assert!(matches!(raw, Err(Error::InvalidArgument(_))));
        }
    }
}
