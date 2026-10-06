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
use core::time::Duration;

use super::ConvertEndian;

type Result<T> = core::result::Result<T, Error>;

const TENTH_OF_A_SECOND: Duration = Duration::from_millis(100);
const SECOND: Duration = Duration::from_secs(1);

/// `duration` as a count of `unit`, refused unless it is a whole number of
/// them that fits `T`.
fn to_count<T: TryFrom<u64>>(duration: Duration, unit: Duration, what: &'static str) -> Result<T> {
    let nanos = |d: Duration| {
        d.as_secs()
            .checked_mul(1_000_000_000)
            .and_then(|n| n.checked_add(u64::from(d.subsec_nanos())))
            .ok_or(Error::InvalidArgument(what))
    };
    let (nanos, unit) = (nanos(duration)?, nanos(unit)?);
    if nanos % unit != 0 {
        return Err(Error::InvalidArgument(what));
    }
    T::try_from(nanos / unit).map_err(|_| Error::InvalidArgument(what))
}

fn from_count(count: impl Into<u32>, unit: Duration) -> Duration {
    unit * count.into()
}

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

    /// White Color
    White,

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
            libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_WHITE => LedColor::White,
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
            LedColor::White => libosdp_sys::osdp_led_color_e_OSDP_LED_COLOR_WHITE as u8,
            LedColor::Unknown(cc) => cc,
        }
    }
}

/// How an LED flashes: `on_color` for `on_time`, then `off_color` for
/// `off_time`, over and over. Both times are whole multiples of 100 ms, up
/// to 25.5 s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LedParams {
    /// How long each flash lasts.
    pub on_time: Duration,

    /// How long the LED stays in `off_color` between flashes.
    pub off_time: Duration,

    /// Color to set during the ON timer
    pub on_color: LedColor,

    /// Color to set during the Off timer
    pub off_color: LedColor,
}

impl LedParams {
    fn from_raw(raw: &libosdp_sys::osdp_cmd_led_params) -> Self {
        LedParams {
            on_time: from_count(raw.on_count, TENTH_OF_A_SECOND),
            off_time: from_count(raw.off_count, TENTH_OF_A_SECOND),
            on_color: raw.on_color.into(),
            off_color: raw.off_color.into(),
        }
    }

    fn to_raw(self, control_code: u8, timer: Duration) -> Result<libosdp_sys::osdp_cmd_led_params> {
        Ok(libosdp_sys::osdp_cmd_led_params {
            control_code,
            on_count: to_count(self.on_time, TENTH_OF_A_SECOND, "LED on time")?,
            off_count: to_count(self.off_time, TENTH_OF_A_SECOND, "LED off time")?,
            on_color: self.on_color.into(),
            off_color: self.off_color.into(),
            timer_count: to_count(timer, TENTH_OF_A_SECOND, "LED timer")?,
        })
    }
}

/// What to do with an LED's temporary state, which overrides its permanent
/// one while it runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LedTemporary {
    /// Leave it as it is.
    #[default]
    Keep,

    /// End it, and show the permanent state now.
    Cancel,

    /// Flash as `params` says until `timer` runs out, then return to the
    /// permanent state. The timer is a whole multiple of 100 ms, up to
    /// 6553.5 s.
    Set {
        /// How to flash.
        params: LedParams,
        /// How long to flash for.
        timer: Duration,
    },
}

impl TryFrom<libosdp_sys::osdp_cmd_led_params> for LedTemporary {
    type Error = Error;

    fn try_from(raw: libosdp_sys::osdp_cmd_led_params) -> Result<Self> {
        match raw.control_code as libosdp_sys::osdp_cmd_led_temporary_control_code_e {
            libosdp_sys::osdp_cmd_led_temporary_control_code_e_OSDP_CMD_LED_TEMPORARY_CC_NOP => {
                Ok(LedTemporary::Keep)
            }
            libosdp_sys::osdp_cmd_led_temporary_control_code_e_OSDP_CMD_LED_TEMPORARY_CC_CANCEL => {
                Ok(LedTemporary::Cancel)
            }
            libosdp_sys::osdp_cmd_led_temporary_control_code_e_OSDP_CMD_LED_TEMPORARY_CC_SET => {
                Ok(LedTemporary::Set {
                    params: LedParams::from_raw(&raw),
                    timer: from_count(raw.timer_count, TENTH_OF_A_SECOND),
                })
            }
            _ => Err(Error::Malformed("LED temporary control code")),
        }
    }
}

