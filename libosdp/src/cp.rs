//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! The Control Panel (CP) is responsible to connecting to and managing multiple Peripheral Devices
//! (PD) on the OSDP bus. It can send commands to and receive events from PDs.

use crate::{
    file::OsdpFileOps, Channel, Error, OsdpCommand, OsdpEvent, OsdpFlag, PdCapability, PdId,
    PdInfoBuilder,
};
use crate::ffi::{
    callback::{cp_event_trampoline, CpEventCallback, OwnedCallback},
    channel::OwnedChannel,
    file::OwnedFileOps,
    log::cp_log_handler,
    submit::cp_command_complete,
};
use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};

type Result<T> = core::result::Result<T, Error>;

/// Builder for a [`ControlPanel`] driving the PDs on one OSDP bus.
#[derive(Debug)]
pub struct ControlPanelBuilder {
    channel: Box<dyn Channel>,
    pd_info: Vec<PdInfoBuilder>,
}

impl ControlPanelBuilder {
    /// Create a builder for the PDs reachable over `channel`.
    pub fn new(channel: Box<dyn Channel>) -> Self {
        Self {
            channel,
            pd_info: Vec::new(),
        }
    }

    /// Add a PD on this bus. PDs are addressed by the order they were added in.
    pub fn add_pd(mut self, pd_info: PdInfoBuilder) -> Self {
        self.pd_info.push(pd_info);
        self
    }

    /// Build the [`ControlPanel`] instance.
    pub fn build(self) -> Result<ControlPanel> {
        if self.pd_info.len() > 126 {
            return Err(Error::InvalidArgument("more than 126 PDs on one bus"));
        }
        let info: Vec<crate::OsdpPdInfoHandle> =
            self.pd_info.into_iter().map(|pd| pd.build().into()).collect();
        let channel = OwnedChannel::new(self.channel);
        let raw = channel.as_raw();
        unsafe { libosdp_sys::osdp_set_log_callback(Some(cp_log_handler)) };
        let ctx = unsafe {
            libosdp_sys::osdp_cp_setup(&raw, info.len() as i32, info.as_ptr() as *const _)
        };
        if ctx.is_null() {
            return Err(Error::Setup);
        }
        unsafe {
            libosdp_sys::osdp_cp_set_command_completion_callback(
                ctx,
                Some(cp_command_complete),
                core::ptr::null_mut(),
            )
        };
        Ok(ControlPanel {
            ctx,
            channel,
            event_callback: None,
            file_ops: BTreeMap::new(),
        })
    }
}

/// OSDP CP device context.
pub struct ControlPanel {
    ctx: *mut core::ffi::c_void,
    #[allow(dead_code)]
    channel: OwnedChannel,
    event_callback: Option<OwnedCallback<CpEventCallback>>,
    file_ops: BTreeMap<i32, OwnedFileOps>,
}

impl core::fmt::Debug for ControlPanel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ControlPanel").field("ctx", &self.ctx).finish()
    }
}

unsafe impl Send for ControlPanel {}

impl ControlPanel {
    /// The application must call this method periodically to refresh the
    /// underlying LibOSDP state. To meet the OSDP timing guarantees, this
    /// function must be called at least once every 50ms. This method does not
    /// block and returns early if there is nothing to be done.
    pub fn refresh(&mut self) {
        unsafe { libosdp_sys::osdp_cp_refresh(self.ctx) }
    }

    /// Send [`OsdpCommand`] to a PD identified by the offset number (in the
    /// order PDs were added to the [`ControlPanelBuilder`]).
    pub fn send_command(&mut self, pd: i32, cmd: OsdpCommand) -> Result<()> {
        let cmd = Box::into_raw(Box::new(libosdp_sys::osdp_cmd::from(cmd)));
        let rc = unsafe { libosdp_sys::osdp_cp_submit_command(self.ctx, pd, cmd) };
        if rc < 0 {
            drop(unsafe { Box::from_raw(cmd) });
            Err(Error::Rejected)
        } else {
            Ok(())
        }
    }

    /// Set a closure that gets called when a PD sends an event to this CP.
    ///
    /// The closure is moved into the context, which is `Send`, so it must be
    /// `Send` too:
    ///
    /// ```compile_fail
    /// # fn f(cp: &mut libosdp::ControlPanel) {
    /// let seen = std::rc::Rc::new(std::cell::Cell::new(0));
    /// cp.set_event_callback(move |_, _| { seen.set(seen.get() + 1); 0 });
    /// # }
    /// ```
    pub fn set_event_callback<F>(&mut self, closure: F)
    where
        F: FnMut(i32, OsdpEvent) -> i32 + Send + 'static,
    {
        let callback = OwnedCallback::<CpEventCallback>::new(Box::new(closure));
        unsafe {
            libosdp_sys::osdp_cp_set_event_callback(
                self.ctx,
                Some(cp_event_trampoline),
                callback.data(),
            );
        }
        self.event_callback = Some(callback);
    }

