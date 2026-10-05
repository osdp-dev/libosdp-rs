//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! OSDP specification defines end-point devices as PDs. These devices are
//! responsible for controlling various hardware peripherals (such as LEDs,
//! buzzers, Displays, GPIOs, etc.,) and exposing them in a portable manner to
//! the controlling counter-part, the CP.
//!
//! PD receives commands from the CP and also generates events for activity that
//! happens on the PD itself (such as card read, key press, etc.,) snd sends it
//! to the CP.

use crate::{
    Channel, Error, OsdpCommand, OsdpEvent, OsdpFileOps, PdCapability, PdInfoBuilder,
};
use crate::ffi::{
    callback::{pd_command_trampoline, OwnedCallback, PdCommandCallback},
    channel::OwnedChannel,
    file::OwnedFileOps,
    log::pd_log_handler,
    submit::pd_event_complete,
};
use alloc::{boxed::Box, vec::Vec};

type Result<T> = core::result::Result<T, Error>;

/// OSDP Peripheral Device (PD) context
pub struct PeripheralDevice {
    ctx: *mut libosdp_sys::osdp_t,
    #[allow(dead_code)]
    channel: OwnedChannel,
    command_callback: Option<OwnedCallback<PdCommandCallback>>,
    file_ops: Option<OwnedFileOps>,
}

impl core::fmt::Debug for PeripheralDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeripheralDevice").field("ctx", &self.ctx).finish()
    }
}

unsafe impl Send for PeripheralDevice {}

impl PeripheralDevice {
    /// Create a new Peripheral panel object for the PD described by the corresponding PdInfo struct.
    pub fn new(info: PdInfoBuilder, channel: Box<dyn Channel>) -> Result<Self> {
        unsafe { libosdp_sys::osdp_set_log_callback(Some(pd_log_handler)) };
        let channel = OwnedChannel::new(channel);
        let mut raw = channel.as_raw();
        let info: crate::OsdpPdInfoHandle = info.build().into();
        let ctx = unsafe { libosdp_sys::osdp_pd_setup(&mut raw, &*info) };
        if ctx.is_null() {
            return Err(Error::Setup);
        }
        unsafe {
            libosdp_sys::osdp_pd_set_event_completion_callback(
                ctx,
                Some(pd_event_complete),
                core::ptr::null_mut(),
            )
        };
        Ok(Self {
            ctx,
            channel,
            command_callback: None,
            file_ops: None,
        })
    }

    /// This method is used to periodically refresh the underlying LibOSDP state
    /// and must be called from the application. To meet the OSDP timing
    /// guarantees, this function must be called at least once every 50ms. This
    /// method does not block and returns early if there is nothing to be done.
    pub fn refresh(&mut self) {
        unsafe { libosdp_sys::osdp_pd_refresh(self.ctx) }
    }

    /// Set a vector of [`PdCapability`] for this PD.
    pub fn set_capabilities(&mut self, cap: &[PdCapability]) {
        let cap: Vec<libosdp_sys::osdp_pd_cap> = cap
            .iter()
            .map(|c| -> libosdp_sys::osdp_pd_cap { c.clone().into() })
            .collect();
        unsafe { libosdp_sys::osdp_pd_set_capabilities(self.ctx, cap.as_ptr()) }
    }

    /// Flush or drop any events queued in this PD (but not delivered to CP yet)
    pub fn flush_events(&mut self) {
        let _ = unsafe { libosdp_sys::osdp_pd_flush_events(self.ctx) };
    }

    /// Queue an [`OsdpEvent`] for this PD. This will be delivered to CP in
    /// the next POLL.
    pub fn notify_event(&mut self, event: OsdpEvent) -> Result<()> {
        let event = Box::into_raw(Box::new(libosdp_sys::osdp_event::from(event)));
        let rc = unsafe { libosdp_sys::osdp_pd_submit_event(self.ctx, event) };
        if rc < 0 {
            drop(unsafe { Box::from_raw(event) });
            Err(Error::Rejected)
        } else {
            Ok(())
        }
    }

    /// Set a closure that gets called when this PD receives a command from the
    /// CP.
    ///
    /// The closure is moved into the context, which is `Send`, so it must be
    /// `Send` too:
    ///
    /// ```compile_fail
    /// # fn f(pd: &mut libosdp::PeripheralDevice) {
    /// let seen = std::rc::Rc::new(std::cell::Cell::new(0));
    /// pd.set_command_callback(move |_| { seen.set(seen.get() + 1); 0 });
    /// # }
    /// ```
    pub fn set_command_callback<F>(&mut self, closure: F)
    where
        F: FnMut(&mut OsdpCommand) -> i32 + Send + 'static,
    {
        let callback = OwnedCallback::<PdCommandCallback>::new(Box::new(closure));
        unsafe {
            libosdp_sys::osdp_pd_set_command_callback(
                self.ctx,
                Some(pd_command_trampoline),
                callback.data(),
            )
        }
        self.command_callback = Some(callback);
    }

    /// Check online status of a PD identified by the offset number (in PdInfo
    /// vector in [`PeripheralDevice::new`]).
    pub fn is_online(&self) -> bool {
        let mut buf: u8 = 0;
        unsafe { libosdp_sys::osdp_get_status_mask(self.ctx, &mut buf as *mut u8) };
        buf != 0
    }

    /// Check secure channel status of a PD identified by the offset number
    /// (in PdInfo vector in [`PeripheralDevice::new`]). Returns (size, offset)
    /// of the current file transfer operation.
    pub fn is_sc_active(&self) -> bool {
        let mut buf: u8 = 0;
        unsafe { libosdp_sys::osdp_get_sc_status_mask(self.ctx, &mut buf as *mut u8) };
        buf != 0
    }

    /// Get status of the ongoing file transfer of PD as (size, offset), or
    /// [`Error::NotInProgress`] once the transfer has ended, however it ended.
    pub fn file_transfer_status(&self) -> Result<(i32, i32)> {
        let mut size: u32 = 0;
        let mut offset: u32 = 0;
        let rc = unsafe {
            libosdp_sys::osdp_get_file_tx_status(self.ctx, 0, &mut size, &mut offset)
        };
        if rc < 0 {
            Err(Error::NotInProgress)
        } else {
            Ok((size as i32, offset as i32))
        }
    }

    /// Register a file operations handler for PD. See [`crate::OsdpFileOps`]
    /// trait documentation for more details.
    pub fn register_file_ops(&mut self, fops: Box<dyn OsdpFileOps>) -> Result<()> {
        let fops = OwnedFileOps::new(fops);
        let raw = fops.as_raw();
        let rc = unsafe { libosdp_sys::osdp_file_register_ops(self.ctx, 0, &raw) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            self.file_ops = Some(fops);
            Ok(())
        }
    }
}

impl Drop for PeripheralDevice {
    fn drop(&mut self) {
        unsafe { libosdp_sys::osdp_pd_teardown(self.ctx) }
    }
}
