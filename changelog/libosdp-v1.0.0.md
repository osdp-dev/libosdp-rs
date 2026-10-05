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
