---
release_crate: libosdp
release_date: 2026-08-25
release_version: v0.2.2
---

Moves the pinned libosdp-sys from 3.2.2 to 3.2.7. The Rust API is unchanged;
this release exists to carry five libosdp releases' worth of packet-layer and
memory-safety fixes to users of this crate.

libosdp depends on libosdp-sys with an exact pin, because libosdp-sys ships
breaking C API changes in minor bumps. That means anyone on 0.2.1 was held at
libosdp-sys 3.2.2 and received none of the fixes below, whatever their
Cargo.lock said. Upgrading is strongly recommended.

## Fixes

- cp: Bound REPLY_KEYPAD and REPLY_RAW payload lengths against the event
  buffer. A malformed or malicious PD reply could previously write past the
  event structure, an out-of-bounds write reachable by any CP talking to an
  untrusted PD (libosdp-sys 3.2.4)
- file: Validate declared chunk lengths and offsets instead of asserting
  them. This crate compiles the vendored sources with assert() enabled, so a
  malformed file transfer command aborted the process rather than being
  rejected (libosdp-sys 3.2.4)
- pd: Stop acknowledging output, LED and buzzer commands that were rejected
  part way through, which told the CP a command had succeeded when it had
  not been applied (libosdp-sys 3.2.6)
- phy: Charge the mark byte against the packet buffer bound, closing a
  one-byte receive buffer overrun reachable from the wire (libosdp-sys
  3.2.7)
- phy: Drain the software receive ring on an error-path state reset, so a
  single malformed frame no longer keeps the link desynchronised
  (libosdp-sys 3.2.7)
- cp: Re-send a retried command under the sequence number it was first sent
  with, so the PD no longer rejects the retry meant to recover the exchange
  (libosdp-sys 3.2.7)

## Enhancements

- docs: Link the compatibility and secure-channel guides from the crate
  documentation, and fix stale links left by the move to the osdp-dev org
