#!/usr/bin/env bash
#
# Copyright (c) 2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
#
# SPDX-License-Identifier: Apache-2.0
#
# Fail if the bare-metal libosdp.a asks the link for anything beyond what a
# no_std application is documented to supply. A newly needed libc function
# would otherwise only surface as a linker error in someone's firmware.

set -euo pipefail

TARGET=${1:-thumbv7em-none-eabihf}

# Rust's compiler_builtins provides the mem* functions and the __aeabi_*
# helpers; libosdp provides osdp_calloc, osdp_free, osdp_millis_now and
# rand_u32; snprintf and vsnprintf are the application's (or its libc's).
ALLOWED='^(mem(cmp|cpy|move|set)|__aeabi_[a-z0-9]+|osdp_calloc|osdp_free|osdp_millis_now|rand_u32|v?snprintf)$'

cd "$(dirname "$0")/.."
cargo build --quiet --package libosdp-sys --target "$TARGET"
archive=$(ls -t target/"$TARGET"/debug/build/libosdp-sys-*/out/libosdp.a | head -n 1)

defined=$(arm-none-eabi-nm --defined-only "$archive" 2>/dev/null |
	awk 'NF == 3 { print $3 }' | sort -u)
undefined=$(arm-none-eabi-nm --undefined-only "$archive" 2>/dev/null |
	awk 'NF == 2 { print $2 }' | sort -u)
external=$(comm -23 <(echo "$undefined") <(echo "$defined"))
unexpected=$(echo "$external" | grep -Ev "$ALLOWED" || true)

if [ -n "$unexpected" ]; then
	echo "libosdp.a for $TARGET needs symbols no_std users are not told to supply:"
	echo "$unexpected" | sed 's/^/  /'
	exit 1
fi
echo "libosdp.a for $TARGET needs only:" $external
