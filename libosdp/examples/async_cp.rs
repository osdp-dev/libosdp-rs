//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use libosdp::{Buzzer, BuzzerCommand, Channel, ChannelError, Command, Error, Flag, PdInfoBuilder};
use std::{env, time::Duration};

struct OsdpChannel;

impl OsdpChannel {
    fn new(_path: &str) -> Self {
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
    let _pd = builder.add_pd(pd_0)?;
    let cp = builder.build()?;

    let app = async {
        let mut events = cp.events();
        while let Some((pd, event)) = events.next().await {
            println!("PD {pd:?}: {event:?}");
            if cp.is_online(pd) {
                let buzz = Command::Buzzer(BuzzerCommand {
                    reader: 0,
                    action: Buzzer::Off,
                });
                match cp.submit(pd, buzz)?.await {
                    Ok(()) => println!("PD {pd:?} took the command"),
                    Err(e) => println!("PD {pd:?}: {e:?}"),
                }
            }
        }
        Ok::<(), Error>(())
    };
    let driver = cp.run(|| tokio::time::sleep(Duration::from_millis(20)));
    tokio::select! {
        result = app => result,
        _ = driver => unreachable!(),
    }
}
