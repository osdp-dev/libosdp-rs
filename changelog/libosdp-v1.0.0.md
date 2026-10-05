---
release_crate: libosdp
release_date: 2026-10-05
release_version: v1.0.0
---

Release subject ## TODO

## Breaking changes

- error: Replace OsdpError with Error, whose variants say what went
  wrong (Setup, InvalidArgument, Rejected, NotInProgress, TooLong,
  Malformed, Channel) rather than which call failed. FileOps
  implementations return the new FileError
- cp: Address PDs by the PdIndex that ControlPanelBuilder::add_pd
  returns rather than by an integer offset. add_pd now takes &mut self
  and refuses a 127th PD when it is added, not at build()
- api: Drop the Osdp prefix from public types. OsdpCommand, OsdpEvent,
  OsdpFlag and OsdpFileOps are now Command, Event, Flag and FileOps;
  payload structs are named for what they carry (LedCommand,
  BuzzerCommand, CardRead, KeyPress, StatusReport, ...)
- api: Take &self on every ControlPanel and PeripheralDevice method. A
  context stays Send but not Sync. refresh() called from inside one of
  the context's own callbacks, including FileOps::open run by
  submit(), does nothing, and submit() and register_file_ops() from
  there return Error::Rejected
- api: Replace send_command and notify_event with submit, which
  returns a Submission whose status() reports how the command or
  event ended: Ok, or a CompletionError (Failed, Flushed, Aborted).
  flush_events becomes flush, on the CP too, and returns how many
  submissions it dropped
- cp: Queue PD events during refresh() and hand them over through
  next_event(), replacing set_event_callback. The queue holds 32
  events unless ControlPanelBuilder::event_queue_capacity says
  otherwise; when it is full, newer events are dropped and counted by
  dropped_events()
- pd: Answer CP commands through a CommandHandler, replacing
  set_command_callback. A handler accepts a command with Ok or
  refuses it with a Nak (BioType, BioFormat, Record), and answers a
  command that expects data -- a manufacturer command, a status
  query -- with Responder::reply. Until a handler is set, every
  command, KEYSET and COMSET included, is refused with Nak::Record
- channel: Make Channel::write all-or-nothing, as LibOSDP 4.0
  requires: it returns Ok(()) once the whole buffer is queued, or
  WouldBlock to have the packet retried on the next refresh. get_id
  is gone, and flush and close return nothing and default to doing
  nothing; flush discards buffered bytes
- commands: Type the LED, buzzer, output and text control codes.
  LedCommand takes LedTemporary::{Keep, Cancel, Set} and
  LedPermanent::{Keep, Set}; BuzzerCommand a Buzzer::{Off, Tone};
  OutputCommand an OutputAction; TextCommand a TextMode. Their
  timings are Durations, and submit refuses one that is not a whole
  number of LibOSDP's units or does not fit

## Enhancements

- ## TODO
- commands: Add LedColor::White, new in LibOSDP 4.0
- libosdp: Declare Rust 1.85 as the minimum supported version

## Fixes

- ## TODO
- file: Refuse a FileOps::offset_write that reports more bytes written
  than it was given, and hand it LibOSDP's buffer without copying it
- commands: Refuse text longer than LibOSDP's buffer with
  Error::TooLong instead of panicking
- api: Refuse a command (MfgCommand, KeySetCommand) or event
  (CardRead, KeyPress, MfgReply) whose data does not fit LibOSDP's
  buffer with Error::TooLong, from submit and Responder::reply,
  instead of panicking. A CardRead with more bits than data is
  refused with Error::InvalidArgument

## Changes

- 3172814 libosdp: Make serde an optional feature
- 2ad854a libosdp: Require Send for callbacks a context owns
- 8001dd9 libosdp: Copy only the bytes a channel or file read returned
- 9e4445f libosdp: Free boxes handed to C when a context drops
- 3f7cc84 libosdp: Move FFI glue into an ffi module
