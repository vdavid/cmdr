//! Extension trait to ignore mutex poisoning.
//!
//! All 75 uses of `.lock().unwrap_or_else(|e| e.into_inner())` in the codebase store simple
//! values where poison is irrelevant. This trait replaces the boilerplate with a readable
//! `.lock_ignore_poison()` call.

use std::sync::{Mutex, MutexGuard};

pub trait IgnorePoison<T> {
    /// Locks the mutex, ignoring poison. Use this for simple value stores where
    /// a panic in another thread doesn't invalidate the data.
    fn lock_ignore_poison(&self) -> MutexGuard<'_, T>;
}

impl<T> IgnorePoison<T> for Mutex<T> {
    fn lock_ignore_poison(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}
