//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::{queue::DrainQueue, Command, CpEvent, PdIndex};
use core::{cell::Cell, ffi::c_void};

pub(crate) type CpEventQueue = DrainQueue<(PdIndex, CpEvent)>;
pub(crate) type PdCommandCallback = alloc::boxed::Box<dyn FnMut(&mut Command) -> i32 + Send>;

/// The application's current callback. LibOSDP is given a pointer to the
/// slot once, at setup; replacing the callback only swaps the slot's content.
pub(crate) struct CallbackSlot<F>(Cell<Option<F>>);

impl<F> CallbackSlot<F> {
    pub(crate) fn new() -> Self {
        Self(Cell::new(None))
    }

    pub(crate) fn replace(&self, callback: F) {
        drop(self.0.replace(Some(callback)));
    }

    /// Run the callback, if one is set. It is taken out of the slot while it
    /// runs, so a callback that replaces itself is not freed under its own
    /// feet; it is dropped once it returns.
    pub(crate) fn call<R>(&self, call: impl FnOnce(&mut F) -> R) -> Option<R> {
        let mut callback = self.0.take()?;
        let ret = call(&mut callback);
        match self.0.take() {
            Some(replacement) => self.0.set(Some(replacement)),
            None => self.0.set(Some(callback)),
        }
        Some(ret)
    }
}

pub(crate) unsafe extern "C" fn cp_event_trampoline(
    data: *mut c_void,
    pd: i32,
    event: *mut libosdp_sys::osdp_event,
) -> i32 {
    let queue = unsafe { &*(data as *const CpEventQueue) };
    match unsafe { core::ptr::read_unaligned(event).try_into() } {
        Ok(event) => {
            queue.push((PdIndex::new(pd as u8), CpEvent::Event(event)));
            0
        }
        Err(_) => -1, // Failed to parse the event data -> Send NAK
    }
}

pub(crate) unsafe extern "C" fn pd_command_trampoline(
    data: *mut c_void,
    cmd_ptr: *mut libosdp_sys::osdp_cmd,
) -> i32 {
    let slot = unsafe { &*(data as *const CallbackSlot<PdCommandCallback>) };
    match unsafe { core::ptr::read_unaligned(cmd_ptr).try_into() } {
        Ok(mut cmd) => {
            let res = slot.call(|callback| callback(&mut cmd)).unwrap_or(-1);
            unsafe { core::ptr::write_unaligned(cmd_ptr, cmd.into()) };
            res
        }
        Err(_) => -1, // Failed to parse the command data -> Send NAK
    }
}

/// An allocation shared with LibOSDP; freed when this is dropped, which must
/// happen only after LibOSDP stops using it.
pub(crate) struct Shared<T>(*mut T);

impl<T> Shared<T> {
    pub(crate) fn new(value: T) -> Self {
        Self(alloc::boxed::Box::into_raw(alloc::boxed::Box::new(value)))
    }

    pub(crate) fn data(&self) -> *mut c_void {
        self.0 as *mut c_void
    }

    pub(crate) fn get(&self) -> &T {
        unsafe { &*self.0 }
    }
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        drop(unsafe { alloc::boxed::Box::from_raw(self.0) });
    }
}

#[cfg(test)]
mod tests {
    use super::CallbackSlot;
    use alloc::{boxed::Box, rc::Rc};

    type Callback = Box<dyn FnMut() -> u32>;

    #[test]
    fn callback_replaced_from_inside_runs_to_completion_first() {
        let slot: Rc<CallbackSlot<Callback>> = Rc::new(CallbackSlot::new());
        let inner = slot.clone();
        slot.replace(Box::new(move || {
            inner.replace(Box::new(|| 2));
            1
        }));
        assert_eq!(slot.call(|f| f()), Some(1));
        assert_eq!(slot.call(|f| f()), Some(2));
    }
}
