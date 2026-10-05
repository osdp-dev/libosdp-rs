//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! CP interacts with and controls PDs by sending commands to it. These commands
//! are specified by OSDP specification. This module is responsible to handling
//! such commands though [`Command`].

use crate::Error;
use crate::StatusReport;
use alloc::vec::Vec;

use super::ConvertEndian;

type Result<T> = core::result::Result<T, Error>;

/// LED Colors as specified in OSDP for the on_color/off_color parameters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LedColor {
    /// No Color
    #[default]
    None,

    /// Red Color
    Red,

    /// Green Color
    Green,

    /// Amber Color
    Amber,

    /// Blue Color
    Blue,

    /// Magenta Color
    Magenta,

    /// Cyan Color
    Cyan,

    /// Unknown/Unsupported Color
    Unknown(u8),
}

impl From<u8> for LedColor {
    fn from(value: u8) -> Self {
        match value as libosdp_sys::osdp_led_color_e {
            libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_NONE => LedColor::None,
            libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_RED => LedColor::Red,
            libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_GREEN => LedColor::Green,
            libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_AMBER => LedColor::Amber,
            libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_BLUE => LedColor::Blue,
            libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_MAGENTA => LedColor::Magenta,
            libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_CYAN => LedColor::Cyan,
            cc => LedColor::Unknown(cc),
        }
    }
}

impl From<LedColor> for u8 {
    fn from(value: LedColor) -> Self {
        match value {
            LedColor::None => libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_NONE as u8,
            LedColor::Red => libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_RED as u8,
            LedColor::Green => libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_GREEN as u8,
            LedColor::Amber => libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_AMBER as u8,
            LedColor::Blue => libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_BLUE as u8,
            LedColor::Magenta => libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_MAGENTA as u8,
            LedColor::Cyan => libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_CYAN as u8,
            LedColor::Unknown(cc) => cc,
        }
    }
}

/// LED params sub-structure. Part of LED command: LedCommand
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LedParams {
    /// Control code serves different purposes based on which member of
    /// [`LedCommand`] it is used with. They are,
    ///
    /// temporary:
    ///
    /// 0 - NOP - do not alter this LED's temporary settings
    /// 1 - Cancel any temporary operation and display this LED's permanent state immediately
    /// 2 - Set the temporary state as given and start timer immediately
    ///
    /// permanent:
    ///
    /// 0 - NOP - do not alter this LED's permanent settings
    /// 1 - Set the permanent state as given
    pub control_code: u8,

    /// The ON duration of the flash, in units of 100 ms
    pub on_count: u8,

    /// The OFF duration of the flash, in units of 100 ms
    pub off_count: u8,

    /// Color to set during the ON timer
    pub on_color: LedColor,

    /// Color to set during the Off timer
    pub off_color: LedColor,

    /// Time in units of 100 ms (only for temporary mode)
    pub timer_count: u16,
}

impl From<libosdp_sys::osdp_cmd_led_params> for LedParams {
    fn from(value: libosdp_sys::osdp_cmd_led_params) -> Self {
        LedParams {
            control_code: value.control_code,
            on_count: value.on_count,
            off_count: value.off_count,
            on_color: value.on_color.into(),
            off_color: value.off_color.into(),
            timer_count: value.timer_count,
        }
    }
}

impl From<LedParams> for libosdp_sys::osdp_cmd_led_params {
    fn from(value: LedParams) -> Self {
        libosdp_sys::osdp_cmd_led_params {
            control_code: value.control_code,
            on_count: value.on_count,
            off_count: value.off_count,
            on_color: value.on_color.into(),
            off_color: value.off_color.into(),
            timer_count: value.timer_count,
        }
    }
}

/// Command to control the behavior of it's on-board LEDs
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LedCommand {
    /// Reader (another device connected to this PD) for which this command is
    /// issued for.
    ///
    /// 0 - self
    /// 1 - fist connected reader
    /// 2 - second connected reader
    /// ....
    pub reader: u8,

    /// LED number to operate on; 0 = first LED, 1 = second LED, etc.
    pub led_number: u8,

    /// Temporary LED activity descriptor. This operation is ephemeral and
    /// interrupts any on going permanent activity.
    pub temporary: LedParams,

    /// Permanent LED activity descriptor. This operation continues till another
    /// permanent activity overwrites this state.
    pub permanent: LedParams,
}

impl From<libosdp_sys::osdp_cmd_led> for LedCommand {
    fn from(value: libosdp_sys::osdp_cmd_led) -> Self {
        LedCommand {
            reader: value.reader,
            led_number: value.led_number,
            temporary: value.temporary.into(),
            permanent: value.permanent.into(),
        }
    }
}

