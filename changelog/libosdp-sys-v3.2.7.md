---
release_crate: libosdp-sys
release_date: 2026-08-25
release_version: v3.2.7
---

Vendors libosdp v3.2.7, a packet-layer robustness release. The public C API
is unchanged and the generated bindings are identical to v3.2.6.

The framer bound fix below is the reason to take this release.

## Fixes

- phy: Charge the mark byte against the packet buffer bound in
  phy_validate_header. The mark byte is not counted in the packet's length
  field but is buffered alongside the packet, so validating the declared
  length alone let a peer announce a full-capacity packet behind a mark
  byte and overrun the receive buffer by one byte
- phy: Drain the software receive ring when the error path resets phy
  state. Bytes left in the ring from the frame that caused the error were
  otherwise re-parsed after the reset, so a single malformed frame could
  keep the link desynchronised. Does not apply when built with
  OPT_OSDP_RX_ZERO_COPY, which uses no software ring
- cp: Re-send a retried command with the sequence number it was first sent
  under. A retry probe advanced the sequence number, so the PD saw a gap
  and rejected the retry that was meant to recover the exchange. The CP now
  records the sequence number it transmitted, flushes the channel and
  rewinds before re-sending
