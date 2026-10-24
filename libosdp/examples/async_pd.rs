//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use libosdp::{
    CapFunction, Capability, Channel, ChannelError, Error, Flag, PdInfoBuilder, Responder,
};
use std::time::Duration;

struct OsdpChannel;

impl OsdpChannel {
    pub fn new(_path: &str) -> Self {
        // setup device
        Self {}
    }
}

/// Read documentation for each member in [libosdp::Channel].
impl Channel for OsdpChannel {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, ChannelError> {
        // TODO: Read from device
        Ok(0)
    }

    fn write(&mut self, _buf: &[u8]) -> Result<(), ChannelError> {
        // TODO: Queue all of buf on the device, or return WouldBlock
        Ok(())
    }

    fn flush(&mut self) {
        // TODO: Discard what the device has buffered
    }

    fn close(&mut self) {
        // TODO: close device
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_target(false)
        .format_timestamp(None)
        .init();
    let args: Vec<String> = std::env::args().collect();
    let channel = OsdpChannel::new(&args[1]);

    #[rustfmt::skip]
    let key = [
        0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
        0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9
    ];

    let pd_info = PdInfoBuilder::new()
        .name("PD 101")?
        .address(101)?
        .baud_rate(115200)?
        .flag(Flag::EnforceSecure)
        .flag(Flag::EnableNotification)
        .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
        .secure_channel_key(key);
    let pd = libosdp::PeripheralDevice::new(pd_info, Box::new(channel))?;
    pd.set_command_handler(|command, _: &mut Responder<'_>| {
        println!("Received {command:?}");
        Ok(())
    });

    let notifications = async {
        let mut notifications = pd.notifications();
        while let Some(n) = notifications.next().await {
            println!("{n:?}");
        }
    };
    let driver = pd.run(|| tokio::time::sleep(Duration::from_millis(20)));
    tokio::select! {
        _ = notifications => unreachable!(),
        _ = driver => unreachable!(),
    }
}