impl TryFrom<LedTemporary> for libosdp_sys::osdp_cmd_led_params {
    type Error = Error;

    fn try_from(value: LedTemporary) -> Result<Self> {
        let (code, params, timer) = match value {
            LedTemporary::Keep => (
                libosdp_sys::osdp_cmd_led_temporary_control_code_e_OSDP_CMD_LED_TEMPORARY_CC_NOP,
                LedParams::default(),
                Duration::ZERO,
            ),
            LedTemporary::Cancel => (
                libosdp_sys::osdp_cmd_led_temporary_control_code_e_OSDP_CMD_LED_TEMPORARY_CC_CANCEL,
                LedParams::default(),
                Duration::ZERO,
            ),
            LedTemporary::Set { params, timer } => (
                libosdp_sys::osdp_cmd_led_temporary_control_code_e_OSDP_CMD_LED_TEMPORARY_CC_SET,
                params,
                timer,
            ),
        };
        params.to_raw(code as u8, timer)
    }
}

/// What to do with an LED's permanent state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LedPermanent {
    /// Leave it as it is.
    #[default]
    Keep,

    /// Flash as the parameters say until told otherwise.
    Set(LedParams),
}

impl TryFrom<libosdp_sys::osdp_cmd_led_params> for LedPermanent {
    type Error = Error;

    fn try_from(raw: libosdp_sys::osdp_cmd_led_params) -> Result<Self> {
        match raw.control_code as libosdp_sys::osdp_cmd_led_permanent_control_code_e {
            libosdp_sys::osdp_cmd_led_permanent_control_code_e_OSDP_CMD_LED_PERMANENT_CC_NOP => {
                Ok(LedPermanent::Keep)
            }
            libosdp_sys::osdp_cmd_led_permanent_control_code_e_OSDP_CMD_LED_PERMANENT_CC_SET => {
                Ok(LedPermanent::Set(LedParams::from_raw(&raw)))
            }
            _ => Err(Error::Malformed("LED permanent control code")),
        }
    }
}

impl TryFrom<LedPermanent> for libosdp_sys::osdp_cmd_led_params {
    type Error = Error;

    fn try_from(value: LedPermanent) -> Result<Self> {
        let (code, params) = match value {
            LedPermanent::Keep => (
                libosdp_sys::osdp_cmd_led_permanent_control_code_e_OSDP_CMD_LED_PERMANENT_CC_NOP,
                LedParams::default(),
            ),
            LedPermanent::Set(params) => (
                libosdp_sys::osdp_cmd_led_permanent_control_code_e_OSDP_CMD_LED_PERMANENT_CC_SET,
                params,
            ),
        };
        params.to_raw(code as u8, Duration::ZERO)
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
    pub temporary: LedTemporary,

    /// Permanent LED activity descriptor. This operation continues till another
    /// permanent activity overwrites this state.
    pub permanent: LedPermanent,
}

impl TryFrom<libosdp_sys::osdp_cmd_led> for LedCommand {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd_led) -> Result<Self> {
        Ok(LedCommand {
            reader: value.reader,
            led_number: value.led_number,
            temporary: value.temporary.try_into()?,
            permanent: value.permanent.try_into()?,
        })
    }
}

impl TryFrom<LedCommand> for libosdp_sys::osdp_cmd_led {
    type Error = Error;

    fn try_from(value: LedCommand) -> Result<Self> {
        Ok(libosdp_sys::osdp_cmd_led {
            reader: value.reader,
            led_number: value.led_number,
            temporary: value.temporary.try_into()?,
            permanent: value.permanent.try_into()?,
        })
    }
}

/// What a buzzer should do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Buzzer {
    /// Stay silent.
    Off,

    /// Sound the default tone for `on`, then pause for `off`, `repeat`
    /// times; a `repeat` of 0 repeats forever. Both times are whole
    /// multiples of 100 ms, up to 25.5 s.
    Tone {
        /// How long each tone lasts.
        on: Duration,
        /// How long the pause between tones lasts.
        off: Duration,
        /// How many tones to sound; 0 sounds them forever.
        repeat: u8,
    },
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

    /// What the buzzer should do.
    pub action: Buzzer,
}

