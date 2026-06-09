//
// Copyright (c) 2023-2024 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use anyhow::Context;
use configparser::ini::Ini;
use libosdp::{ControlPanelBuilder, OsdpFlag, PdCapability, PdId, PdInfoBuilder};
use rand::RngExt;
use std::{
    fmt::Write,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::unix_channel::UnixChannel;

type Result<T> = anyhow::Result<T, anyhow::Error>;

fn vec_to_array<T, const N: usize>(v: Vec<T>) -> [T; N] {
    v.try_into()
        .unwrap_or_else(|v: Vec<T>| panic!("Expected a Vec of length {} but it was {}", N, v.len()))
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyStore {
    store: PathBuf,
    pub key: [u8; 16],
}

impl KeyStore {
    pub fn create(store: PathBuf, key: &str) -> Result<Self> {
        let key = KeyStore::str_to_key(key)?;
        std::fs::write(&store, key)
            .expect("Unable to write to keystore");
        Ok(Self { store, key })
    }

    pub fn _new(store: PathBuf) -> Result<Self> {
        let key = KeyStore::_random_key();
        let key_str = KeyStore::key_to_str(&key);
        std::fs::write(&store, key_str)
            .expect("Unable to write to keystore");
        Ok(Self { store, key })
    }

    pub fn _random_key() -> [u8; 16] {
        let mut key = [0u8; 16];
        rand::rng().fill(&mut key);
        key
    }

    pub fn decode_hex(s: &str) -> anyhow::Result<Vec<u8>, std::num::ParseIntError> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }

    fn str_to_key(s: &str) -> Result<[u8; 16]> {
        let key = KeyStore::decode_hex(s)?;
        Ok(vec_to_array::<u8, 16>(key))
    }

    fn key_to_str(key: &[u8; 16]) -> String {
        let mut s = String::with_capacity(key.len() * 2);
        for b in key {
            write!(&mut s, "{:02x}", b).unwrap();
        }
        s
    }

    pub fn _load(&self) -> Result<[u8; 16]> {
        let s = std::fs::read_to_string(&self.store)
            .context(format!("keystore {} not found", self.store.display()))?;
        KeyStore::str_to_key(&s)
    }

    pub fn store(&mut self, key: [u8; 16]) -> Result<()> {
        std::fs::write(&self.store, KeyStore::key_to_str(&key))
            .expect("Unable to write to keystore");
        self.key = key;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PdData {
    pub name: String,
    channel: String,
    address: i32,
    pub key_store: KeyStore,
    flags: OsdpFlag,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CpConfig {
    pub runtime_dir: PathBuf,
    pub name: String,
    pd_data: Vec<PdData>,
    pub log_level: log::LevelFilter,
}

impl CpConfig {
    pub fn new(config: &Ini, runtime_dir: &Path) -> Result<Self> {
        let num_pd = config.getuint("default", "num_pd").unwrap().unwrap() as usize;
        let name = config.get("default", "name").unwrap();
        let runtime_dir = runtime_dir.to_owned();
        let mut pd_data = Vec::new();
        for pd in 0..num_pd {
            let section = format!("pd-{pd}");
            let key = &config.get(&section, "scbk").unwrap();
            pd_data.push(PdData {
                name: config.get(&section, "name").unwrap(),
                channel: config.get(&section, "channel").unwrap(),
                address: config.getuint(&section, "address").unwrap().unwrap() as i32,
                key_store: KeyStore::create(runtime_dir.join(format!("pd-{}-key.store", pd)), key)?,
                flags: OsdpFlag::empty(),
            });
        }
        let log_level = config.get("default", "log_level").unwrap();
        let log_level = match log_level.as_str() {
            "INFO" => log::LevelFilter::Info,
            "DEBUG" => log::LevelFilter::Debug,
            "WARN" => log::LevelFilter::Warn,
            "TRACE" => log::LevelFilter::Trace,
            _ => log::LevelFilter::Off,
        };
        Ok(Self {
            name,
            log_level,
            pd_data,
            runtime_dir,
        })
    }

    pub fn pd_info(&self) -> Result<ControlPanelBuilder> {
        let mut runtime_dir = self.runtime_dir.clone();
        runtime_dir.pop();
        let mut cp = ControlPanelBuilder::new();
        for d in self.pd_data.iter() {
            let parts: Vec<&str> = d.channel.split("::").collect();
            if parts[0] != "unix" {
                bail!("Only unix channel is supported for now")
            }
            let path = runtime_dir.join(format!("{}/{}.sock", d.name, parts[1]).as_str());
            let channel = UnixChannel::connect(&path).context("Unable to connect to PD channel")?;
            let pd_info = PdInfoBuilder::new()
                .name(&self.name)?
                .address(d.address)?
                .baud_rate(115200)?
                .flag(d.flags)
                .secure_channel_key(d.key_store.key);
            cp = cp.add_channel(Box::new(channel), vec![pd_info]);
        }
        Ok(cp)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PdConfig {
    pub runtime_dir: PathBuf,
    pub name: String,
    channel: String,
    address: i32,
    pub key_store: KeyStore,
    pd_id: PdId,
    pd_cap: Vec<PdCapability>,
    flags: OsdpFlag,
    pub log_level: log::LevelFilter,
}

impl PdConfig {
    pub fn new(config: &Ini, runtime_dir: &Path) -> Result<Self> {
        let vendor_code = config.getuint("pd_id", "vendor_code").unwrap().unwrap() as u32;
        let serial_number = config.getuint("pd_id", "serial_number").unwrap().unwrap() as u32;
        let firmware_version = config
            .getuint("pd_id", "firmware_version")
            .unwrap()
            .unwrap() as u32;
        let pd_id = PdId {
            version: config.getuint("pd_id", "version").unwrap().unwrap() as u8,
            model: config.getuint("pd_id", "model").unwrap().unwrap() as u8,
            vendor_code: (
                vendor_code as u8,
                (vendor_code >> 8) as u8,
                (vendor_code >> 16) as u8,
            ),
            serial_number: serial_number.to_le_bytes(),
            firmware_version: (
                firmware_version as u8,
                (firmware_version >> 8) as u8,
                (firmware_version >> 16) as u8,
            ),
        };
        let mut flags = OsdpFlag::empty();
        if let Some(val) = config.get("default", "flags") {
            let fl: Vec<&str> = val.split('|').collect();
            for f in fl {
                flags.set(OsdpFlag::from_str(f)?, true);
            }
        }
        let map = config.get_map().unwrap();
        let cap_map = map.get("capability").unwrap();
        let mut pd_cap = Vec::new();
        for (key, val) in cap_map {
            pd_cap.push(PdCapability::from_str(
                format!("{}:{}", key, val.as_deref().unwrap()).as_str(),
            )?);
        }
        let log_level = config.get("default", "log_level").unwrap();
        let log_level = match log_level.as_str() {
            "INFO" => log::LevelFilter::Info,
            "DEBUG" => log::LevelFilter::Debug,
            "WARN" => log::LevelFilter::Warn,
            "TRACE" => log::LevelFilter::Trace,
            _ => log::LevelFilter::Off,
        };
        let key = &config.get("default", "scbk").unwrap();
        let name = config.get("default", "name").unwrap();
        let runtime_dir = runtime_dir.to_owned();
        let key_store = KeyStore::create(runtime_dir.join("key.store"), key)?;
        Ok(Self {
            name,
            channel: config.get("default", "channel").unwrap(),
            address: config.getuint("default", "address").unwrap().unwrap() as i32,
            key_store,
            log_level,
            pd_id,
            pd_cap,
            flags,
            runtime_dir,
        })
    }

    pub fn pd_info(&self) -> Result<(Box<dyn libosdp::Channel>, PdInfoBuilder)> {
        let parts: Vec<&str> = self.channel.split("::").collect();
        if parts[0] != "unix" {
            bail!("Only unix channel is supported for now")
        }
        let path = self.runtime_dir.join(format!("{}.sock", parts[1]).as_str());
        let channel = UnixChannel::new(&path)?;
        let pd_info = PdInfoBuilder::new()
            .name(&self.name)?
            .address(self.address)?
            .baud_rate(115200)?
            .flag(self.flags)
            .capabilities(&self.pd_cap)
            .id(&self.pd_id)
            .secure_channel_key(self.key_store.key);
        Ok((Box::new(channel), pd_info))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DeviceConfig {
    CpConfig(CpConfig),
    PdConfig(PdConfig),
}

fn read_pid_from_file(file: PathBuf) -> Result<i32> {
    let pid = std::fs::read_to_string(file)?;
    let pid = pid.trim();
    let pid = pid.parse::<i32>()?;
    Ok(pid)
}

impl DeviceConfig {
    pub fn get_pid(&self) -> Result<i32> {
        match self {
            DeviceConfig::CpConfig(dev) => {
                let pid_file = dev.runtime_dir.join(format!("dev-{}.pid", dev.name));
                read_pid_from_file(pid_file)
            }
            DeviceConfig::PdConfig(dev) => {
                let pid_file = dev.runtime_dir.join(format!("dev-{}.pid", dev.name));
                read_pid_from_file(pid_file)
            }
        }
    }
}

impl DeviceConfig {
    pub fn new(cfg: &Path, runtime_dir: &Path) -> Result<Self> {
        let mut config = Ini::new_cs();
        if !cfg.exists() {
            bail!("Config {} does not exist!", cfg.display())
        }
        config.load(cfg).unwrap();

        let mut runtime_dir = runtime_dir.to_owned();
        let name = config.get("default", "name").unwrap();
        runtime_dir.push(&name);
        _ = std::fs::create_dir_all(&runtime_dir);

        let config = match config.get("default", "num_pd") {
            Some(_) => DeviceConfig::CpConfig(CpConfig::new(&config, &runtime_dir)?),
            None => DeviceConfig::PdConfig(PdConfig::new(&config, &runtime_dir)?),
        };
        Ok(config)
    }

    pub fn name(&self) -> &str {
        match self {
            DeviceConfig::CpConfig(c) => &c.name,
            DeviceConfig::PdConfig(c) => &c.name,
        }
    }
}
