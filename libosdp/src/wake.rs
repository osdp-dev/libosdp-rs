//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Wakers of tasks awaiting a context. LibOSDP's callbacks only collect
//! them; the context wakes them once the C call that ran those callbacks
//! has returned, so no application code runs inside a callback.

use alloc::vec::Vec;
use core::{
    cell::{Cell, RefCell},
    task::Waker,
};

#[derive(Default)]
pub(crate) struct Wakeups(RefCell<Vec<Waker>>);

impl Wakeups {
    pub(crate) fn defer(&self, waker: Waker) {
        self.0.borrow_mut().push(waker);
    }

    pub(crate) fn wake_all(&self) {
        let wakers = core::mem::take(&mut *self.0.borrow_mut());
        for waker in wakers {
            waker.wake();
        }
    }
}

#[derive(Default)]
pub(crate) struct WakerSlot(Cell<Option<Waker>>);

impl WakerSlot {
    /// Park `waker`, keeping the one already parked if it wakes the same
    /// task.
    pub(crate) fn register(&self, waker: &Waker) {
        let parked = match self.0.take() {
            Some(current) if current.will_wake(waker) => current,
            _ => waker.clone(),
        };
        self.0.set(Some(parked));
    }

    pub(crate) fn take(&self) -> Option<Waker> {
        self.0.take()
    }
}
