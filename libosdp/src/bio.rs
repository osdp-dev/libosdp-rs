//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! A CP can ask a PD with a biometric reader to scan a body part and send
//! the scan back, or to match the scan against a template it sends.

use crate::{copy_into, Error};
use alloc::vec::Vec;

type Result<T> = core::result::Result<T, Error>;

const MATCH_TEMPLATE_MAX: usize = libosdp_sys::OSDP_CMD_BIOMATCH_MAX_TEMPLATE_LEN as usize;
const READ_TEMPLATE_MAX: usize = libosdp_sys::OSDP_EVENT_BIOREADR_MAX_TEMPLATE_LEN as usize;

/// An enum carried in one byte, whose values OSDP defines only in part.
macro_rules! byte_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident = $value:path,)* }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "defmt", derive(defmt::Format))]
        pub enum $name {
            $($(#[$vmeta])* $variant,)*
            /// A value OSDP does not define.
            Other(u8),
        }

        impl From<u8> for $name {
            fn from(value: u8) -> Self {
                match value {
                    $($value => $name::$variant,)*
                    other => $name::Other(other),
                }
            }
        }

        impl From<$name> for u8 {
            fn from(value: $name) -> Self {
                match value {
                    $($name::$variant => $value,)*
                    $name::Other(other) => other,
                }
            }
        }
    };
}

byte_enum! {
    /// The body part a biometric reader scans.
    BioType {
        /// Whatever the reader scans by default.
        NotSpecified = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_NOT_SPECIFIED,
        /// Right thumb print.
        RightThumb = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_RIGHT_THUMB_PRINT,
        /// Right index finger print.
        RightIndexFinger = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_RIGHT_INDEX_FINGER_PRINT,
        /// Right middle finger print.
        RightMiddleFinger =
            libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_RIGHT_MIDDLE_FINGER_PRINT,
        /// Right ring finger print.
        RightRingFinger = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_RIGHT_RING_FINGER_PRINT,
        /// Right little finger print.
        RightLittleFinger =
            libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_RIGHT_LITTLE_FINGER_PRINT,
        /// Left thumb print.
        LeftThumb = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_LEFT_THUMB_PRINT,
        /// Left index finger print.
        LeftIndexFinger = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_LEFT_INDEX_FINGER_PRINT,
        /// Left middle finger print.
        LeftMiddleFinger =
            libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_LEFT_MIDDLE_FINGER_PRINT,
        /// Left ring finger print.
        LeftRingFinger = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_LEFT_RING_FINGER_PRINT,
        /// Left little finger print.
        LeftLittleFinger =
            libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_LEFT_LITTLE_FINGER_PRINT,
        /// Right iris scan.
        RightIris = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_RIGHT_IRIS_SCAN,
        /// Right retina scan.
        RightRetina = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_RIGHT_RETINA_SCAN,
        /// Left iris scan.
        LeftIris = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_LEFT_IRIS_SCAN,
        /// Left retina scan.
        LeftRetina = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_LEFT_RETINA_SCAN,
        /// Full face image.
        FullFace = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_FULL_FACE_IMAGE,
        /// Right hand geometry.
        RightHandGeometry = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_RIGHT_HAND_GEOMETRY,
        /// Left hand geometry.
        LeftHandGeometry = libosdp_sys::osdp_biometric_type_e_OSDP_BIO_TYPE_LEFT_HAND_GEOMETRY,
    }
}

byte_enum! {
    /// How biometric data is encoded.
    BioFormat {
        /// Whatever the reader produces by default; it reports which.
        NotSpecified = libosdp_sys::osdp_biometric_format_e_OSDP_BIO_FMT_NOT_SPECIFIED,
        /// Raw fingerprint data as a PGM image.
        RawPgm = libosdp_sys::osdp_biometric_format_e_OSDP_BIO_FMT_RAW_PGM,
        /// ANSI/INCITS 378 fingerprint template.
        AnsiIncits378 = libosdp_sys::osdp_biometric_format_e_OSDP_BIO_FMT_ANSI_INCITS_378,
    }
}

byte_enum! {
    /// How a biometric scan went.
    BioStatus {
        /// The scan succeeded; the rest of the reply is valid.
        Success = libosdp_sys::osdp_biometric_status_e_OSDP_BIO_STATUS_SUCCESS,
        /// The scan timed out.
        Timeout = libosdp_sys::osdp_biometric_status_e_OSDP_BIO_STATUS_TIMEOUT,
        /// The scan failed for an unknown reason.
        Error = libosdp_sys::osdp_biometric_status_e_OSDP_BIO_STATUS_UNKNOWN_ERROR,
    }
}

/// Ask a PD to scan a body part and send the scan back as an
/// [`Event::BioRead`](crate::Event::BioRead).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BioReadCommand {
    /// Reader to scan with: 0 is the PD itself, 1 its first attached
    /// reader, and so on.
    pub reader: u8,
    /// Body part to scan.
    pub kind: BioType,
    /// Format to send the scan in.
    pub format: BioFormat,
    /// Scan quality to aim for, normalised to 0-255.
    pub quality: u8,
}

impl From<libosdp_sys::osdp_cmd_bioread> for BioReadCommand {
    fn from(value: libosdp_sys::osdp_cmd_bioread) -> Self {
        BioReadCommand {
            reader: value.reader,
            kind: value.type_.into(),
            format: value.format.into(),
            quality: value.quality,
        }
    }
}

