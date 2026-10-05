//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Context methods take `&self`, so application code that runs inside a
//! LibOSDP callback -- a channel, file op or command handler -- can reach its
//! own context again, through a thread local for instance. LibOSDP must not
//! be refreshed from inside its own refresh, and an allocation must not be
//! freed while one of its methods is running, so a context marks itself busy
//! around every call that can run application code: `refresh`,
//! `register_file_ops`, and `send_command`, which runs `FileOps::open`
//! inline when it starts a file transfer. When the context is already busy,
//! `refresh` does nothing and the other two return `Error::Rejected`.

use core::cell::Cell;

pub(crate) struct Busy<'a>(&'a Cell<bool>);

impl<'a> Busy<'a> {
    /// `None` when the context is already busy, that is, when the caller is
    /// running inside one of the context's callbacks.
    pub(crate) fn enter(flag: &'a Cell<bool>) -> Option<Self> {
        if flag.replace(true) {
            None
        } else {
            Some(Self(flag))
        }
    }
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
