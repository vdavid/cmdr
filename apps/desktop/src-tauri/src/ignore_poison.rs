//! Extension trait to ignore mutex / rwlock poisoning, plus the project-wide
//! **lock-poison policy** every `std::sync::Mutex` / `RwLock` acquisition follows.
//!
//! ## Why this policy exists
//!
//! A `Mutex`/`RwLock` is *poisoned* when a thread panics while holding the guard.
//! The next acquirer then has two choices: propagate the poison as a panic
//! (`.lock().unwrap()` / `.expect(...)` — "abort") or take the data anyway
//! (`.lock_ignore_poison()` — "recover"). Which is correct is **a property of the
//! data the lock guards, not a matter of taste**:
//!
//! - **Recover** is correct — and strictly better than aborting — for a **simple
//!   value store**: a `Vec`, `Option`, counter, `Instant`, or cache where any
//!   single operation leaves the value well-formed. A panic mid-operation can't
//!   tear an invariant here; at worst one update is lost. Crashing the whole app
//!   over a lock whose data is fine violates "the app must feel rock solid" — and
//!   the realistic trigger is a panic in a *background* thread (an MTP poll, an SMB
//!   watcher) poisoning a *shared* lock, so the abort would land on the next
//!   *unrelated* user action.
//! - **Abort** is correct for the rare lock guarding a **multi-field invariant or a
//!   state machine briefly in an illegal intermediate state**, where reading after a
//!   panic could observe — and recovering would *propagate* — corrupt state. Here a
//!   loud crash beats silently acting on torn data.
//!
//! The original panic is never masked by recovering: the panic hook + crash
//! reporter still capture it. Recovering only suppresses the *second*, app-killing
//! panic at the innocent next acquirer.
//!
//! ## The rule (enforced by the `lock-poison` checker)
//!
//! 1. **Default to recover** for value-store locks: `lock_ignore_poison()` /
//!    `read_ignore_poison()` / `write_ignore_poison()`. This is the overwhelmingly
//!    common case.
//! 2. **Abort only when the lock guards a real cross-field invariant**, and say so:
//!    `.lock().expect("<lock name> poisoned: <the invariant that makes recovery
//!    unsafe>")`. The message MUST contain "poison" so the deliberate choice is
//!    visible and machine-checkable.
//! 3. **Bare `.lock().unwrap()` / `.read().unwrap()` / `.write().unwrap()` on a
//!    std lock is banned** in non-test `src-tauri` code — it records no intent, so a
//!    reader can't tell a considered abort from a thoughtless one. The `lock-poison`
//!    check (`scripts/check/checks/`) fails on it; pick form 1 or 2.
//!
//! The checker enforces *form* (a deliberate choice was recorded), not *choice*
//! (that the right form was picked for the data) — the latter is the author's
//! judgment, guided by the value-store-vs-invariant test above.
//!
//! ## Decision / Why (recover-by-default, not abort-by-default)
//!
//! A file manager's headline promise is responsiveness and never losing the user's
//! session to an unrelated fault. Abort-by-default inverts that: it converts every
//! poisoned value-store lock — data that is provably fine — into an app crash. The
//! invariant-guarded locks that genuinely warrant aborting are a small, namable
//! minority, so they carry the justification (the named `expect`) rather than the
//! safe-by-construction majority carrying the boilerplate.

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
