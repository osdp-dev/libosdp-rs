//
// Copyright (c) 2024 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

mod common;

type Result<T> = core::result::Result<T, libosdp::OsdpError>;

use core::time::Duration;
use libosdp::{OsdpCommand, OsdpCommandFileTx, OsdpError, OsdpFileOps};
use rand::RngExt;
use std::{
    cmp,
    collections::HashMap,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    str::FromStr,
    thread,
};

use crate::common::{device::CpDevice, device::PdDevice, memory_channel::MemoryChannel};

#[cfg(not(target_os = "windows"))]
use std::os::unix::prelude::FileExt;
#[cfg(target_os = "windows")]
use std::os::windows::fs::FileExt;

/// OSDP file transfer context
#[derive(Debug, Default)]
pub struct OsdpFileManager {
    files: HashMap<i32, PathBuf>,
    file: Option<File>,
}

impl OsdpFileManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_file(&mut self, id: i32, path: &str) {
        let _ = self.files.insert(id, PathBuf::from_str(path).unwrap());
    }
}

impl OsdpFileOps for OsdpFileManager {
    fn open(&mut self, id: i32, read_only: bool) -> Result<usize> {
        let path = self
            .files
            .get(&id)
            .ok_or(OsdpError::FileTransfer("Invalid file ID"))?;
        log::debug!("File {:?}", path);
        let file = if read_only {
            File::open(path.as_os_str())?
        } else {
            File::create(path.as_os_str())?
        };
        let size = file.metadata()?.len() as usize;
        self.file = Some(file);
        Ok(size)
    }

    fn offset_read(&self, buf: &mut [u8], off: u64) -> Result<usize> {
        let file = self
            .file
            .as_ref()
            .ok_or(OsdpError::FileTransfer("File not open"))?;

        #[cfg(not(target_os = "windows"))]
        let r = file.read_at(buf, off)?;

        #[cfg(target_os = "windows")]
        let r = file.seek_read(buf, off)?;

        Ok(r)
    }

    fn offset_write(&self, buf: &[u8], off: u64) -> Result<usize> {
        let file = self
            .file
            .as_ref()
            .ok_or(OsdpError::FileTransfer("File not open"))?;

        #[cfg(not(target_os = "windows"))]
        let r = file.write_at(buf, off)?;

        #[cfg(target_os = "windows")]
        let r = file.seek_write(buf, off)?;

        Ok(r)
    }

    fn close(&mut self) -> Result<()> {
        let _ = self.file.take().unwrap();
        Ok(())
    }
}

fn create_random_file<P>(path: P, size: usize)
where
    P: AsRef<std::path::Path>,
{
    if path.as_ref().exists() {
        return;
    }

    let mut buffer = [0; 1024];
    let mut remaining_size = size;

    let f = File::create(path).unwrap();
    let mut writer = BufWriter::new(f);

    let mut rng = rand::rng();

    while remaining_size > 0 {
        let to_write = cmp::min(remaining_size, buffer.len());
        let buffer = &mut buffer[..to_write];
        rng.fill(buffer);
        writer.write_all(buffer).unwrap();
        remaining_size -= to_write;
    }
}

#[test]
fn test_file_transfer() -> Result<()> {
    common::setup();
    let (cp_bus, pd_bus) = MemoryChannel::new();
    let pd = PdDevice::new(Box::new(pd_bus))?;
    let cp = CpDevice::new(Box::new(cp_bus))?;

    create_random_file("/tmp/ftx_test.in", 50 * 1024);

    thread::sleep(Duration::from_secs(2));

    let mut fm = OsdpFileManager::new();
    fm.register_file(1, "/tmp/ftx_test.in");

    cp.get_device().register_file_ops(0, Box::new(fm))?;

    let mut fm = OsdpFileManager::new();
    fm.register_file(1, "/tmp/ftx_test.out");

    pd.get_device().register_file_ops(Box::new(fm))?;

    let command = OsdpCommand::FileTx(OsdpCommandFileTx {id: 1, flags: 0});
    cp.get_device().send_command(0, command.clone())?;

    assert_eq!(
        pd.receiver.recv().unwrap(),
        command,
        "PD file tx command callback verification failed!"
    );

    loop {
        let (size, offset) = pd.get_device().file_transfer_status()?;
        log::info!("File TX in progress: size:{size} offset:{offset}");
        if size == offset {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }

    assert_eq!(
        sha256::digest(std::fs::read("/tmp/ftx_test.in").unwrap()),
        sha256::digest(std::fs::read("/tmp/ftx_test.out").unwrap()),
        "Transferred file hash mismatch!"
    );
    Ok(())
}
