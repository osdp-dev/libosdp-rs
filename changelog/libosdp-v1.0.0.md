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
  send_command(), does nothing, and send_command() and
  register_file_ops() from there return Error::Rejected

## Enhancements

- ## TODO

## Fixes

- ## TODO

## Changes

- 3172814 libosdp: Make serde an optional feature
- 2ad854a libosdp: Require Send for callbacks a context owns
- 8001dd9 libosdp: Copy only the bytes a channel or file read returned
- 9e4445f libosdp: Free boxes handed to C when a context drops
- 3f7cc84 libosdp: Move FFI glue into an ffi module
