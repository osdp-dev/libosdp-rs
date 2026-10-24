//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

//! PDs advertise what they can do to the CP by means of capabilities, one
//! per function.

use crate::Error;

/// A PD function that OSDP defines a capability for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CapFunction {
    /// This function indicates the ability to monitor the status of a switch
    /// using a two-wire electrical connection between the PD and the switch.
    /// The on/off position of the switch indicates the state of an external
    /// device.
    ///
    /// The PD may simply resolve all circuit states to an open/closed
    /// status, or it may implement supervision of the monitoring circuit.
    /// A supervised circuit is able to indicate circuit fault status in
    /// addition to open/closed status.
    ContactStatusMonitoring,

    /// This function provides a switched output, typically in the form of a
    /// relay. The Output has two states: active or inactive. The Control Panel
    /// (CP) can directly set the Output's state, or, if the PD supports timed
    /// operations, the CP can specify a time period for the activation of the
    /// Output.
    OutputControl,

    /// This capability indicates the form of the card data is presented to the
    /// Control Panel.
    CardDataFormat,

    /// This capability indicates the presence of and type of LEDs.
    LedControl,

    /// This capability indicates the presence of and type of an audible
    /// annunciator (buzzer or similar tone generator).
    AudibleOutput,

    /// This capability indicates that the PD supports a text display emulating
    /// character-based display terminals.
    TextOutput,

    /// This capability indicates that the type of date and time awareness or
    /// time keeping ability of the PD.
    TimeKeeping,

    /// All PDs must be able to support the checksum mode. This capability
    /// indicates if the PD is capable of supporting CRC mode.
    CheckCharacterSupport,

    /// This capability indicates the extent to which the PD supports
    /// communication security (Secure Channel Communication)
    CommunicationSecurity,

    /// This capability indicates the maximum size single message the PD can
    /// receive.
    ReceiveBufferSize,

    /// This capability indicates the maximum size multi-part message which the
    /// PD can handle.
    LargestCombinedMessage,

    /// This capability indicates whether the PD supports the transparent mode
    /// used for communicating directly with a smart card.
    SmartCardSupport,

    /// This capability indicates the number of credential reader devices
    /// present. Compliance levels are bit fields to be assigned as needed.
    Readers,

    /// This capability indicates the ability of the reader to handle biometric
    /// input.
    Biometrics,

    /// This capability indicates if the reader is capable of supporting
    /// Secure Pin Entry (SPE) for smart cards
    SecurePinEntry,

    /// This capability indicates the version of OSDP the PD supports
    /// Compliance Levels:
    ///   0 - Unspecified
    ///   1 - IEC 60839-11-5
    ///   2 - SIA OSDP 2.2
    OsdpVersion,
}

impl From<CapFunction> for libosdp_sys::osdp_pd_cap_function_code_e {
    fn from(value: CapFunction) -> Self {
        match value {
            CapFunction::ContactStatusMonitoring => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_CONTACT_STATUS_MONITORING
            }
            CapFunction::OutputControl => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_OUTPUT_CONTROL
            }
            CapFunction::CardDataFormat => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_CARD_DATA_FORMAT
            }
            CapFunction::LedControl => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_READER_LED_CONTROL
            }
            CapFunction::AudibleOutput => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_READER_AUDIBLE_OUTPUT
            }
            CapFunction::TextOutput => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_READER_TEXT_OUTPUT
            }
            CapFunction::TimeKeeping => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_TIME_KEEPING
            }
            CapFunction::CheckCharacterSupport => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_CHECK_CHARACTER_SUPPORT
            }
            CapFunction::CommunicationSecurity => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_COMMUNICATION_SECURITY
            }
            CapFunction::ReceiveBufferSize => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_RECEIVE_BUFFERSIZE
            }
            CapFunction::LargestCombinedMessage => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_LARGEST_COMBINED_MESSAGE_SIZE
            }
            CapFunction::SmartCardSupport => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_SMART_CARD_SUPPORT
            }
            CapFunction::Readers => libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_READERS,
            CapFunction::Biometrics => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_BIOMETRICS
            }
            CapFunction::SecurePinEntry => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_SECURE_PIN_ENTRY
            }
            CapFunction::OsdpVersion => {
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_OSDP_VERSION
            }
        }
    }
}

