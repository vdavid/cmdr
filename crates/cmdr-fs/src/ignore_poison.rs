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
//! Recovering never masks the original panic, and doesn't make it wait for a relaunch to
//! be heard. The app's panic hook writes the crash file that a *fatal* panic is reported
//! from at the next launch, and hands a *survived* panic (the case this policy creates) to
//! a courier thread that reports it in the same session, gated on the error-report opt-in.
//! Recovering only suppresses the *second*, app-killing panic at the innocent next
//! acquirer. Both halves live in `apps/desktop/src-tauri/src/crash_reporter/`.
//!
//! ## The rule (enforced by the `lock-poison` checker)
//!
//! A failed acquisition has exactly three sanctioned outcomes. Anything else
//! substitutes a default value out of thin air, which is worse than both.
//!
//! 1. **Recover**, the default for value-store locks: `lock_ignore_poison()` /
//!    `read_ignore_poison()` / `write_ignore_poison()`. This is the overwhelmingly
//!    common case.
//! 2. **Abort**, only when the lock guards a real cross-field invariant, and say so:
//!    `.lock().expect("<lock name> poisoned: <the invariant that makes recovery
//!    unsafe>")`. The message MUST contain "poison" so the deliberate choice is
//!    visible and machine-checkable.
//! 3. **Propagate**, handing the caller an `Err` to decide on.
//!
//! Two things are banned in non-test Rust code anywhere in the workspace, and the
//! `lock-poison` check (`scripts/check/checks/`) catches both:
//!
//! - **A bare `.lock().unwrap()` / `.read().unwrap()` / `.write().unwrap()`**
//!   records no intent, so a reader can't tell a considered abort from a
//!   thoughtless one. Error-level; pick form 1, 2, or 3.
//! - **Silently discarding the failure** — `if let Ok(g) = m.lock()` with no
//!   `else`, a `match` arm that returns on `Err(_)`, `let Ok(g) = m.lock() else
//!   { return }`, `.lock().ok()`, `.lock().map(…).unwrap_or_default()`. These READ
//!   as handled while doing something worse than panicking: the block is skipped
//!   with no log line and no recovery, so a watcher stops watching or a list
//!   reaches the user empty while the data behind it is intact. Warn-only against a
//!   per-file ratchet, since the tree still carries a pile of them.
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

/// Poison-ignoring `lock()` for [`Mutex`].
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
    /// Takes the read guard, ignoring poison.
    fn read_ignore_poison(&self) -> RwLockReadGuard<'_, T>;
    /// Takes the write guard, ignoring poison.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    /// Panics while holding `lock`, leaving it poisoned.
    fn poison<T>(lock: &Mutex<T>) {
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let _held = lock.lock().unwrap();
            panic!("poisoning the mutex on purpose");
        }));
        assert!(panicked.is_err());
    }

    #[test]
    fn a_poisoned_mutex_still_hands_over_its_data() {
        let lock = Mutex::new(vec![1, 2, 3]);
        poison(&lock);
        assert!(lock.is_poisoned());

        // The whole point: the value under a poisoned lock is intact, and the
        // next acquirer gets it instead of a second, app-killing panic.
        assert_eq!(*lock.lock_ignore_poison(), vec![1, 2, 3]);
        lock.lock_ignore_poison().push(4);
        assert_eq!(*lock.lock_ignore_poison(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn a_poisoned_rwlock_still_reads_and_writes() {
        let lock = RwLock::new(String::from("before"));
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let _held = lock.write().unwrap();
            panic!("poisoning the rwlock on purpose");
        }));
        assert!(panicked.is_err());
        assert!(lock.is_poisoned());

        assert_eq!(*lock.read_ignore_poison(), "before");
        *lock.write_ignore_poison() = String::from("after");
        assert_eq!(*lock.read_ignore_poison(), "after");
    }

    #[test]
    fn an_unpoisoned_lock_behaves_exactly_as_lock_does() {
        let lock = Mutex::new(7);
        *lock.lock_ignore_poison() += 1;
        assert_eq!(*lock.lock().unwrap(), 8);
    }
}
