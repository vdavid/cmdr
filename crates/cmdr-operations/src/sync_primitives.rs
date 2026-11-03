//! The locks, channels, atomics, and threads that the crate's raced code is
//! built from: std's in every build, Shuttle's under the `shuttle` feature, so
//! the interleaving tests (`*_shuttle_tests.rs`) explore that code's OWN
//! schedules rather than a model of them. Today that's the in-flight ledger
//! (`in_flight_temps.rs` and its log writer) and the stop-and-pause machine
//! (`operation_intent.rs`).
//!
//! A re-export, so the seam is gone at compile time: a shipped build names
//! exactly the std types it always did. ❌ Anything those modules' threads
//! share through a lock, a queue, or an atomic goes through here, never
//! `std::sync` directly, or Shuttle can't switch threads there and the
//! schedules it reports stop covering it. `Arc` stays std's: it orders nothing.

#[cfg(not(feature = "shuttle"))]
pub(crate) use std::sync::atomic::{AtomicBool, AtomicU8};
#[cfg(not(feature = "shuttle"))]
pub(crate) use std::sync::{Condvar, Mutex, MutexGuard, Once, mpsc};
#[cfg(not(feature = "shuttle"))]
pub(crate) use std::thread;

#[cfg(feature = "shuttle")]
pub(crate) use shuttle::sync::atomic::{AtomicBool, AtomicU8};
#[cfg(feature = "shuttle")]
pub(crate) use shuttle::sync::{Condvar, Mutex, MutexGuard, Once, mpsc};
#[cfg(feature = "shuttle")]
pub(crate) use shuttle::thread;

/// Locks `mutex`, recovering a poisoned one: `IgnorePoison::lock_ignore_poison`
/// for a lock that may be Shuttle's, which that trait can't reach.
pub(crate) fn lock_ignore_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