impl TryFrom<libosdp_sys::osdp_pd_cap_function_code_e> for CapFunction {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_pd_cap_function_code_e) -> Result<Self, Error> {
        match value {
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_CONTACT_STATUS_MONITORING => {
                Ok(CapFunction::ContactStatusMonitoring)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_OUTPUT_CONTROL => {
                Ok(CapFunction::OutputControl)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_CARD_DATA_FORMAT => {
                Ok(CapFunction::CardDataFormat)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_READER_LED_CONTROL => {
                Ok(CapFunction::LedControl)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_READER_AUDIBLE_OUTPUT => {
                Ok(CapFunction::AudibleOutput)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_READER_TEXT_OUTPUT => {
                Ok(CapFunction::TextOutput)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_TIME_KEEPING => {
                Ok(CapFunction::TimeKeeping)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_CHECK_CHARACTER_SUPPORT => {
                Ok(CapFunction::CheckCharacterSupport)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_COMMUNICATION_SECURITY => {
                Ok(CapFunction::CommunicationSecurity)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_RECEIVE_BUFFERSIZE => {
                Ok(CapFunction::ReceiveBufferSize)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_LARGEST_COMBINED_MESSAGE_SIZE => {
                Ok(CapFunction::LargestCombinedMessage)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_SMART_CARD_SUPPORT => {
                Ok(CapFunction::SmartCardSupport)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_READERS => {
                Ok(CapFunction::Readers)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_BIOMETRICS => {
                Ok(CapFunction::Biometrics)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_SECURE_PIN_ENTRY => {
                Ok(CapFunction::SecurePinEntry)
            }
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_OSDP_VERSION => {
                Ok(CapFunction::OsdpVersion)
            }
            _ => Err(Error::Malformed("capability function")),
        }
    }
}

/// What a PD can do for one [`CapFunction`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Capability {
    /// The function this capability describes.
    pub function: CapFunction,

    /// What the PD can do with the function; its meaning depends on the
    /// function.
    pub compliance: u8,

    /// How many of the function's entities the PD has: for
    /// [`CapFunction::LedControl`], the number of LEDs.
    pub items: u8,
}

impl Capability {
    /// Create a capability for `function`.
    pub fn new(function: CapFunction, compliance: u8, items: u8) -> Self {
        Self {
            function,
            compliance,
            items,
        }
    }
}

impl From<Capability> for libosdp_sys::osdp_pd_cap {
    fn from(value: Capability) -> Self {
        libosdp_sys::osdp_pd_cap {
            function_code: value.function.into(),
            compliance_level: value.compliance,
            num_items: value.items,
        }
    }
}

impl TryFrom<libosdp_sys::osdp_pd_cap> for Capability {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_pd_cap) -> Result<Self, Error> {
        Ok(Capability {
            function: value.function_code.try_into()?,
            compliance: value.compliance_level,
            items: value.num_items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_round_trips() {
        let cap = Capability::new(CapFunction::OsdpVersion, 2, 0);
        let raw: libosdp_sys::osdp_pd_cap = cap.into();
        assert_eq!(
            raw.function_code,
            libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_OSDP_VERSION
        );
        assert_eq!(Capability::try_from(raw), Ok(cap));
    }

    #[test]
    fn unknown_function_codes_are_malformed() {
        let raw = libosdp_sys::osdp_pd_cap {
            function_code: libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_SENTINEL,
            compliance_level: 0,
            num_items: 0,
        };
        assert_eq!(
            Capability::try_from(raw),
            Err(Error::Malformed("capability function"))
        );
    }
}
