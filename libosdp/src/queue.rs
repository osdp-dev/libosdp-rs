//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! A bounded queue that LibOSDP's callbacks fill during a refresh and the
//! application drains afterwards, so that no application code runs inside
//! those callbacks.

use alloc::collections::VecDeque;
use core::{
    cell::{Cell, RefCell},
    num::NonZeroUsize,
};

pub(crate) struct DrainQueue<T> {
    items: RefCell<VecDeque<T>>,
    capacity: usize,
    dropped: Cell<usize>,
}

impl<T> DrainQueue<T> {
    pub(crate) fn new(capacity: NonZeroUsize) -> Self {
        Self {
            items: RefCell::new(VecDeque::new()),
            capacity: capacity.get(),
            dropped: Cell::new(0),
        }
    }

    /// Queue `item`, or count it as dropped when the queue is full.
    pub(crate) fn push(&self, item: T) {
        let mut items = self.items.borrow_mut();
        if items.len() < self.capacity {
            items.push_back(item);
        } else {
            self.dropped.set(self.dropped.get() + 1);
        }
    }

    pub(crate) fn pop(&self) -> Option<T> {
        self.items.borrow_mut().pop_front()
    }

    #[cfg(feature = "async")]
    pub(crate) fn is_empty(&self) -> bool {
        self.items.borrow().is_empty()
    }

    pub(crate) fn dropped(&self) -> usize {
        self.dropped.get()
    }
}

#[cfg(test)]
mod tests {
    use super::DrainQueue;
    use core::num::NonZeroUsize;

    #[test]
    fn full_queue_drops_the_newest_and_counts_it() {
        let queue = DrainQueue::new(NonZeroUsize::new(2).unwrap());
        queue.push(1);
        queue.push(2);
        queue.push(3);
        assert_eq!(queue.dropped(), 1);
        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn huge_capacity_allocates_only_for_queued_items() {
        let queue = DrainQueue::new(NonZeroUsize::MAX);
        queue.push(7);
        assert_eq!(queue.pop(), Some(7));
        assert_eq!(queue.dropped(), 0);
    }
}