impl From<BioReadCommand> for libosdp_sys::osdp_cmd_bioread {
    fn from(value: BioReadCommand) -> Self {
        libosdp_sys::osdp_cmd_bioread {
            reader: value.reader,
            type_: value.kind.into(),
            format: value.format.into(),
            quality: value.quality,
        }
    }
}

/// Ask a PD to scan a body part and match the scan against `template`,
/// answering with an [`Event::BioMatch`](crate::Event::BioMatch).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BioMatchCommand {
    /// Reader to scan with: 0 is the PD itself, 1 its first attached
    /// reader, and so on.
    pub reader: u8,
    /// Body part to scan.
    pub kind: BioType,
    /// Format of `template`.
    pub format: BioFormat,
    /// Match score, normalised to 0-255, needed to accept the match.
    pub threshold: u8,
    /// Template to match the scan against; up to 128 bytes.
    pub template: Vec<u8>,
}

impl TryFrom<libosdp_sys::osdp_cmd_biomatch> for BioMatchCommand {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd_biomatch) -> Result<Self> {
        let template = value
            .data
            .get(..value.length as usize)
            .ok_or(Error::Malformed("biometric template length"))?;
        Ok(BioMatchCommand {
            reader: value.reader,
            kind: value.type_.into(),
            format: value.format.into(),
            threshold: value.quality,
            template: template.to_vec(),
        })
    }
}

impl TryFrom<BioMatchCommand> for libosdp_sys::osdp_cmd_biomatch {
    type Error = Error;

    fn try_from(value: BioMatchCommand) -> Result<Self> {
        Ok(libosdp_sys::osdp_cmd_biomatch {
            reader: value.reader,
            type_: value.kind.into(),
            format: value.format.into(),
            quality: value.threshold,
            length: value.template.len() as u16,
            data: copy_into::<MATCH_TEMPLATE_MAX>(&value.template)?,
        })
    }
}

/// A PD's answer to a [`BioReadCommand`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BioReadReply {
    /// Reader that scanned.
    pub reader: u8,
    /// How the scan went; the other fields are valid only on
    /// [`BioStatus::Success`].
    pub status: BioStatus,
    /// Body part scanned.
    pub kind: BioType,
    /// Scan quality, normalised to 0-255.
    pub quality: u8,
    /// The scan; up to 256 bytes.
    pub template: Vec<u8>,
}

impl TryFrom<libosdp_sys::osdp_event_bioreadr> for BioReadReply {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_event_bioreadr) -> Result<Self> {
        let template = value
            .data
            .get(..value.length as usize)
            .ok_or(Error::Malformed("biometric template length"))?;
        Ok(BioReadReply {
            reader: value.reader,
            status: value.status.into(),
            kind: value.type_.into(),
            quality: value.quality,
            template: template.to_vec(),
        })
    }
}

impl TryFrom<BioReadReply> for libosdp_sys::osdp_event_bioreadr {
    type Error = Error;

    fn try_from(value: BioReadReply) -> Result<Self> {
        Ok(libosdp_sys::osdp_event_bioreadr {
            reader: value.reader,
            status: value.status.into(),
            type_: value.kind.into(),
            quality: value.quality,
            length: value.template.len() as u16,
            data: copy_into::<READ_TEMPLATE_MAX>(&value.template)?,
        })
    }
}

/// A PD's answer to a [`BioMatchCommand`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BioMatchReply {
    /// Reader that scanned.
    pub reader: u8,
    /// How the scan went; `score` is valid only on
    /// [`BioStatus::Success`].
    pub status: BioStatus,
    /// How well the scan matched, from 0, no match, to 255.
    pub score: u8,
}

impl From<libosdp_sys::osdp_event_biomatchr> for BioMatchReply {
    fn from(value: libosdp_sys::osdp_event_biomatchr) -> Self {
        BioMatchReply {
            reader: value.reader,
            status: value.status.into(),
            score: value.score,
        }
    }
}

impl From<BioMatchReply> for libosdp_sys::osdp_event_biomatchr {
    fn from(value: BioMatchReply) -> Self {
        libosdp_sys::osdp_event_biomatchr {
            reader: value.reader,
            status: value.status.into(),
            score: value.score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn bytes_osdp_does_not_define_survive_as_other() {
        assert_eq!(BioType::from(0x11), BioType::LeftHandGeometry);
        assert_eq!(BioType::from(0x12), BioType::Other(0x12));
        assert_eq!(u8::from(BioStatus::Error), 0xFF);
        assert_eq!(BioStatus::from(7), BioStatus::Other(7));
        assert_eq!(u8::from(BioFormat::Other(9)), 9);
    }

    #[test]
    fn templates_past_libosdp_buffers_are_refused() {
        let command = BioMatchCommand {
            reader: 0,
            kind: BioType::RightIndexFinger,
            format: BioFormat::AnsiIncits378,
            threshold: 200,
            template: vec![0; MATCH_TEMPLATE_MAX + 1],
        };
        assert!(matches!(
            libosdp_sys::osdp_cmd_biomatch::try_from(command),
            Err(Error::TooLong { max: 128 })
        ));
        let reply = BioReadReply {
            reader: 0,
            status: BioStatus::Success,
            kind: BioType::FullFace,
            quality: 9,
            template: vec![0; READ_TEMPLATE_MAX + 1],
        };
        assert!(matches!(
            libosdp_sys::osdp_event_bioreadr::try_from(reply),
            Err(Error::TooLong { max: 256 })
        ));
    }
}
