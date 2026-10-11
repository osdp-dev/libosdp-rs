//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::{
    queue::DrainQueue, submit::Slot, CommandHandler, CpEvent, Event, Nak, Notification, PdIndex,
    Responder,
};
use alloc::{boxed::Box, collections::BTreeMap, rc::Weak};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    num::NonZeroUsize,
};

/// What the CP event callback is registered with: the queue the
/// application drains, and, per PD, the submission waiting for the reply
/// to a PIV or authenticate command.
pub(crate) struct CpEvents {
    pub(crate) queue: DrainQueue<(PdIndex, CpEvent)>,
    replies: RefCell<BTreeMap<PdIndex, Weak<Slot>>>,
    #[cfg(feature = "async")]
    pub(crate) stream_waker: crate::wake::WakerSlot,
}

impl CpEvents {
    pub(crate) fn new(capacity: NonZeroUsize) -> Self {
        Self {
            queue: DrainQueue::new(capacity),
            replies: RefCell::new(BTreeMap::new()),
            #[cfg(feature = "async")]
            stream_waker: Default::default(),
        }
    }

    /// Send the reply to the PIV or authenticate command just submitted to
    /// `pd` to `slot`, or, when `None`, to the queue.
    pub(crate) fn route_reply(&self, pd: PdIndex, slot: Option<Weak<Slot>>) {
        let mut replies = self.replies.borrow_mut();
        match slot {
            Some(slot) => replies.insert(pd, slot),
            None => replies.remove(&pd),
        };
    }

    /// Hand a reply to the submission waiting for it on `pd`, if its handle
    /// is still there; any other event, or a reply nobody waits for, is
    /// given back for the queue.
    fn deliver_reply(&self, pd: PdIndex, event: Event) -> Option<Event> {
        let (Event::PivData(reply) | Event::GenAuth(reply) | Event::CrAuth(reply)) = &event else {
            return Some(event);
        };
        let waiting = self.replies.borrow_mut().remove(&pd);
        match waiting.and_then(|slot| slot.upgrade()) {
            Some(slot) if slot.is_pending() && slot.has_handle() => {
                slot.set_reply(reply.clone());
                None
            }
            _ => Some(event),
        }
    }
}

/// What the PD command callback is registered with: the context, which an
/// inline reply is submitted to, the application's handler, and the queue
/// that LibOSDP's notifications, which arrive as commands, are diverted to.
pub(crate) struct PdCommandSlot {
    pub(crate) ctx: *mut libosdp_sys::osdp_t,
    pub(crate) handler: CallbackSlot<Box<dyn CommandHandler>>,
    pub(crate) notifications: DrainQueue<Notification>,
    #[cfg(feature = "async")]
    pub(crate) stream_waker: crate::wake::WakerSlot,
}

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
    let events = unsafe { &*(data as *const CpEvents) };
    let pd = PdIndex::new(pd as u8);
    let event = unsafe { core::ptr::read_unaligned(event) };
    let item = if event.type_ == libosdp_sys::osdp_event_type_OSDP_EVENT_NOTIFICATION {
        Notification::try_from(unsafe { event.__bindgen_anon_1.notif }).map(CpEvent::Notification)
    } else {
        Event::try_from(event).map(CpEvent::Event)
    };
    match item {
        Ok(CpEvent::Event(event)) => {
            if let Some(event) = events.deliver_reply(pd, event) {
                events.queue.push((pd, CpEvent::Event(event)));
            }
            0
        }
        Ok(item) => {
            events.queue.push((pd, item));
            0
        }
        // LibOSDP ignores the result; an event that does not convert is dropped
        Err(_) => -1,
    }
}

pub(crate) unsafe extern "C" fn pd_command_trampoline(
    data: *mut c_void,
    cmd_ptr: *mut libosdp_sys::osdp_cmd,
) -> i32 {
    let slot = unsafe { &*(data as *const PdCommandSlot) };
    let cmd = unsafe { core::ptr::read_unaligned(cmd_ptr) };
    if cmd.id == libosdp_sys::osdp_cmd_e_OSDP_CMD_NOTIFICATION {
        if let Ok(notification) = Notification::try_from(unsafe { cmd.__bindgen_anon_1.notif }) {
            slot.notifications.push(notification);
        }
        return 0;
    }
    let Ok(cmd) = cmd.try_into() else {
        return Nak::Record.as_c();
    };
    let mut responder = Responder::new(slot.ctx, cmd_ptr);
    match slot
        .handler
        .call(|handler| handler.handle(cmd, &mut responder))
    {
        Some(Ok(())) => 0,
        Some(Err(nak)) => nak.as_c(),
        None => Nak::Record.as_c(),
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
