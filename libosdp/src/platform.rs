//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

//! The clock and entropy source a bare-metal target owes LibOSDP. The C
//! library calls `osdp_millis_now` and `rand_u32` but defines neither on a
//! target it cannot identify; this crate defines both and forwards them to
//! the [`Platform`] the application registers.

use core::{
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

#[cfg(target_os = "none")]
use crate::Error;

/// What LibOSDP needs from a bare-metal target (`target_os = "none"`),
/// registered once with [`set_platform`] before the first
/// [`ControlPanel`](crate::ControlPanel) or
/// [`PeripheralDevice`](crate::PeripheralDevice) is set up; until then,
/// setup fails with [`Error::NoPlatform`](crate::Error::NoPlatform). Hosted targets have both from
/// the operating system and do not have this type.
///
/// ```ignore
/// static PLATFORM: libosdp::Platform = libosdp::Platform {
///     millis: board::millis,
///     random: board::trng_u32,
/// };
/// libosdp::set_platform(&PLATFORM);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Platform {
    /// Milliseconds since an arbitrary start, from a clock that never goes
    /// backwards. LibOSDP times every timeout and retry with it.
    pub millis: fn() -> u64,
    /// A random 32-bit value. Secure channel challenges are drawn from it,
    /// so back it with a hardware TRNG: a predictable source makes the
    /// session keys predictable.
    pub random: fn() -> u32,
}

// A thin pointer, so that one atomic store registers it; that holds even on
// targets such as thumbv6m, whose atomics only load and store.
static PLATFORM: AtomicPtr<Platform> = AtomicPtr::new(ptr::null_mut());

/// Register the [`Platform`] that LibOSDP reads its clock and entropy from.
/// A later call replaces it.
pub fn set_platform(platform: &'static Platform) {
    PLATFORM.store(ptr::from_ref(platform).cast_mut(), Ordering::Release);
}

#[cfg(target_os = "none")]
fn platform() -> Option<&'static Platform> {
    // SAFETY: PLATFORM is null or was stored from a &'static Platform.
    unsafe { PLATFORM.load(Ordering::Acquire).as_ref() }
}

/// Contexts refuse to set up without a [`Platform`], so LibOSDP never calls
/// the hooks below before one is registered.
#[cfg(target_os = "none")]
pub(crate) fn require_platform() -> Result<(), Error> {
    platform().map(|_| ()).ok_or(Error::NoPlatform)
}

#[cfg(target_os = "none")]
#[no_mangle]
extern "C" fn osdp_millis_now() -> u64 {
    let platform = platform().expect("LibOSDP read the clock with no Platform set");
    (platform.millis)()
}

#[cfg(target_os = "none")]
#[no_mangle]
extern "C" fn rand_u32() -> u32 {
    let platform = platform().expect("LibOSDP drew entropy with no Platform set");
    (platform.random)()
}