impl From<LedCommand> for libosdp_sys::osdp_cmd_led {
    fn from(value: LedCommand) -> Self {
        libosdp_sys::osdp_cmd_led {
            reader: value.reader,
            led_number: value.led_number,
            temporary: value.temporary.into(),
            permanent: value.permanent.into(),
        }
    }
}

/// Command to control the behavior of a buzzer in the PD
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuzzerCommand {
    /// Reader (another device connected to this PD) for which this command is
    /// issued for.
    ///
    /// 0 - self
    /// 1 - fist connected reader
    /// 2 - second connected reader
    /// ....
    pub reader: u8,

    /// Control code instructs the operation to perform:
    ///
    /// 0 - no tone
    /// 1 - off
    /// 2 - default tone
    /// 3+ - TBD
    pub control_code: u8,

    /// The ON duration of the flash, in units of 100 ms
    pub on_count: u8,

    /// The OFF duration of the flash, in units of 100 ms
    pub off_count: u8,

    /// The number of times to repeat the ON/OFF cycle; Setting this value to 0
    /// indicates the action is to be repeated forever.
    pub rep_count: u8,
}

impl From<libosdp_sys::osdp_cmd_buzzer> for BuzzerCommand {
    fn from(value: libosdp_sys::osdp_cmd_buzzer) -> Self {
        BuzzerCommand {
            reader: value.reader,
            control_code: value.control_code,
            on_count: value.on_count,
            off_count: value.off_count,
            rep_count: value.rep_count,
        }
    }
}

impl From<BuzzerCommand> for libosdp_sys::osdp_cmd_buzzer {
    fn from(value: BuzzerCommand) -> Self {
        libosdp_sys::osdp_cmd_buzzer {
            reader: value.reader,
            control_code: value.control_code,
            on_count: value.on_count,
            off_count: value.off_count,
            rep_count: value.rep_count,
        }
    }
}

/// Command to manipulate the on-board display unit (Can be LED, LCD, 7-Segment,
/// etc.,) on the PD.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextCommand {
    /// Reader (another device connected to this PD) for which this command is
    /// issued for.
    ///
    /// 0 - self
    /// 1 - fist connected reader
    /// 2 - second connected reader
    /// ....
    pub reader: u8,

    /// Control code instructs the operation to perform:
    ///
    /// 1 - permanent text, no wrap
    /// 2 - permanent text, with wrap
    /// 3 - temporary text, no wrap
    /// 4 - temporary text, with wrap
    pub control_code: u8,

    /// duration to display temporary text, in seconds
    pub temp_time: u8,

    /// row to display the first character (1 indexed)
    pub offset_row: u8,

    /// column to display the first character (1 indexed)
    pub offset_col: u8,

    /// The string to display (ASCII codes)
    pub data: Vec<u8>,
}

impl From<libosdp_sys::osdp_cmd_text> for TextCommand {
    fn from(value: libosdp_sys::osdp_cmd_text) -> Self {
        let n = value.length as usize;
        let data = value.data[0..n].to_vec();
        TextCommand {
            reader: value.reader,
            control_code: value.control_code,
            temp_time: value.temp_time,
            offset_row: value.offset_row,
            offset_col: value.offset_col,
            data,
        }
    }
}

impl From<TextCommand> for libosdp_sys::osdp_cmd_text {
    fn from(value: TextCommand) -> Self {
        let mut data = [0; libosdp_sys::OSDP_CMD_TEXT_MAX_LEN as usize];
        data[..value.data.len()].copy_from_slice(&value.data[..]);
        libosdp_sys::osdp_cmd_text {
            reader: value.reader,
            control_code: value.control_code,
            temp_time: value.temp_time,
            offset_row: value.offset_row,
            offset_col: value.offset_col,
            length: value.data.len() as u8,
            data,
        }
    }
}

/// Command to control digital output exposed by the PD.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutputCommand {
    /// The output number this to apply this action.
    ///
    /// 0 - First Output
    /// 1 - Second Output
    /// ....
    pub output_no: u8,

    /// Control code instructs the operation to perform:
    ///
    /// 0 - NOP – do not alter this output
    /// 1 - set the permanent state to OFF, abort timed operation (if any)
    /// 2 - set the permanent state to ON, abort timed operation (if any)
    /// 3 - set the permanent state to OFF, allow timed operation to complete
    /// 4 - set the permanent state to ON, allow timed operation to complete
    /// 5 - set the temporary state to ON, resume perm state on timeout
    /// 6 - set the temporary state to OFF, resume permanent state on timeout
    pub control_code: u8,

    ///  Time in units of 100 ms
    pub timer_count: u16,
}

