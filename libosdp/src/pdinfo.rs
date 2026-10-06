//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::{Error, Flag, PdCapability, PdId};
use alloc::{boxed::Box, ffi::CString, string::String, vec::Vec};
use core::ops::Deref;

/// OSDP PD Information. This struct is used to describe a PD to LibOSDP
#[derive(Debug, Default)]
pub struct PdInfo {
    name: CString,
    address: i32,
    baud_rate: i32,
    flags: Flag,
    id: PdId,
    cap: Vec<libosdp_sys::osdp_pd_cap>,
    scbk: Option<[u8; 16]>,
}
impl PdInfo {
    /// Gets the PDs `name`
    /// A user provided `name` for this PD (log messages include this name defaults to `pd-<address>`)
    ///
    /// # Example
    /// ```
    /// # use libosdp::PdInfoBuilder;
    /// let pd = PdInfoBuilder::new().name("door_42").unwrap().build();
    /// assert_eq!(pd.name(), "door_42".to_string());
    /// ```
    #[must_use]
    pub fn name(&self) -> String {
        self.name
            .clone()
            .into_string()
            .expect("since this is configured with a &str, this must be valid String")
    }
    /// Gets the PDs 7 bit `address`
    /// The special address 0x7F is used for broadcast.
    /// So there can be 2^7-1 valid addresses on a bus.
    ///
    /// # Example
    /// ```
    /// # use libosdp::PdInfoBuilder;
    /// let pd = PdInfoBuilder::new().address(42).unwrap().build();
    /// assert_eq!(pd.address(), 42);
    /// ```
    #[must_use]
    pub fn address(&self) -> i32 {
        self.address
    }

    /// Gets the PDs baud rate.
    /// Can be one of `9600`/`19200`/`38400`/`57600`/`115200`/`230400`
    ///
    /// # Example
    /// ```
    /// # use libosdp::PdInfoBuilder;
    /// let pd = PdInfoBuilder::new().baud_rate(9600).unwrap().build();
    /// assert_eq!(pd.baud_rate(), 9600);
    /// ```
    pub fn baud_rate(&self) -> i32 {
        self.baud_rate
    }

    /// Gets the PDs [`Flag`]
    /// Used to modify the way the context is set up
    ///
    /// # Example
    /// ```
    /// # use libosdp::{Flag, PdInfoBuilder};
    /// let pd = PdInfoBuilder::new().flag(Flag::EnforceSecure).build();
    /// assert_eq!(pd.flag(), Flag::EnforceSecure);
    /// ```
    #[must_use]
    pub fn flag(&self) -> Flag {
        self.flags
    }

    /// Gets the PDs' [`PdId`]
    /// Static information that the PD reports to the CP when it received a `CMD_ID`.
    /// For CP mode, this field is ignored, but PD mode must set it
    ///
    /// # Example
    /// ```
    /// # use libosdp::{PdId, PdInfoBuilder};
    /// let pd = PdInfoBuilder::new().id(&PdId::from_number(42)).build();
    /// assert_eq!(pd.id(), PdId::from_number(42));
    /// ```
    #[must_use]
    pub fn id(&self) -> PdId {
        self.id
    }

    /// Get a PDs [`PdCapability`]s
    ///
    /// # Example
    /// ```
    /// # use libosdp::{PdCapability, PdInfoBuilder, PdCapEntity};
    /// let pd = PdInfoBuilder::new()
    ///             .capability(PdCapability::CommunicationSecurity(PdCapEntity::new(1, 1)))
    ///             .capability(PdCapability::AudibleOutput(PdCapEntity::new(1, 1)))
    ///             .build();
    /// assert_eq!(
    ///   pd.capabilities(),
    ///   vec![PdCapability::CommunicationSecurity(PdCapEntity::new(1, 1)), PdCapability::AudibleOutput(PdCapEntity::new(1, 1))]
    /// );
    /// ```
    #[must_use]
    pub fn capabilities(&self) -> Vec<PdCapability> {
        self.cap.iter().cloned().map(PdCapability::from).collect()
    }