impl TryFrom<libosdp_sys::osdp_cmd_buzzer> for BuzzerCommand {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd_buzzer) -> Result<Self> {
        let action = match value.control_code {
            libosdp_sys::osdp_cmd_buzzer_control_code_e_OSDP_CMD_BUZZER_CC_NO_TONE
            | libosdp_sys::osdp_cmd_buzzer_control_code_e_OSDP_CMD_BUZZER_CC_OFF => Buzzer::Off,
            libosdp_sys::osdp_cmd_buzzer_control_code_e_OSDP_CMD_BUZZER_CC_DEFAULT_TONE => {
                Buzzer::Tone {
                    on: from_count(value.on_count, TENTH_OF_A_SECOND),
                    off: from_count(value.off_count, TENTH_OF_A_SECOND),
                    repeat: value.rep_count,
                }
            }
            _ => return Err(Error::Malformed("buzzer control code")),
        };
        Ok(BuzzerCommand {
            reader: value.reader,
            action,
        })
    }
}

impl TryFrom<BuzzerCommand> for libosdp_sys::osdp_cmd_buzzer {
    type Error = Error;

    fn try_from(value: BuzzerCommand) -> Result<Self> {
        let (control_code, on_count, off_count, rep_count) = match value.action {
            Buzzer::Off => (
                libosdp_sys::osdp_cmd_buzzer_control_code_e_OSDP_CMD_BUZZER_CC_OFF,
                0,
                0,
                0,
            ),
            Buzzer::Tone { on, off, repeat } => (
                libosdp_sys::osdp_cmd_buzzer_control_code_e_OSDP_CMD_BUZZER_CC_DEFAULT_TONE,
                to_count(on, TENTH_OF_A_SECOND, "buzzer on time")?,
                to_count(off, TENTH_OF_A_SECOND, "buzzer off time")?,
                repeat,
            ),
        };
        Ok(libosdp_sys::osdp_cmd_buzzer {
            reader: value.reader,
            control_code,
            on_count,
            off_count,
            rep_count,
        })
    }
}

/// How a display shows a [`TextCommand`]'s text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextMode {
    /// Until replaced, cut at the end of the row.
    #[default]
    PermanentNoWrap,

    /// Until replaced, wrapping onto the next row.
    PermanentWrap,

    /// For this long, cut at the end of the row; whole seconds, up to
    /// 255 s.
    TemporaryNoWrap(Duration),

    /// For this long, wrapping onto the next row; whole seconds, up to
    /// 255 s.
    TemporaryWrap(Duration),
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

    /// How to show the text, and for how long.
    pub mode: TextMode,

    /// row to display the first character (1 indexed)
    pub offset_row: u8,

    /// column to display the first character (1 indexed)
    pub offset_col: u8,

    /// The string to display (ASCII codes)
    pub data: Vec<u8>,
}

impl TryFrom<libosdp_sys::osdp_cmd_text> for TextCommand {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd_text) -> Result<Self> {
        let shown_for = from_count(value.temp_time, SECOND);
        let mode = match value.control_code {
            libosdp_sys::osdp_cmd_text_control_code_e_OSDP_CMD_TEXT_CC_PERMANENT_NO_WRAP => {
                TextMode::PermanentNoWrap
            }
            libosdp_sys::osdp_cmd_text_control_code_e_OSDP_CMD_TEXT_CC_PERMANENT_WRAP => {
                TextMode::PermanentWrap
            }
            libosdp_sys::osdp_cmd_text_control_code_e_OSDP_CMD_TEXT_CC_TEMPORARY_NO_WRAP => {
                TextMode::TemporaryNoWrap(shown_for)
            }
            libosdp_sys::osdp_cmd_text_control_code_e_OSDP_CMD_TEXT_CC_TEMPORARY_WRAP => {
                TextMode::TemporaryWrap(shown_for)
            }
            _ => return Err(Error::Malformed("text control code")),
        };
        let n = value.length as usize;
        Ok(TextCommand {
            reader: value.reader,
            mode,
            offset_row: value.offset_row,
            offset_col: value.offset_col,
            data: value
                .data
                .get(..n)
                .ok_or(Error::Malformed("text length"))?
                .to_vec(),
        })
    }
}

