//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Glue between LibOSDP's C callbacks and the safe Rust API. Every `unsafe`
//! trampoline lives under this module.

pub(crate) mod callback;
pub(crate) mod channel;
pub(crate) mod file;
pub(crate) mod log;