    /// Get a PDs secure channel key.
    /// Without a key the secure channel is disabled; set
    /// [`Flag::InstallMode`] to bring it up on SCBK-D.
    ///
    /// # Example
    /// ```
    /// # use libosdp::PdInfoBuilder;
    /// # #[rustfmt::skip]
    /// # let pd_0_key = [
    /// #   0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
    /// #   0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9
    /// # ];
    /// let pd = PdInfoBuilder::new().secure_channel_key(pd_0_key).build();
    /// assert_eq!(pd.secure_channel_key(), Some(pd_0_key));
    /// ```

    #[must_use]
    pub fn secure_channel_key(&self) -> Option<[u8; 16]> {
        self.scbk
    }
}

/// OSDP PD Info Builder
#[derive(Debug, Default)]
pub struct PdInfoBuilder {
    name: Option<CString>,
    address: i32,
    baud_rate: i32,
    flags: Flag,
    id: PdId,
    cap: Vec<libosdp_sys::osdp_pd_cap>,
    scbk: Option<[u8; 16]>,
}

impl PdInfoBuilder {
    /// Create am instance of PdInfo builder
    pub fn new() -> PdInfoBuilder {
        PdInfoBuilder::default()
    }

    /// Set PD name; a user provided name for this PD (log messages include this
    /// name defaults to `pd-<address>`)
    pub fn name(mut self, name: &str) -> Result<PdInfoBuilder, Error> {
        let name = CString::new(name).map_err(|_| Error::InvalidArgument("PD name contains a NUL"))?;
        self.name = Some(name);
        Ok(self)
    }

    /// Set 7 bit PD address; the special address 0x7F is used for broadcast. So
    /// there can be 2^7-1 valid addresses on a bus.
    pub fn address(mut self, address: i32) -> Result<PdInfoBuilder, Error> {
        if address < 0 || address > 126 {
            return Err(Error::InvalidArgument("PD address outside 0-126"));
        }
        self.address = address;
        Ok(self)
    }

    /// Set baud rate; can be one of `9600`/`19200`/`38400`/`57600`/`115200`/`230400`
    pub fn baud_rate(mut self, baud_rate: i32) -> Result<PdInfoBuilder, Error> {
        if baud_rate != 9600
            && baud_rate != 19200
            && baud_rate != 38400
            && baud_rate != 57600
            && baud_rate != 115200
            && baud_rate != 230400
        {
            return Err(Error::InvalidArgument("unsupported baud rate"));
        }
        self.baud_rate = baud_rate;
        Ok(self)
    }

    /// Set flags for the PD; used to modify the way the context is setup
    pub fn flag(mut self, flag: Flag) -> PdInfoBuilder {
        self.flags.set(flag, true);
        self
    }

    /// Set PD ID; Static information that the PD reports to the CP when it
    /// received a `CMD_ID`. For CP mode, this field is ignored, but PD mode
    /// must set it
    pub fn id(mut self, id: &PdId) -> PdInfoBuilder {
        self.id = *id;
        self
    }

    /// Set a PD capability
    pub fn capability(mut self, cap: PdCapability) -> PdInfoBuilder {
        self.cap.push(cap.into());
        self
    }

    /// Set multiple capabilities at once
    pub fn capabilities<'a, I>(mut self, caps: I) -> PdInfoBuilder
    where
        I: IntoIterator<Item = &'a PdCapability>,
    {
        for cap in caps {
            self.cap.push(cap.clone().into());
        }
        self
    }

    /// Set secure channel key. Without a key the secure channel is
    /// disabled; set [`Flag::InstallMode`] to bring it up on SCBK-D.
    pub fn secure_channel_key(mut self, key: [u8; 16]) -> PdInfoBuilder {
        self.scbk = Some(key);
        self
    }

