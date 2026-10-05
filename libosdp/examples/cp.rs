//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use libosdp::{Channel, ChannelError, Error, Flag, PdInfoBuilder};
use std::{env, thread, time::Duration};

struct OsdpChannel;

impl OsdpChannel {
    fn new(_path: &str) -> Self {
        // setup device
        Self {}
    }
}

/// Read documentation for each member in [libosdp::Channel].
impl Channel for OsdpChannel {
    fn get_id(&self) -> i32 {
        0
    }

    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, ChannelError> {
        // TODO: Read from device
        Ok(0)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, ChannelError> {
        // TODO: Write from device
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), ChannelError> {
        // TODO: flush device
        Ok(())
    }

    fn close(&mut self) -> Result<(), ChannelError> {
        // TODO: close device
        Ok(())
    }
}

fn main() -> Result<(), Error> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_target(false)
        .format_timestamp(None)
        .init();
    let args: Vec<String> = env::args().collect();
    let channel = OsdpChannel::new(&args[1]);

    #[rustfmt::skip]
    let pd_0_key = [
        0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
        0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9
    ];

    let pd_0 = PdInfoBuilder::new()
        .name("PD 101")?
        .address(101)?
        .baud_rate(115200)?
        .flag(Flag::EnforceSecure)
        .secure_channel_key(pd_0_key);
    let mut builder = libosdp::ControlPanelBuilder::new(Box::new(channel));
    builder.add_pd(pd_0)?;
    let mut cp = builder.build()?;
    loop {
        cp.refresh();
        thread::sleep(Duration::from_millis(50));
    }
}
