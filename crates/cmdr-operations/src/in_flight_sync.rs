//! The locks, channels, and threads the in-flight ledger is built from: std's
//! in every build, Shuttle's under the `shuttle` feature, so the interleaving
//! tests (`in_flight_shuttle_tests.rs`) explore the ledger's OWN schedules
//! rather than a model of them.
//!
//! A re-export, so the seam is gone at compile time: a shipped build names
//! exactly the std types it always did. ❌ Anything the ledger's threads share
//! through a lock or a queue goes through here, never `std::sync` directly, or
//! Shuttle can't switch threads there and the schedules it reports stop
//! covering it. `Arc` stays std's: it orders nothing.

#[cfg(not(feature = "shuttle"))]
pub(super) use std::sync::{Mutex, MutexGuard, Once, mpsc};
#[cfg(not(feature = "shuttle"))]
pub(super) use std::thread;

#[cfg(feature = "shuttle")]
pub(super) use shuttle::sync::{Mutex, MutexGuard, Once, mpsc};
#[cfg(feature = "shuttle")]
pub(super) use shuttle::thread;

/// Locks `mutex`, recovering a poisoned one: `IgnorePoison::lock_ignore_poison`
/// for a lock that may be Shuttle's, which that trait can't reach.
pub(super) fn lock_ignore_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
