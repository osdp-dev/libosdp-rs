//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Streams over the queues that a context fills during refresh.

use crate::{ControlPanel, CpEvent, Notification, PdIndex, PeripheralDevice};
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use futures_core::Stream;

/// The events and notifications a [`ControlPanel`] queues during refresh,
/// from [`ControlPanel::events`]. It never ends.
///
/// Only the most recently polled stream of a context is woken; keep one.
#[derive(Debug)]
pub struct EventStream<'a> {
    cp: &'a ControlPanel,
}

impl<'a> EventStream<'a> {
    pub(crate) fn new(cp: &'a ControlPanel) -> Self {
        Self { cp }
    }
}

impl Stream for EventStream<'_> {
    type Item = (PdIndex, CpEvent);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.cp.next_event() {
            Some(item) => Poll::Ready(Some(item)),
            None => {
                self.cp.park_event_waker(cx.waker());
                Poll::Pending
            }
        }
    }
}

/// The notifications a [`PeripheralDevice`] queues during refresh, from
/// [`PeripheralDevice::notifications`]. It never ends.
///
/// Only the most recently polled stream of a context is woken; keep one.
#[derive(Debug)]
pub struct NotificationStream<'a> {
    pd: &'a PeripheralDevice,
}

impl<'a> NotificationStream<'a> {
    pub(crate) fn new(pd: &'a PeripheralDevice) -> Self {
        Self { pd }
    }
}

impl Stream for NotificationStream<'_> {
    type Item = Notification;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.pd.next_notification() {
            Some(item) => Poll::Ready(Some(item)),
            None => {
                self.pd.park_notification_waker(cx.waker());
                Poll::Pending
            }
        }
    }
}
