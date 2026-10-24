//
// Copyright (c) 2023-2026 Siddharth Chandrasekaran <sidcha@osdp.dev>
//
// SPDX-License-Identifier: Apache-2.0

use std::io::Read;

use libosdp::ChannelError;
use ringbuf::{
    traits::{Observer, Producer, Split},
    HeapCons, HeapProd, HeapRb,
};

/// An in-memory OSDP channel suitable for testing
pub struct MemoryChannel {
    id: i32,
    sender: HeapProd<u8>,
    receiver: HeapCons<u8>,
}

impl std::fmt::Debug for MemoryChannel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MemoryChannel")
            .field("id", &self.id)
            .finish()
    }
}

impl MemoryChannel {
    /// Create a new MemoryChannel
    pub fn new() -> (Self, Self) {
        let rb1 = HeapRb::<u8>::new(1024);
        let (prod1, cons1) = rb1.split();
        let rb2 = HeapRb::<u8>::new(1024);
        let (prod2, cons2) = rb2.split();
        (
            Self {
                id: 0,
                sender: prod1,
                receiver: cons2,
            },
            Self {
                id: 1,
                sender: prod2,
                receiver: cons1,
            },
        )
    }
}

impl libosdp::Channel for MemoryChannel {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, libosdp::ChannelError> {
        self.receiver.read(buf).map_err(ChannelError::from)
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), libosdp::ChannelError> {
        if self.sender.vacant_len() < buf.len() {
            return Err(ChannelError::WouldBlock);
        }
        self.sender.push_slice(buf);
        Ok(())
    }
}
