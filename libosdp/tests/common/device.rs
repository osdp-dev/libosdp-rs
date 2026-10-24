//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{mpsc::Receiver, Arc, Mutex, MutexGuard},
    thread, time,
};

use libosdp::{
    CapFunction, Capability, ControlPanel, ControlPanelBuilder, Command, CpEvent, Event, FileOps,
    PdIndex, PdInfoBuilder, PeripheralDevice, Responder,
};
type Result<T> = core::result::Result<T, libosdp::Error>;

pub struct CpDevice {
    dev: Arc<Mutex<ControlPanel>>,
    #[allow(unused)] // false positive, it is used!
    pub receiver: Receiver<(PdIndex, Event)>,
    pub pd: PdIndex,
}

impl CpDevice {
    pub fn new(
        bus: Box<dyn libosdp::Channel>,
        file_ops: Option<Box<dyn FileOps>>,
    ) -> Result<Self> {
        #[rustfmt::skip]
        let pd_0_key = [
            0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
            0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9
        ];

        let mut pd_0 = PdInfoBuilder::new()
            .name("PD 101")?
            .address(101)?
            .baud_rate(115200)?
            .secure_channel_key(pd_0_key);
        if let Some(file_ops) = file_ops {
            pd_0 = pd_0.file_ops(file_ops);
        }
        let mut builder = ControlPanelBuilder::new(bus);
        let pd = builder.add_pd(pd_0)?;
        let cp = builder.build()?;
        let (event_tx, event_rx) = std::sync::mpsc::channel::<(PdIndex, Event)>();

        let dev = Arc::new(Mutex::new(cp));
        let dev_clone = dev.clone();
        let _ = thread::Builder::new()
            .name("CP Thread".to_string())
            .spawn(move || {
                let dev = dev_clone;
                let sender = event_tx;
                loop {
                    {
                        let cp = dev.lock().unwrap();
                        cp.refresh();
                        while let Some((pd, item)) = cp.next_event() {
                            if let CpEvent::Event(event) = item {
                                sender.send((pd, event)).expect("CP event send");
                            }
                        }
                    }
                    thread::sleep(time::Duration::from_millis(10));
                }
            });
        Ok(Self {
            dev,
            receiver: event_rx,
            pd,
        })
    }

    pub fn get_device(&self) -> MutexGuard<'_, ControlPanel> {
        self.dev.lock().unwrap()
    }
}

pub struct PdDevice {
    dev: Arc<Mutex<PeripheralDevice>>,
    pub receiver: Receiver<Command>,
}

impl PdDevice {
    pub fn new(
        bus: Box<dyn libosdp::Channel>,
        file_ops: Option<Box<dyn FileOps>>,
    ) -> Result<Self> {
        #[rustfmt::skip]
        let key = [
            0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
            0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9
        ];

        let mut pd_info = PdInfoBuilder::new()
            .name("PD 101")?
            .address(101)?
            .baud_rate(115200)?
            .capability(Capability::new(CapFunction::CommunicationSecurity, 1, 1))
            .capability(Capability::new(CapFunction::AudibleOutput, 1, 1))
            .capability(Capability::new(CapFunction::LedControl, 1, 1))
            .secure_channel_key(key);
        if let Some(file_ops) = file_ops {
            pd_info = pd_info.file_ops(file_ops);
        }
        let pd = PeripheralDevice::new(pd_info, bus)?;
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Command>();
        let dev = Arc::new(Mutex::new(pd));
        let dev_clone = dev.clone();
        let _ = thread::Builder::new()
            .name("PD Thread".to_string())
            .spawn(move || {
                let dev = dev_clone;
                let sender = cmd_tx;
                dev.lock()
                    .unwrap()
                    .set_command_handler(move |command, _: &mut Responder<'_>| {
                        sender.send(command).expect("PD command send");
                        Ok(())
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