impl TryFrom<TextCommand> for libosdp_sys::osdp_cmd_text {
    type Error = Error;

    fn try_from(value: TextCommand) -> Result<Self> {
        let (control_code, temp_time) = match value.mode {
            TextMode::PermanentNoWrap => (
                libosdp_sys::osdp_cmd_text_control_code_e_OSDP_CMD_TEXT_CC_PERMANENT_NO_WRAP,
                0,
            ),
            TextMode::PermanentWrap => (
                libosdp_sys::osdp_cmd_text_control_code_e_OSDP_CMD_TEXT_CC_PERMANENT_WRAP,
                0,
            ),
            TextMode::TemporaryNoWrap(shown_for) => (
                libosdp_sys::osdp_cmd_text_control_code_e_OSDP_CMD_TEXT_CC_TEMPORARY_NO_WRAP,
                to_count(shown_for, SECOND, "text display time")?,
            ),
            TextMode::TemporaryWrap(shown_for) => (
                libosdp_sys::osdp_cmd_text_control_code_e_OSDP_CMD_TEXT_CC_TEMPORARY_WRAP,
                to_count(shown_for, SECOND, "text display time")?,
            ),
        };
        let mut data = [0; libosdp_sys::OSDP_CMD_TEXT_MAX_LEN as usize];
        if value.data.len() > data.len() {
            return Err(Error::TooLong { max: data.len() });
        }
        data[..value.data.len()].copy_from_slice(&value.data[..]);
        Ok(libosdp_sys::osdp_cmd_text {
            reader: value.reader,
            control_code,
            temp_time,
            offset_row: value.offset_row,
            offset_col: value.offset_col,
            length: value.data.len() as u8,
            data,
        })
    }
}

/// What to do with a digital output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OutputAction {
    /// Leave it as it is.
    #[default]
    Keep,

    /// Turn it off for good, aborting a timed operation.
    PermanentOff,

    /// Turn it on for good, aborting a timed operation.
    PermanentOn,

    /// Turn it off for good once a timed operation completes.
    PermanentOffAllowTimed,

    /// Turn it on for good once a timed operation completes.
    PermanentOnAllowTimed,

    /// Turn it on for this long, then return to its permanent state. A
    /// whole multiple of 100 ms, up to 6553.5 s.
    TemporaryOn(Duration),

    /// Turn it off for this long, then return to its permanent state. A
    /// whole multiple of 100 ms, up to 6553.5 s.
    TemporaryOff(Duration),
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

    /// What to do with the output.
    pub action: OutputAction,
}

impl TryFrom<libosdp_sys::osdp_cmd_output> for OutputCommand {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd_output) -> Result<Self> {
        let timer = from_count(value.timer_count, TENTH_OF_A_SECOND);
        let action = match value.control_code {
            libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_NOP => {
                OutputAction::Keep
            }
            libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_PERMANENT_OFF => {
                OutputAction::PermanentOff
            }
            libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_PERMANENT_ON => {
                OutputAction::PermanentOn
            }
            libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_PERMANENT_OFF_ALLOW_TIMED => {
                OutputAction::PermanentOffAllowTimed
            }
            libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_PERMANENT_ON_ALLOW_TIMED => {
                OutputAction::PermanentOnAllowTimed
            }
            libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_TEMPORARY_ON => {
                OutputAction::TemporaryOn(timer)
            }
            libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_TEMPORARY_OFF => {
                OutputAction::TemporaryOff(timer)
            }
            _ => return Err(Error::Malformed("output control code")),
        };
        Ok(OutputCommand {
            output_no: value.output_no,
            action,
        })
    }
}

impl TryFrom<OutputCommand> for libosdp_sys::osdp_cmd_output {
    type Error = Error;

