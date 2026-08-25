---
release_crate: libosdp-sys
release_date: 2026-08-25
release_version: v3.2.4
---

Vendors libosdp v3.2.4, a security and robustness release touching the CP
response decoder, the file transfer path and the packet framer. The public C
API is unchanged, so the generated bindings are identical to v3.2.3, but the
behaviour of the vendored library changes and every user should upgrade.

Note that this crate compiles the vendored sources with assert() active in
both debug and release profiles, because the cc crate does not define NDEBUG.
That distinction matters below: where upstream describes an out-of-bounds
read reachable in an assert-disabled build, Rust users instead saw the
process abort.

## Fixes

- cp: Bound REPLY_KEYPAD and REPLY_RAW payload lengths against the event
  buffer capacity in cp_decode_response. Both lengths are taken from the
  wire and were only checked for consistency with the packet length, so a
  malicious or malformed PD reply could memcpy past the event structure.
  This is an out-of-bounds write reachable by any CP talking to an untrusted
  PD, and is the reason to take this release
- file: Replace the assert() guarding the declared chunk length in
  osdp_file_cmd_tx_decode with a real bounds check that returns an error.
  The check was load-bearing input validation written as an assertion, so a
  malformed file transfer command aborted the process here rather than
  being rejected
- file: Reject a chunk whose offset plus length runs past the size the peer
  announced, so a transfer cannot be made to write outside the file it
  declared
- file: Stop asserting that the running offset never exceeds the file size
  in osdp_file_cmd_stat_build. A legitimately retransmitted chunk after a
  lost FTSTAT re-drives the offset and can transiently overshoot, aborting
  a healthy transfer; the write-side bound above is the real guard
- phy: Guard the packet length subtraction against underflow
- phy: Fix a framer wedge on a fragmented packet header
