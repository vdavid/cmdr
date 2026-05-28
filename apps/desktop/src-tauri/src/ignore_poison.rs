//! Extension trait to ignore mutex / rwlock poisoning.
//!
//! Wraps `Mutex` / `RwLock` accessors so simple value stores can swap the
//! boilerplate `.lock().unwrap_or_else(|e| e.into_inner())` (and
//! `read()` / `write()` equivalents) for a readable
//! `.lock_ignore_poison()` / `.read_ignore_poison()` / `.write_ignore_poison()`.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

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

/// Sibling for `RwLock`. Same simple-value-store contract: a panic in another
/// thread doesn't invalidate the data, so reading the previous value is
/// strictly better than a cascading panic at the next lock site.
pub trait RwLockIgnorePoison<T> {
    fn read_ignore_poison(&self) -> RwLockReadGuard<'_, T>;
    fn write_ignore_poison(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockIgnorePoison<T> for RwLock<T> {
    fn read_ignore_poison(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|e| e.into_inner())
    }
    fn write_ignore_poison(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|e| e.into_inner())
    }
}
