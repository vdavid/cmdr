//! Test seam: holds a safe-overwrite open between the rename that sets the
//! user's original ASIDE and the rename that replaces it.
//!
//! ❗ That window is the whole reason the ledger records asides: a crash or a
//! pulled drive inside it leaves the only copy of the file wearing a scratch
//! name. Nothing else can stop an overwrite there — the two renames are
//! back-to-back syscalls.
//!
//! One slot per operation, on its scope (test builds only): a test arms the
//! park on the operation it drives, and the overwrite runs inside
//! `spawn_blocking` on another thread, which reaches the same slot through the
//! same state. One park at a time; the guard disarms on drop, and a parked
//! overwrite gives up on its own after [`PARK_CAP`] so a test that dies can't
//! wedge the suite.

// Only the macOS real-detach lane arms the park; elsewhere the engine's own hook is
// all that's read, and `-D unused` is fatal on the Linux build.
#![cfg_attr(
    not(target_os = "macos"),
    expect(dead_code, reason = "only the macOS real-detach lane arms the park")
)]

use crate::rt::Instant;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How long a parked overwrite waits for its release before carrying on.
pub(crate) const PARK_CAP: Duration = Duration::from_secs(30);

/// One operation's park: armed by its test, taken by its next safe-overwrite
/// that sets an original aside.
// DEFAULT-OK: unarmed, unparked, and unreleased is where every operation
// starts; nothing parks until a test arms it.
#[derive(Default)]
pub(crate) struct AsideParkSlot {
    armed: AtomicBool,
    is_parked: AtomicBool,
    released: AtomicBool,
}

/// An armed park. Disarms on drop, released or not.
pub(crate) struct AsidePark<'a> {
    slot: &'a AsideParkSlot,
}

impl AsideParkSlot {
    /// Parks this operation's next safe-overwrite that has set an original aside.
    pub(crate) fn park_next_overwrite(&self) -> AsidePark<'_> {
        self.is_parked.store(false, Ordering::SeqCst);
        self.released.store(false, Ordering::SeqCst);
        self.armed.store(true, Ordering::SeqCst);
        AsidePark { slot: self }
    }

    pub(super) fn park_if_armed(&self) {
        // One park per arming: the overwrite that got here owns it.
        if !self.armed.swap(false, Ordering::SeqCst) {
            return;
        }
        self.is_parked.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + PARK_CAP;
        while !self.released.load(Ordering::SeqCst) && Instant::now() < deadline {
            // allowed-test-sleep: this IS the park. Holding the overwrite still
            // between the two renames is the whole point, and `PARK_CAP` above
            // keeps a dead test from wedging the suite.
            std::thread::sleep(Duration::from_millis(10));
        }
        self.is_parked.store(false, Ordering::SeqCst);
    }
}

impl AsidePark<'_> {
    /// Blocks until an overwrite is parked with its aside on disk.
    /// `false` if nothing parked within `timeout`.
    pub(crate) fn wait_until_parked(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.slot.is_parked.load(Ordering::SeqCst) {
                return true;
            }
            // allowed-test-sleep: polling a flag another THREAD sets is the
            // subject. The park is what this caller is waiting to observe, so
            // there's no condition `wait_until` could ask about instead.
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// Lets the parked overwrite carry on.
    pub(crate) fn release(&self) {
        self.slot.released.store(true, Ordering::SeqCst);
    }
}

impl Drop for AsidePark<'_> {
    fn drop(&mut self) {
        self.slot.armed.store(false, Ordering::SeqCst);
        self.slot.released.store(true, Ordering::SeqCst);
        self.slot.is_parked.store(false, Ordering::SeqCst);
    }
}
