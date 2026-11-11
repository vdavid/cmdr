//! A fixed table of address ranges a signal handler can read without a lock.
//!
//! macOS fills one with every loaded image's extent, from dyld's add-image callback in normal
//! context, and [`recover`](super::recover) asks it whether a fault address is an image's.
//! Append-only, so a reader never sees a slot reused under it: an unloaded image's slot is zeroed
//! and stays dead. A table that ran out of slots says so through [`RangeTable::is_complete`],
//! and the handler then recovers nothing, since an image it couldn't record would read as fair
//! game.

#![cfg_attr(
    any(target_os = "linux", target_os = "windows"),
    allow(dead_code, reason = "only macOS tracks image ranges")
)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// Up to `N` `[start, end)` ranges.
pub(super) struct RangeTable<const N: usize> {
    starts: [AtomicU64; N],
    ends: [AtomicU64; N],
    /// How many slots are published. A slot's bounds are stored before `len` moves past it.
    len: AtomicUsize,
    /// A range arrived with no slot left for it.
    overflowed: AtomicBool,
    /// Serializes the writers. Never taken by a reader.
    writer: Mutex<()>,
}

impl<const N: usize> RangeTable<N> {
    pub(super) const fn new() -> Self {
        Self {
            starts: [const { AtomicU64::new(0) }; N],
            ends: [const { AtomicU64::new(0) }; N],
            len: AtomicUsize::new(0),
            overflowed: AtomicBool::new(false),
            writer: Mutex::new(()),
        }
    }

    /// Records `[start, end)`. Normal context only.
    pub(super) fn add(&self, start: u64, end: u64) {
        let _writer = self.writer.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let slot = self.len.load(Ordering::Acquire);
        if slot >= N {
            self.overflowed.store(true, Ordering::SeqCst);
            return;
        }
        self.starts[slot].store(start, Ordering::SeqCst);
        self.ends[slot].store(end, Ordering::SeqCst);
        self.len.store(slot + 1, Ordering::Release);
    }

    /// Forgets the range `[start, end)`, if it's recorded. Normal context only.
    pub(super) fn remove(&self, start: u64, end: u64) {
        let _writer = self.writer.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let len = self.len.load(Ordering::Acquire);
        for slot in 0..len {
            if self.starts[slot].load(Ordering::SeqCst) == start && self.ends[slot].load(Ordering::SeqCst) == end {
                // `end` first: a reader between the two stores sees `[start, 0)`, which holds
                // nothing, rather than a range stretched down to zero.
                self.ends[slot].store(0, Ordering::SeqCst);
                self.starts[slot].store(0, Ordering::SeqCst);
                return;
            }
        }
    }

    /// Whether a recorded range holds `address`. Async-signal-safe: atomic loads only.
    pub(super) fn contains(&self, address: u64) -> bool {
        let len = self.len.load(Ordering::Acquire).min(N);
        (0..len).any(|slot| {
            let start = self.starts[slot].load(Ordering::SeqCst);
            let end = self.ends[slot].load(Ordering::SeqCst);
            start <= address && address < end
        })
    }

    /// Whether every range ever added fit. When one didn't, an image may be unrecorded.
    pub(super) fn is_complete(&self) -> bool {
        !self.overflowed.load(Ordering::SeqCst)
    }

    /// Marks the table untrustworthy, for an image whose extent couldn't be read.
    pub(super) fn give_up(&self) {
        self.overflowed.store(true, Ordering::SeqCst);
    }
}
