//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! A CP can reach a smart card in a PD's reader: read a PIV data object
//! from it, or have it answer a cryptographic challenge. These travel as
//! multi-part messages, and a PD runs one at a time.

use crate::Error;
use alloc::vec::Vec;

type Result<T> = core::result::Result<T, Error>;

const MAX_LEN: usize = libosdp_sys::OSDP_PIV_DATA_MAX_LEN as usize;

/// Copy `data` into LibOSDP's buffer, refusing it when empty or too long.
fn to_payload(data: &[u8], what: &'static str) -> Result<(u16, [u8; MAX_LEN])> {
    if data.is_empty() {
        return Err(Error::InvalidArgument(what));
    }
    let mut payload = [0; MAX_LEN];
    payload
        .get_mut(..data.len())
        .ok_or(Error::TooLong { max: MAX_LEN })?
        .copy_from_slice(data);
    Ok((data.len() as u16, payload))
}

/// Ask a PD's smart card for a PIV data object, or part of one
/// (osdp_PIVDATA).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PivRequest {
    /// PIV object ID, per NIST SP 800-73-4 Part 1.
    pub object_id: [u8; 3],
    /// Element of the object.
    pub element: u8,
    /// Offset into the element.
    pub offset: u8,
}

impl From<libosdp_sys::osdp_cmd_pivdata> for PivRequest {
    fn from(value: libosdp_sys::osdp_cmd_pivdata) -> Self {
        PivRequest {
            object_id: value.oid,
            element: value.element,
            offset: value.offset,
        }
    }
}

impl From<PivRequest> for libosdp_sys::osdp_cmd_pivdata {
    fn from(value: PivRequest) -> Self {
        libosdp_sys::osdp_cmd_pivdata {
            oid: value.object_id,
            element: value.element,
            offset: value.offset,
        }
    }
}

/// A cryptographic challenge for a PD's smart card (osdp_GENAUTH or
/// osdp_CRAUTH).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Challenge {
    /// Algorithm, per ISO 7816-4:2005 7.5.5.
    pub algorithm: u8,
    /// Key reference.
    pub key: u8,
    /// The challenge; 1 to 256 bytes when sent.
    pub data: Vec<u8>,
}

impl TryFrom<libosdp_sys::osdp_cmd_auth> for Challenge {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd_auth) -> Result<Self> {
        let data = value
            .data
            .get(..value.length as usize)
            .ok_or(Error::Malformed("challenge length"))?;
        Ok(Challenge {
            algorithm: value.algorithm,
            key: value.key,
            data: data.to_vec(),
        })
    }
}

impl TryFrom<Challenge> for libosdp_sys::osdp_cmd_auth {
    type Error = Error;

    fn try_from(value: Challenge) -> Result<Self> {
        let (length, data) = to_payload(&value.data, "challenge is empty")?;
        Ok(libosdp_sys::osdp_cmd_auth {
            algorithm: value.algorithm,
            key: value.key,
            length,
            data,
        })
    }
}

/// A smart card's answer to a [`PivRequest`] or a [`Challenge`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PivReply {
    /// The object data or challenge response; 1 to 256 bytes when sent.
    pub data: Vec<u8>,
}

impl TryFrom<libosdp_sys::osdp_event_piv_reply> for PivReply {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_event_piv_reply) -> Result<Self> {
        let data = value
            .data
            .get(..value.length as usize)
            .ok_or(Error::Malformed("PIV reply length"))?;
        Ok(PivReply {
            data: data.to_vec(),
        })
    }
}

impl TryFrom<PivReply> for libosdp_sys::osdp_event_piv_reply {
    type Error = Error;

    fn try_from(value: PivReply) -> Result<Self> {
        let (length, data) = to_payload(&value.data, "PIV reply is empty")?;
        Ok(libosdp_sys::osdp_event_piv_reply { length, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn payloads_must_fit_and_not_be_empty() {
        let empty = Challenge {
            algorithm: 0x11,
            key: 0x9E,
            data: vec![],
        };
        assert!(matches!(
            libosdp_sys::osdp_cmd_auth::try_from(empty),
            Err(Error::InvalidArgument(_))
        ));
        let reply = PivReply {
            data: vec![0; MAX_LEN + 1],
        };
        assert!(matches!(
            libosdp_sys::osdp_event_piv_reply::try_from(reply),
            Err(Error::TooLong { max: 256 })
        ));
    }
}