impl From<libosdp_sys::osdp_cmd_output> for OutputCommand {
    fn from(value: libosdp_sys::osdp_cmd_output) -> Self {
        OutputCommand {
            output_no: value.output_no,
            control_code: value.control_code,
            timer_count: value.timer_count,
        }
    }
}

impl From<OutputCommand> for libosdp_sys::osdp_cmd_output {
    fn from(value: OutputCommand) -> Self {
        libosdp_sys::osdp_cmd_output {
            output_no: value.output_no,
            control_code: value.control_code,
            timer_count: value.timer_count,
        }
    }
}

/// Command to set the communication parameters for the PD. The effects of this
/// command is expected to be be stored in PD's non-volatile memory as the CP
/// will expect the PD to be in this state moving forward.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComSet {
    /// Unit ID to which this PD will respond after the change takes effect.
    pub address: u8,
    /// Baud rate.
    /// Valid values: 9600, 19200, 38400, 115200, 230400.
    pub baud_rate: u32,
}

impl From<libosdp_sys::osdp_cmd_comset> for ComSet {
    fn from(value: libosdp_sys::osdp_cmd_comset) -> Self {
        ComSet {
            address: value.address,
            baud_rate: value.baud_rate,
        }
    }
}

impl From<ComSet> for libosdp_sys::osdp_cmd_comset {
    fn from(value: ComSet) -> Self {
        libosdp_sys::osdp_cmd_comset {
            address: value.address,
            baud_rate: value.baud_rate,
        }
    }
}

/// Command to set secure channel keys to the PD.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct KeySetCommand {
    key_type: u8,
    /// Key data
    pub data: Vec<u8>,
}

impl KeySetCommand {
    /// Create a new SCBK KeySet command for a given key
    ///
    /// # Arguments
    ///
    /// * `key` - 16 bytes of secure channel base key
    pub fn new_scbk(key: [u8; 16]) -> Self {
        let data = key.to_vec();
        Self { key_type: 1, data }
    }
}

impl From<libosdp_sys::osdp_cmd_keyset> for KeySetCommand {
    fn from(value: libosdp_sys::osdp_cmd_keyset) -> Self {
        let n = value.length as usize;
        let data = value.data[0..n].to_vec();
        KeySetCommand {
            key_type: value.type_,
            data,
        }
    }
}

impl From<KeySetCommand> for libosdp_sys::osdp_cmd_keyset {
    fn from(value: KeySetCommand) -> Self {
        let mut data = [0; libosdp_sys::OSDP_CMD_KEYSET_KEY_MAX_LEN as usize];
        data[..value.data.len()].copy_from_slice(&value.data[..]);
        libosdp_sys::osdp_cmd_keyset {
            type_: value.key_type,
            length: value.data.len() as u8,
            data,
        }
    }
}

/// Command to to act as a wrapper for manufacturer specific commands
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MfgCommand {
    /// 3-byte IEEE assigned OUI used as vendor code
    pub vendor_code: (u8, u8, u8),

    /// Command data (if any)
    pub data: Vec<u8>,
}

impl From<libosdp_sys::osdp_cmd_mfg> for MfgCommand {
    fn from(value: libosdp_sys::osdp_cmd_mfg) -> Self {
        let n = value.length as usize;
        let data = value.data[0..n].to_vec();
        let bytes = value.vendor_code.to_le_bytes();
        let vendor_code: (u8, u8, u8) = (bytes[0], bytes[1], bytes[2]);
        MfgCommand {
            vendor_code,
            data,
        }
    }
}

impl From<MfgCommand> for libosdp_sys::osdp_cmd_mfg {
    fn from(value: MfgCommand) -> Self {
        let mut data = [0; libosdp_sys::OSDP_CMD_MFG_MAX_DATALEN as usize];
        data[..value.data.len()].copy_from_slice(&value.data[..]);
        libosdp_sys::osdp_cmd_mfg {
            vendor_code: value.vendor_code.as_le(),
            length: value.data.len() as u8,
            data,
        }
    }
}

/// File transfer command flag used to cancel ongoing transfers (not sent on OSDP channel).
pub const OSDP_CMD_FILE_TX_FLAG_CANCEL: u32 = 1 << 31;

/// Command to kick-off a file transfer to the PD.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FileTxCommand {
    /// Pre-agreed file ID between CP and PD
    pub id: i32,
    /// Reserved and set to zero by OSDP spec.
    /// Note that the upper bits are used by libosdp internally (IOW, not sent
    /// over the OSDP bus). Currently the following flags are defined:
    /// - OSDP_CMD_FILE_TX_FLAG_CANCEL
    pub flags: u32,
}