    fn try_from(value: OutputCommand) -> Result<Self> {
        let (control_code, timer) = match value.action {
            OutputAction::Keep => (
                libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_NOP,
                Duration::ZERO,
            ),
            OutputAction::PermanentOff => (
                libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_PERMANENT_OFF,
                Duration::ZERO,
            ),
            OutputAction::PermanentOn => (
                libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_PERMANENT_ON,
                Duration::ZERO,
            ),
            OutputAction::PermanentOffAllowTimed => (
                libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_PERMANENT_OFF_ALLOW_TIMED,
                Duration::ZERO,
            ),
            OutputAction::PermanentOnAllowTimed => (
                libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_PERMANENT_ON_ALLOW_TIMED,
                Duration::ZERO,
            ),
            OutputAction::TemporaryOn(timer) => (
                libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_TEMPORARY_ON,
                timer,
            ),
            OutputAction::TemporaryOff(timer) => (
                libosdp_sys::osdp_cmd_output_control_code_e_OSDP_CMD_OUTPUT_CC_TEMPORARY_OFF,
                timer,
            ),
        };
        Ok(libosdp_sys::osdp_cmd_output {
            output_no: value.output_no,
            control_code,
            timer_count: to_count(timer, TENTH_OF_A_SECOND, "output timer")?,
        })
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

impl TryFrom<libosdp_sys::osdp_cmd_keyset> for KeySetCommand {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd_keyset) -> Result<Self> {
        let data = value
            .data
            .get(..value.length as usize)
            .ok_or(Error::Malformed("key length"))?;
        Ok(KeySetCommand {
            key_type: value.type_,
            data: data.to_vec(),
        })
    }
}

impl TryFrom<KeySetCommand> for libosdp_sys::osdp_cmd_keyset {
    type Error = Error;

    fn try_from(value: KeySetCommand) -> Result<Self> {
        let mut data = [0; libosdp_sys::OSDP_CMD_KEYSET_KEY_MAX_LEN as usize];
        if value.data.len() > data.len() {
            return Err(Error::TooLong { max: data.len() });
        }
        data[..value.data.len()].copy_from_slice(&value.data[..]);
        Ok(libosdp_sys::osdp_cmd_keyset {
            type_: value.key_type,
            length: value.data.len() as u8,
            data,
        })
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

impl TryFrom<libosdp_sys::osdp_cmd_mfg> for MfgCommand {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd_mfg) -> Result<Self> {
        let data = value
            .data
            .get(..value.length as usize)
            .ok_or(Error::Malformed("manufacturer command length"))?;
        let bytes = value.vendor_code.to_le_bytes();
        Ok(MfgCommand {
            vendor_code: (bytes[0], bytes[1], bytes[2]),
            data: data.to_vec(),
        })
    }
}

impl TryFrom<MfgCommand> for libosdp_sys::osdp_cmd_mfg {
    type Error = Error;

