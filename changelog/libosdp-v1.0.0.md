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
  submit(), does nothing, and submit() from there returns
  Error::Rejected
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
  PdInfoBuilder takes it. The string parsers for capabilities, which
  only osdpctl used, are gone
- cp, pd: Answer queries with Option: pd_id and capability return None
  while the PD is offline, capability also for a function the PD did not
  advertise, and file_progress, which replaces file_transfer_status,
  returns a FileProgress { size, offset } of u32s, or None outside a
  transfer. is_sc_active is now is_secure
- file: Reshape FileOps around LibOSDP 4.0's file ops: open(id,
  OpenMode) returns the file size as a u32, read and write take &mut
  self and a u32 offset (offset_read and offset_write are gone). A CP's
  file ops are always opened to OpenMode::Read and a PD's to
  OpenMode::Write { size }, so a PD receiving an empty file is no longer
  told to read it
- api: Give a PD its FileOps with PdInfoBuilder::file_ops, which
  ControlPanelBuilder::build and PeripheralDevice::new hand to LibOSDP,
  in place of register_file_ops on ControlPanel and PeripheralDevice.
  PeripheralDevice::set_capabilities is gone too: PdInfoBuilder sets
  the capabilities. LibOSDP fixes both when a context is first
  refreshed
- file: Cancel a file transfer with Submission::cancel on the submission
  that started it. FileTxCommand::flags and OSDP_CMD_FILE_TX_FLAG_CANCEL
  are gone: LibOSDP 4.0 no longer honours the flag
- api: Mark Command, Event, MpKind, TrsCommand, TrsReply and
  TrsSessionStatus non-exhaustive, as Notification is: LibOSDP adds
  commands, replies and operations as it gains features, so a match on
  them outside this crate needs a wildcard arm

## Enhancements

- ## TODO
- commands: Add LedColor::White, new in LibOSDP 4.0
- libosdp: Declare Rust 1.85 as the minimum supported version
- flags: Add LibOSDP 4.0's setup flags: EnableNotification,
  CapturePackets, AllowEmptyEncryptedDataBlock and BioReadMultipart
- notifications: Report what LibOSDP observes about a link -- secure
  channel and online changes, multi-part progress, the PD's identity,
  TRS session state -- as Notifications, for PDs with
  Flag::EnableNotification set. A CP hands them over through next_event
  as CpEvent::Notification, a PD through next_notification; until now a
  PD refused them as unknown commands
- file: Let a PD decide how a received transfer ends with
  FileOps::finalize, which reports Finalize::Done with an MpOutcome, or
  Finalize::Pending while it verifies or installs the file. The default
  ends the transfer at once with MpOutcome::Ok, as before
- events: Add Event::MfgStatus and Event::MfgError, LibOSDP 4.0's
  manufacturer specific status and error replies. A PD may answer a
  Command::Mfg with either; an error fails the command's Submission
- bio: Add biometric read and match: Command::BioRead and
  Command::BioMatch, answered with Event::BioRead and Event::BioMatch,
  with BioType, BioFormat and BioStatus. A PD refuses a body part or
  format it cannot scan with Nak::BioType or Nak::BioFormat.
  Submission::cancel stops a multi-part read
- commands: Add Command::TimeDate, OSDP's obsolete date and time set
  (osdp_TDSET), for older PDs that keep time. A date or time out of
  range is refused with Error::InvalidArgument
- cp: Take a PD off the bus and back with ControlPanel::disable and
  enable, and check which it is with is_enabled
- metrics: Read a link's health counters -- packets, check errors, NAKs,
  secure channel handshakes and failures, commands, events -- with
  take_metrics on a ControlPanel or PeripheralDevice, which resets them
- piv: Read PIV data objects from, and send general and
  challenge/response authenticate challenges to, the smart card in a
  PD's reader with ControlPanel::piv_data, general_auth and
  challenge_auth. Each returns a Submission<'_, PivReply> whose status()
  resolves into the card's reply; Submission::cancel stops it. A PD
  answers Command::PivData, GenAuth and CrAuth with the matching Event
- trs: Talk to the smart card in a PD's reader through transparent
  reader card sessions: Command::Trs carries a TrsCommand (Start, Stop,
  Apdu, EnterPin, CardScan) and Event::Trs a TrsReply (CardInfo,
  CardPresent, CardData, PinComplete, Error). LibOSDP builds this only
  with the new trs feature, which turns on libosdp-sys's; without it,
  these commands are refused with Error::Rejected
- trs: Scan a PD's reader for smart cards in the background with
  ControlPanel::trs_scan_enable, at a TrsScan cadence, and
  trs_scan_disable; max_apdu_len says how large a C-APDU the link
  carries
- pd: Responder::reply refuses a status with the wrong number of entries
  for the PD's capabilities with Error::Rejected, rather than LibOSDP
  silently answering the CP with a NAK

## Fixes

- ## TODO
- file: Refuse a FileOps::write that reports more bytes written than it
  was given, and hand it LibOSDP's buffer without copying it
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
- pdid: Read a PdId's serial number in the byte order it is written in,
  so the serial number a CP reads is the one the PD was given rather
  than its bytes reversed

## Changes

- 3172814 libosdp: Make serde an optional feature
- 2ad854a libosdp: Require Send for callbacks a context owns
- 8001dd9 libosdp: Copy only the bytes a channel or file read returned
- 9e4445f libosdp: Free boxes handed to C when a context drops
- 3f7cc84 libosdp: Move FFI glue into an ffi module
