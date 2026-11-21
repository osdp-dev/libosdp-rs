//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

//! LibOSDP's heap on bare metal. The C library leaves `osdp_calloc` and
//! `osdp_free` to targets it cannot identify; serving them from the global
//! allocator keeps C and Rust on one heap, with no libc allocator behind it.

use core::{alloc::Layout, ffi::c_void, ptr};

/// Alignment of every block, which also covers C's `max_align_t` on every
/// bare-metal target Rust has. A block starts with this many bytes holding
/// its total size, since `osdp_free` gets no size but `dealloc` needs the
/// `Layout`.
const HEADER: usize = 16;

#[no_mangle]
extern "C" fn osdp_calloc(count: usize, size: usize) -> *mut c_void {
    let Some(total) = count
        .checked_mul(size)
        .and_then(|bytes| bytes.checked_add(HEADER))
    else {
        return ptr::null_mut();
    };
    let Ok(layout) = Layout::from_size_align(total, HEADER) else {
        return ptr::null_mut();
    };
    // SAFETY: layout has a non-zero size, at least HEADER.
    let base = unsafe { alloc::alloc::alloc_zeroed(layout) };
    if base.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: base is HEADER-aligned and has room for the header.
    unsafe {
        base.cast::<usize>().write(total);
        base.add(HEADER).cast()
    }
}

/// # Safety
///
/// `block` is NULL or came from [`osdp_calloc`] and was not freed since.
#[no_mangle]
unsafe extern "C" fn osdp_free(block: *mut c_void) {
    if block.is_null() {
        return;
    }
    // SAFETY: osdp_calloc handed out base + HEADER and wrote the total size
    // it allocated with at base.
    unsafe {
        let base = block.cast::<u8>().sub(HEADER);
        let total = base.cast::<usize>().read();
        alloc::alloc::dealloc(base, Layout::from_size_align_unchecked(total, HEADER));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_zeroed_and_aligned() {
        let block = osdp_calloc(3, 40).cast::<u8>();
        assert!(!block.is_null());
        assert_eq!(block as usize % HEADER, 0);
        let bytes = unsafe { core::slice::from_raw_parts_mut(block, 120) };
        assert!(bytes.iter().all(|&b| b == 0));
        bytes.fill(0xa5);
        unsafe { osdp_free(block.cast()) };
    }

    #[test]
    fn empty_and_null_are_fine() {
        let block = osdp_calloc(0, 8);
        assert!(!block.is_null());
        unsafe {
            osdp_free(block);
            osdp_free(ptr::null_mut());
        }
    }

    #[test]
    fn overflowing_requests_get_null() {
        assert!(osdp_calloc(usize::MAX, 2).is_null());
        assert!(osdp_calloc(1, usize::MAX - HEADER + 1).is_null());
    }
}