    /// Finalize the PdInfo from the current builder
    pub fn build(self) -> PdInfo {
        let name = self.name.unwrap_or_else(|| {
            let mut buffer = itoa::Buffer::new();
            let s = buffer.format(self.address as u8);
            let mut buf = [0u8; 6];
            let buf = &mut buf[..3 + s.len()];
            buf[..3].copy_from_slice(b"PD-");
            buf[3..].copy_from_slice(s.as_bytes());
            CString::new(buf)
                .ok() // panic can only happen if buf contained a null byte, which it never will
                .expect("fallback PD name could not be constrcuted")
        });
        PdInfo {
            name,
            address: self.address,
            baud_rate: self.baud_rate,
            flags: self.flags,
            id: self.id,
            cap: self.cap,
            scbk: self.scbk,
        }
    }
}

#[repr(transparent)]
pub(crate) struct PdInfoHandle(pub libosdp_sys::osdp_pd_info_t);

impl Deref for PdInfoHandle {
    type Target = libosdp_sys::osdp_pd_info_t;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// `caps` as the array LibOSDP reads, which it walks up to a sentinel entry.
pub(crate) fn terminated_capabilities(
    caps: impl IntoIterator<Item = libosdp_sys::osdp_pd_cap>,
) -> Vec<libosdp_sys::osdp_pd_cap> {
    caps.into_iter()
        .chain(core::iter::once(libosdp_sys::osdp_pd_cap {
            function_code: libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_SENTINEL,
            compliance_level: 0,
            num_items: 0,
        }))
        .collect()
}

impl From<PdInfo> for PdInfoHandle {
    fn from(info: PdInfo) -> PdInfoHandle {
        let scbk = if let Some(key) = info.scbk {
            Box::into_raw(Box::new(key)) as *mut _
        } else {
            core::ptr::null_mut::<u8>()
        };
        let cap = if !info.cap.is_empty() {
            let cap = terminated_capabilities(info.cap.iter().copied());
            Box::into_raw(cap.into_boxed_slice()) as *mut _
        } else {
            core::ptr::null_mut::<libosdp_sys::osdp_pd_cap>()
        };
        PdInfoHandle(libosdp_sys::osdp_pd_info_t {
            name: info.name.clone().into_raw(),
            baud_rate: info.baud_rate as u32,
            address: info.address,
            flags: info.flags.bits(),
            id: info.id.into(),
            cap: cap as *mut _,
            scbk,
        })
    }
}

impl Drop for PdInfoHandle {
    fn drop(&mut self) {
        unsafe {
            let info = self.0;
            if !info.name.is_null() {
                drop(CString::from_raw(info.name as *mut _));
            }
            if !info.cap.is_null() {
                let mut cap = info.cap as *mut libosdp_sys::osdp_pd_cap;
                while (*cap).function_code
                    != libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_SENTINEL
                {
                    cap = cap.add(1);
                }
                let len = (cap.offset_from(info.cap) + 1) as usize;
                drop(Vec::from_raw_parts(
                    info.cap as *mut libosdp_sys::osdp_pd_cap,
                    len,
                    len,
                ));
            }
            if !info.scbk.is_null() {
                drop(Box::from_raw(info.scbk as *mut [u8; 16]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::terminated_capabilities;

    #[test]
    fn capability_list_ends_with_the_sentinel() {
        let led = libosdp_sys::osdp_pd_cap {
            function_code: libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_READER_LED_CONTROL,
            compliance_level: 1,
            num_items: 4,
        };
        for caps in [&[][..], &[led][..]] {
            let raw = terminated_capabilities(caps.iter().copied());
            assert_eq!(raw.len(), caps.len() + 1);
            assert_eq!(
                raw.last().unwrap().function_code,
                libosdp_sys::osdp_pd_cap_function_code_e_OSDP_PD_CAP_SENTINEL
            );
        }
    }
}
