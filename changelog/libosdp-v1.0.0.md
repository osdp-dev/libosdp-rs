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
  BuzzerCommand, CardRead, KeyPress, ...)
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
- libosdp: Make ConvertEndian private; it only packed vendor codes
  and IDs for LibOSDP
- cp: Replace set_flag, which panicked when LibOSDP refuses a change,
  with set_flags and clear_flags, which return Error::Rejected and
  change no flag
- status: Replace StatusReport and its fixed 64-byte report with
  Status::{Input, Output, Local { tamper, power }, Reader}, one entry
  per entity, as LibOSDP 4.0 reports them. Command::Status takes the
  StatusKind to query, and the serde feature no longer pulls in
  serde_arrays
- pdcap: Replace PdCapability and PdCapEntity with Capability {
  function, compliance, items }, whose function is a CapFunction.
  PdInfoBuilder and PeripheralDevice::set_capabilities take it. The
  string parsers for capabilities, which only osdpctl used, are gone
- cp, pd: Answer queries with Option: pd_id and capability return None
  while the PD is offline, capability also for a function the PD did not
  advertise, and file_progress, which replaces file_transfer_status,
  returns a FileProgress { size, offset } of u32s, or None outside a
  transfer. is_sc_active is now is_secure

## Enhancements

- ## TODO
- commands: Add LedColor::White, new in LibOSDP 4.0
- libosdp: Declare Rust 1.85 as the minimum supported version
- flags: Add LibOSDP 4.0's setup flags: EnableNotification,
  CapturePackets, AllowEmptyEncryptedDataBlock and BioReadMultipart

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
- api: Refuse a command or event from LibOSDP whose length runs past its
  buffer (KeySetCommand, MfgCommand, CardRead, KeyPress, MfgReply) with
  Error::Malformed instead of panicking inside LibOSDP's callback, which
  aborted the process
- pd: End the capability list that set_capabilities hands LibOSDP, which
  read past its end looking for the terminator

## Changes

- 3172814 libosdp: Make serde an optional feature
- 2ad854a libosdp: Require Send for callbacks a context owns
- 8001dd9 libosdp: Copy only the bytes a channel or file read returned
- 9e4445f libosdp: Free boxes handed to C when a context drops
- 3f7cc84 libosdp: Move FFI glue into an ffi module
