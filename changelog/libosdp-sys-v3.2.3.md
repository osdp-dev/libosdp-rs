---
release_crate: libosdp-sys
release_date: 2026-08-25
release_version: v3.2.3
---

Vendors libosdp v3.2.3. No OSDP protocol source changed in this range, and
the public C API is untouched, so the generated bindings are byte-identical
to v3.2.2. The only change reaching this crate is the utils submodule bump,
which improves portability of the vendored build.

## Fixes

- utils: Gate the IS_ENABLED() _Static_assert self-test behind C11, so the
  vendored sources build under compilers that do not select a C11 mode
- utils: Restore __weak after <zephyr/toolchain.h> undefines it, fixing
  header parsing on Zephyr targets
