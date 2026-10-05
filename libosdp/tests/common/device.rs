//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{mpsc::Receiver, Arc, Mutex, MutexGuard},
    thread, time,
};

use libosdp::{
    ControlPanel, ControlPanelBuilder, OsdpCommand, OsdpEvent, PdCapEntity, PdCapability,
    PdInfoBuilder, PeripheralDevice,
};
type Result<T> = core::result::Result<T, libosdp::Error>;

pub struct CpDevice {
    dev: Arc<Mutex<ControlPanel>>,
    #[allow(unused)] // false positive, it is used!
    pub receiver: Receiver<(i32, OsdpEvent)>,
}

impl CpDevice {
    pub fn new(bus: Box<dyn libosdp::Channel>) -> Result<Self> {
        #[rustfmt::skip]
        let pd_0_key = [
            0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
            0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9
        ];

        let pd_0 = PdInfoBuilder::new()
            .name("PD 101")?
            .address(101)?
            .baud_rate(115200)?
            .secure_channel_key(pd_0_key);
        let cp = ControlPanelBuilder::new(bus).add_pd(pd_0).build()?;
        let (event_tx, event_rx) = std::sync::mpsc::channel::<(i32, OsdpEvent)>();

        let dev = Arc::new(Mutex::new(cp));
        let dev_clone = dev.clone();
        let _ = thread::Builder::new()
            .name("CP Thread".to_string())
            .spawn(move || {
                let dev = dev_clone;
                let sender = event_tx;
                dev.lock().unwrap().set_event_callback(move |pd, event| {
                    sender.send((pd, event)).expect("CP event send");
                    0
                });
                loop {
                    dev.lock().unwrap().refresh();
                    thread::sleep(time::Duration::from_millis(10));
                }
            });
        Ok(Self {
            dev,
            receiver: event_rx,
        })
    }

    pub fn get_device(&self) -> MutexGuard<'_, ControlPanel> {
        self.dev.lock().unwrap()
    }
}

pub struct PdDevice {
    dev: Arc<Mutex<PeripheralDevice>>,
    pub receiver: Receiver<OsdpCommand>,
}

impl PdDevice {
    pub fn new(bus: Box<dyn libosdp::Channel>) -> Result<Self> {
        #[rustfmt::skip]
        let key = [
            0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
            0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9
        ];

        let pd_info = PdInfoBuilder::new()
            .name("PD 101")?
            .address(101)?
            .baud_rate(115200)?
            .capability(PdCapability::CommunicationSecurity(PdCapEntity::new(1, 1)))
            .capability(PdCapability::AudibleOutput(PdCapEntity::new(1, 1)))
            .capability(PdCapability::LedControl(PdCapEntity::new(1, 1)))
            .secure_channel_key(key);
        let pd = PeripheralDevice::new(pd_info, bus)?;
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<OsdpCommand>();
        let dev = Arc::new(Mutex::new(pd));
        let dev_clone = dev.clone();
        let _ = thread::Builder::new()
            .name("PD Thread".to_string())
            .spawn(move || {
                let dev = dev_clone;
                let sender = cmd_tx;
                dev.lock().unwrap().set_command_callback(move |command| {
                    sender.send(command.clone()).expect("PD command send");
                    0
                });
                loop {
                    dev.lock().unwrap().refresh();
                    thread::sleep(time::Duration::from_millis(10));
                }
            });

        Ok(Self {
            dev,
            receiver: cmd_rx,
        })
    }

    pub fn get_device(&self) -> MutexGuard<'_, PeripheralDevice> {
        self.dev.lock().unwrap()
    }
}