impl From<libosdp_sys::osdp_cmd_file_tx> for FileTxCommand {
    fn from(value: libosdp_sys::osdp_cmd_file_tx) -> Self {
        FileTxCommand {
            id: value.id,
            flags: value.flags,
        }
    }
}

impl From<FileTxCommand> for libosdp_sys::osdp_cmd_file_tx {
    fn from(value: FileTxCommand) -> Self {
        libosdp_sys::osdp_cmd_file_tx {
            id: value.id,
            flags: value.flags,
        }
    }
}

/// CP interacts with and controls PDs by sending commands to it. The commands
/// in this enum are specified by OSDP specification.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Command {
    /// Command to control the behavior of it’s on-board LEDs
    Led(LedCommand),

    /// Command to control the behavior of a buzzer in the PD
    Buzzer(BuzzerCommand),

    /// Command to manipulate the on-board display unit (Can be LED, LCD,
    /// 7-Segment, etc.,) on the PD
    Text(TextCommand),

    /// Command to control digital output exposed by the PD
    Output(OutputCommand),

    /// Command to request setting the communication parameters for the PD.
    ComSet(ComSet),

    /// Set communication parameter completed.
    /// The effects of this command is expected to be be stored in PD’s non-volatile
    /// memory as the CP will expect the PD to be in this state moving forward
    ComSetDone(ComSet),

    /// Command to set secure channel keys to the PD
    KeySet(KeySetCommand),

    /// Command to to act as a wrapper for manufacturer specific commands
    Mfg(MfgCommand),

    /// Command to kick-off a file transfer to the PD
    FileTx(FileTxCommand),

    /// Command to query status from the PD
    Status(StatusReport),
}

impl From<Command> for libosdp_sys::osdp_cmd {
    fn from(value: Command) -> Self {
        match value {
            Command::Led(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_LED,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 {
                    led: c.clone().into(),
                },
            },
            Command::Buzzer(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_BUZZER,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 { buzzer: c.into() },
            },
            Command::Text(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_TEXT,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 {
                    text: c.clone().into(),
                },
            },
            Command::Output(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_OUTPUT,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 { output: c.into() },
            },
            Command::ComSet(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_COMSET,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 { comset: c.into() },
            },
            Command::ComSetDone(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_COMSET_DONE,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 { comset: c.into() },
            },
            Command::KeySet(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_KEYSET,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 {
                    keyset: c.clone().into(),
                },
            },
            Command::Mfg(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_MFG,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 {
                    mfg: c.clone().into(),
                },
            },
            Command::FileTx(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_FILE_TX,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 { file_tx: c.into() },
            },
            Command::Status(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_STATUS,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 { status: c.into() },
            },
        }
    }
}

impl TryFrom<libosdp_sys::osdp_cmd> for Command {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd) -> Result<Self> {
        match value.id {
            libosdp_sys::osdp_cmd_e_OSDP_CMD_LED => {
                Ok(Command::Led(unsafe { value.__bindgen_anon_1.led.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_BUZZER => {
                Ok(Command::Buzzer(unsafe { value.__bindgen_anon_1.buzzer.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_TEXT => {
                Ok(Command::Text(unsafe { value.__bindgen_anon_1.text.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_OUTPUT => {
                Ok(Command::Output(unsafe { value.__bindgen_anon_1.output.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_COMSET => {
                Ok(Command::ComSet(unsafe { value.__bindgen_anon_1.comset.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_COMSET_DONE => {
                Ok(Command::ComSetDone(unsafe { value.__bindgen_anon_1.comset.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_KEYSET => {
                Ok(Command::KeySet(unsafe { value.__bindgen_anon_1.keyset.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_MFG => {
                Ok(Command::Mfg(unsafe { value.__bindgen_anon_1.mfg.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_FILE_TX => {
                Ok(Command::FileTx(unsafe { value.__bindgen_anon_1.file_tx.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_STATUS => {
                let data = unsafe { value.__bindgen_anon_1.status.try_into() }?;
                Ok(Command::Status(data))
            }
            _ => Err(Error::Malformed("command id")),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::MfgCommand;
    use libosdp_sys::osdp_cmd_mfg;

    #[test]
    fn test_command_mfg() {
        let cmd = MfgCommand {
            vendor_code: (0x05, 0x07, 0x09),
            data: vec![0x55, 0xAA],
        };
        let cmd_struct: osdp_cmd_mfg = cmd.clone().into();

        assert_eq!(cmd_struct.vendor_code, 0x90705);
        assert_eq!(cmd_struct.length, 2);
        assert_eq!(cmd_struct.data[0], 0x55);
        assert_eq!(cmd_struct.data[1], 0xAA);

        assert_eq!(cmd, cmd_struct.into());
    }
}
