---
release_crate: libosdp-sys
release_date: 2026-08-25
release_version: v3.2.6
---

Vendors libosdp v3.2.6, skipping v3.2.5. That release re-cut v3.2.4 to fix
libosdp's own PyPI publishing and changed no compiled source, so it carried
nothing for this crate. The public C API is unchanged and the generated
bindings are identical to v3.2.4.

The PD-side acknowledgement fix below is the reason to take this release.

## Fixes

- pd: Stop acknowledging output, LED and buzzer commands that were not
  carried out. CMD_OUT, CMD_LED and CMD_BUZ decode a run of items in a
  loop that breaks early when the capability check or the command callback
  rejects one, but the code then set REPLY_ACK unconditionally. A PD would
  tell the CP the command succeeded while having applied none or only part
  of it. It now acknowledges only when every item in the command was
  accepted
- phy, cp, common: Assemble multi-byte wire values through uint32_t rather
  than relying on integer promotion. The byte-pair readers, the packet
  length and CRC assembly in the framer, and the CP's peer receive-buffer
  size all shifted a value promoted to int. On every target this crate
  builds for, int is 32 bits and the result cannot overflow, so this is a
  portability fix for narrower-int targets and clears the corresponding
  undefined-behaviour sanitizer findings rather than a behaviour change
- diag: Print a size_t packet count with %zu instead of %d. This file is
  compiled only under the packet_trace and data_trace features, so the
  mismatch was reachable only with one of those enabled
