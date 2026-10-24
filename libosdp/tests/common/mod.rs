//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

pub mod device;
pub mod memory_channel;
pub mod threadbus;

pub fn setup() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_target(false)
        .format_timestamp(None)
        .init();
}