    fn try_from(value: MfgCommand) -> Result<Self> {
        let mut data = [0; libosdp_sys::OSDP_CMD_MFG_MAX_DATALEN as usize];
        if value.data.len() > data.len() {
            return Err(Error::TooLong { max: data.len() });
        }
        data[..value.data.len()].copy_from_slice(&value.data[..]);
        Ok(libosdp_sys::osdp_cmd_mfg {
            vendor_code: value.vendor_code.as_le(),
            length: value.data.len() as u8,
            data,
        })
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

impl TryFrom<Command> for libosdp_sys::osdp_cmd {
    type Error = Error;

    fn try_from(value: Command) -> Result<Self> {
        Ok(match value {
            Command::Led(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_LED,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 { led: c.try_into()? },
            },
            Command::Buzzer(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_BUZZER,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 {
                    buzzer: c.try_into()?,
                },
            },
            Command::Text(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_TEXT,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 {
                    text: c.try_into()?,
                },
            },
            Command::Output(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_OUTPUT,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 {
                    output: c.try_into()?,
                },
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
                    keyset: c.try_into()?,
                },
            },
            Command::Mfg(c) => libosdp_sys::osdp_cmd {
                _node: unsafe { core::mem::zeroed() },
                id: libosdp_sys::osdp_cmd_e_OSDP_CMD_MFG,
                flags: 0,
                __bindgen_anon_1: libosdp_sys::osdp_cmd__bindgen_ty_1 {
                    mfg: c.try_into()?,
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
        })
    }
}

impl TryFrom<libosdp_sys::osdp_cmd> for Command {
    type Error = Error;

    fn try_from(value: libosdp_sys::osdp_cmd) -> Result<Self> {
        match value.id {
            libosdp_sys::osdp_cmd_e_OSDP_CMD_LED => {
                Ok(Command::Led(unsafe { value.__bindgen_anon_1.led }.try_into()?))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_BUZZER => {
                Ok(Command::Buzzer(unsafe { value.__bindgen_anon_1.buzzer }.try_into()?))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_TEXT => {
                Ok(Command::Text(unsafe { value.__bindgen_anon_1.text }.try_into()?))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_OUTPUT => {
                Ok(Command::Output(unsafe { value.__bindgen_anon_1.output }.try_into()?))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_COMSET => {
                Ok(Command::ComSet(unsafe { value.__bindgen_anon_1.comset.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_COMSET_DONE => {
                Ok(Command::ComSetDone(unsafe { value.__bindgen_anon_1.comset.into() }))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_KEYSET => {
                Ok(Command::KeySet(unsafe { value.__bindgen_anon_1.keyset }.try_into()?))
            }
            libosdp_sys::osdp_cmd_e_OSDP_CMD_MFG => {
                Ok(Command::Mfg(unsafe { value.__bindgen_anon_1.mfg }.try_into()?))
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
    use crate::{
        Buzzer, BuzzerCommand, Command, Error, KeySetCommand, LedColor, LedCommand, LedParams,
        LedPermanent, LedTemporary, MfgCommand, OutputAction, OutputCommand, TextCommand, TextMode,
    };
    use core::time::Duration;
    use libosdp_sys::osdp_cmd_mfg;

    fn round_trip(cmd: Command) -> Command {
        let raw: libosdp_sys::osdp_cmd = cmd.try_into().unwrap();
        raw.try_into().unwrap()
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn led_codes_round_trip() {
        let flash = LedParams {
            on_time: ms(300),
            off_time: ms(200),
            on_color: LedColor::White,
            off_color: LedColor::None,
        };
        let cmd = Command::Led(LedCommand {
            reader: 0,
            led_number: 1,
            temporary: LedTemporary::Set {
                params: flash,
                timer: ms(5_000),
            },
            permanent: LedPermanent::Set(flash),
        });
        assert_eq!(round_trip(cmd.clone()), cmd);
        let raw: libosdp_sys::osdp_cmd = cmd.try_into().unwrap();
        let led = unsafe { raw.__bindgen_anon_1.led };
        assert_eq!(
            (led.temporary.control_code, led.temporary.timer_count),
            (2, 50)
        );
        assert_eq!((led.permanent.control_code, led.permanent.on_count), (1, 3));
    }

    #[test]
    fn led_keep_and_cancel_round_trip() {
        let params = LedParams {
            on_time: ms(100),
            off_time: ms(100),
            on_color: LedColor::Red,
            off_color: LedColor::Green,
        };
        for (temporary, permanent) in [
            (LedTemporary::Keep, LedPermanent::Set(params)),
            (LedTemporary::Cancel, LedPermanent::Set(params)),
            (
                LedTemporary::Set {
                    params,
                    timer: ms(100),
                },
                LedPermanent::Keep,
            ),
        ] {
            let cmd = Command::Led(LedCommand {
                reader: 0,
                led_number: 0,
                temporary,
                permanent,
            });
            assert_eq!(round_trip(cmd.clone()), cmd);
        }
    }

    #[test]
    fn buzzer_codes_round_trip() {
        let tone = Buzzer::Tone {
            on: ms(500),
            off: ms(100),
            repeat: 2,
        };
        for action in [Buzzer::Off, tone] {
            let cmd = Command::Buzzer(BuzzerCommand { reader: 0, action });
            assert_eq!(round_trip(cmd.clone()), cmd);
        }
    }

    #[test]
    fn output_codes_round_trip() {
        for action in [
            OutputAction::Keep,
            OutputAction::PermanentOn,
            OutputAction::PermanentOffAllowTimed,
            OutputAction::TemporaryOn(ms(1_500)),
        ] {
            let cmd = Command::Output(OutputCommand {
                output_no: 2,
                action,
            });
            assert_eq!(round_trip(cmd.clone()), cmd);
        }
    }

    #[test]
    fn text_modes_are_one_indexed_and_timed_in_seconds() {
        let cmd = Command::Text(TextCommand {
            reader: 0,
            mode: TextMode::TemporaryWrap(Duration::from_secs(3)),
            offset_row: 1,
            offset_col: 1,
            data: b"hi".to_vec(),
        });
        assert_eq!(round_trip(cmd.clone()), cmd);
        let raw: libosdp_sys::osdp_cmd = cmd.try_into().unwrap();
        let text = unsafe { raw.__bindgen_anon_1.text };
        assert_eq!((text.control_code, text.temp_time), (4, 3));
    }

    #[test]
    fn timings_off_the_unit_or_out_of_range_are_refused() {
        let off_unit = Command::Buzzer(BuzzerCommand {
            reader: 0,
            action: Buzzer::Tone {
                on: ms(150),
                off: ms(100),
                repeat: 1,
            },
        });
        let too_long = Command::Output(OutputCommand {
            output_no: 0,
            action: OutputAction::TemporaryOff(Duration::from_secs(7_000)),
        });
        let led_timer_too_long = Command::Led(LedCommand {
            reader: 0,
            led_number: 0,
            temporary: LedTemporary::Set {
                params: LedParams::default(),
                timer: ms(6_553_600),
            },
            permanent: LedPermanent::Keep,
        });
        for cmd in [off_unit, too_long, led_timer_too_long] {
            let raw: Result<libosdp_sys::osdp_cmd, Error> = cmd.try_into();
            assert!(matches!(raw, Err(Error::InvalidArgument(_))));
        }
    }

    #[test]
    fn unknown_codes_and_bad_lengths_are_malformed() {
        let led = Command::Led(LedCommand {
            reader: 0,
            led_number: 0,
            temporary: LedTemporary::Keep,
            permanent: LedPermanent::Keep,
        });
        let mut bad_led: libosdp_sys::osdp_cmd = led.try_into().unwrap();
        bad_led.__bindgen_anon_1.led.temporary.control_code = 9;
        let mut bad_output: libosdp_sys::osdp_cmd = Command::Output(OutputCommand {
            output_no: 0,
            action: OutputAction::Keep,
        })
        .try_into()
        .unwrap();
        bad_output.__bindgen_anon_1.output.control_code = 9;
        let mut bad_text: libosdp_sys::osdp_cmd = Command::Text(TextCommand::default())
            .try_into()
            .unwrap();
        bad_text.__bindgen_anon_1.text.length = u8::MAX;
        let mut bad_keyset: libosdp_sys::osdp_cmd =
            Command::KeySet(KeySetCommand::new_scbk([0; 16]))
                .try_into()
                .unwrap();
        bad_keyset.__bindgen_anon_1.keyset.length = u8::MAX;
        let mut bad_mfg: libosdp_sys::osdp_cmd =
            Command::Mfg(MfgCommand::default()).try_into().unwrap();
        bad_mfg.__bindgen_anon_1.mfg.length = u8::MAX;
        for (raw, reason) in [
            (bad_led, "LED temporary control code"),
            (bad_output, "output control code"),
            (bad_text, "text length"),
            (bad_keyset, "key length"),
            (bad_mfg, "manufacturer command length"),
        ] {
            assert_eq!(Command::try_from(raw), Err(Error::Malformed(reason)));
        }
    }

    #[test]
    fn text_longer_than_libosdp_holds_is_refused() {
        let cmd = Command::Text(TextCommand {
            data: vec![b'x'; libosdp_sys::OSDP_CMD_TEXT_MAX_LEN as usize + 1],
            ..TextCommand::default()
        });
        let raw: Result<libosdp_sys::osdp_cmd, Error> = cmd.try_into();
        assert!(matches!(raw, Err(Error::TooLong { .. })));
    }

    #[test]
    fn test_command_mfg() {
        let cmd = MfgCommand {
            vendor_code: (0x05, 0x07, 0x09),
            data: vec![0x55, 0xAA],
        };
        let cmd_struct: osdp_cmd_mfg = cmd.clone().try_into().unwrap();

        assert_eq!(cmd_struct.vendor_code, 0x90705);
        assert_eq!(cmd_struct.length, 2);
        assert_eq!(cmd_struct.data[0], 0x55);
        assert_eq!(cmd_struct.data[1], 0xAA);

        assert_eq!(cmd, cmd_struct.try_into().unwrap());
    }
}
