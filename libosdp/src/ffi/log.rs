//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "defmt")]
use defmt::{debug, error, info, warn};
#[cfg(all(feature = "log", not(feature = "defmt")))]
use log::{debug, error, info, warn};

pub(crate) unsafe extern "C" fn cp_log_handler(
    _pd: ::core::ffi::c_int,
    _log_level: ::core::ffi::c_int,
    _msg: *const ::core::ffi::c_char,
    _file: *const ::core::ffi::c_char,
    _line: ::core::ffi::c_ulong,
) {
    #[cfg(any(feature = "log", feature = "defmt"))]
    {
        let msg = crate::cstr_to_string(_msg);
        let msg = msg.trim();
        match _log_level as libosdp_sys::osdp_log_level_e {
            libosdp_sys::osdp_log_level_e_OSDP_LOG_EMERG => error!("CP: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_ALERT => error!("CP: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_CRIT => error!("CP: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_ERROR => error!("CP: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_WARNING => warn!("CP: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_NOTICE => warn!("CP: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_INFO => info!("CP: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_DEBUG => debug!("CP: {}", msg),
            _ => panic!("Unknown log level"),
        };
    }
}

pub(crate) unsafe extern "C" fn pd_log_handler(
    _pd: ::core::ffi::c_int,
    _log_level: ::core::ffi::c_int,
    _msg: *const ::core::ffi::c_char,
    _file: *const ::core::ffi::c_char,
    _line: ::core::ffi::c_ulong,
) {
    #[cfg(any(feature = "log", feature = "defmt"))]
    {
        let msg = crate::cstr_to_string(_msg);
        let msg = msg.trim();
        match _log_level as libosdp_sys::osdp_log_level_e {
            libosdp_sys::osdp_log_level_e_OSDP_LOG_EMERG => error!("PD: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_ALERT => error!("PD: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_CRIT => error!("PD: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_ERROR => error!("PD: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_WARNING => warn!("PD: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_NOTICE => warn!("PD: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_INFO => info!("PD: {}", msg),
            libosdp_sys::osdp_log_level_e_OSDP_LOG_DEBUG => debug!("PD: {}", msg),
            _ => panic!("Unknown log level"),
        };
    }
}