    /// Get the [`PdId`] from a PD identified by the offset number (in PdInfo
    /// vector in [`ControlPanel::new`]).
    pub fn get_pd_id(&self, pd: i32) -> Result<PdId> {
        let mut pd_id: libosdp_sys::osdp_pd_id =
            unsafe { core::mem::MaybeUninit::zeroed().assume_init() };
        let rc = unsafe { libosdp_sys::osdp_cp_get_pd_id(self.ctx, pd, &mut pd_id) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            Ok(pd_id.into())
        }
    }

    /// Get the [`PdCapability`] from a PD identified by the offset number (in
    /// PdInfo vector in [`ControlPanel::new`]).
    pub fn get_capability(&self, pd: i32, cap: PdCapability) -> Result<PdCapability> {
        let mut cap = cap.into();
        let rc = unsafe { libosdp_sys::osdp_cp_get_capability(self.ctx, pd, &mut cap) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            Ok(cap.try_into()?)
        }
    }

    /// Set [`OsdpFlag`] for a PD identified by the offset number (in PdInfo
    /// vector in [`ControlPanel::new`]).
    ///
    /// # Panics
    ///
    /// Panics when LibOSDP refuses the change: setting
    /// [`OsdpFlag::InstallMode`], clearing [`OsdpFlag::EnforceSecure`], or
    /// setting [`OsdpFlag::EnforceSecure`] on a PD without an SCBK.
    pub fn set_flag(&mut self, pd: i32, flags: OsdpFlag, value: bool) {
        let rc = unsafe { libosdp_sys::osdp_cp_modify_flag(self.ctx, pd, flags.bits(), value) };
        if rc < 0 {
            // The flag API has no error path, so a refused change panics.
            panic!("osdp_cp_modify_flag failed!")
        }
    }

    /// Check online status of a PD identified by the offset number (in PdInfo
    /// vector in [`ControlPanel::new`]).
    pub fn is_online(&self, pd: i32) -> bool {
        let mut buf: [u8; 16] = [0; 16];
        unsafe { libosdp_sys::osdp_get_status_mask(self.ctx, &mut buf as *mut u8) };
        let pos = pd / 8;
        let idx = pd % 8;
        buf[pos as usize] & (1 << idx) != 0
    }

    /// Check secure channel status of a PD identified by the offset number
    /// (in PdInfo vector in [`ControlPanel::new`]).
    pub fn is_sc_active(&self, pd: i32) -> bool {
        let mut buf: [u8; 16] = [0; 16];
        unsafe { libosdp_sys::osdp_get_sc_status_mask(self.ctx, &mut buf as *mut u8) };
        let pos = pd / 8;
        let idx = pd % 8;
        buf[pos as usize] & (1 << idx) != 0
    }

    /// Get status of the ongoing file transfer of a PD, identified by the
    /// offset number (in PdInfo vector in [`ControlPanel::new`]). Returns
    /// (size, offset) of the current file transfer operation, or
    /// [`Error::NotInProgress`] once the transfer has ended, however it ended.
    pub fn file_transfer_status(&self, pd: i32) -> Result<(i32, i32)> {
        let mut size: u32 = 0;
        let mut offset: u32 = 0;
        let rc = unsafe {
            libosdp_sys::osdp_get_file_tx_status(self.ctx, pd, &mut size, &mut offset)
        };
        if rc < 0 {
            Err(Error::NotInProgress)
        } else {
            Ok((size as i32, offset as i32))
        }
    }

    /// Register a file operations handler for a PD. See [`crate::OsdpFileOps`]
    /// trait documentation for more details.
    pub fn register_file_ops(&mut self, pd: i32, fops: Box<dyn OsdpFileOps>) -> Result<()> {
        let fops = OwnedFileOps::new(fops);
        let raw = fops.as_raw();
        let rc = unsafe { libosdp_sys::osdp_file_register_ops(self.ctx, pd, &raw) };
        if rc < 0 {
            Err(Error::Rejected)
        } else {
            self.file_ops.insert(pd, fops);
            Ok(())
        }
    }
}

impl Drop for ControlPanel {
    fn drop(&mut self) {
        unsafe { libosdp_sys::osdp_cp_teardown(self.ctx) }
    }
}
