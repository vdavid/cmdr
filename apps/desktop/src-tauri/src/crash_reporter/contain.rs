//! A scoped exemption from crash reporting, for a parser that panics on untrusted input.
//!
//! The panic hook reports EVERY panic, including one a blocking thread catches with
//! `catch_unwind`: a crash file, the survival watchdog, and the in-session courier. That
//! is the right default for our own code, where a panic is a bug. It is the wrong one for
//! `pdf-extract`, whose parser carries ~100 `unwrap` / `expect` / `panic!` sites and meets
//! arbitrary user files: without this seam, one malformed PDF is one crash report.
//!
//! [`contain_panics`] marks the calling thread for the extent of one closure; the hook
//! reads the mark first ([`panic_is_contained`]) and, when set, logs one warning and stops.
//! The mark is a thread-local, so a panic on any OTHER thread during the closure is still
//! reported in full, and reading it cannot panic, which the hook requires of everything in
//! it.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

thread_local! {
    /// `true` while a [`contain_panics`] closure runs on this thread.
    static CONTAINED: Cell<bool> = const { Cell::new(false) };
}

/// Run `f`, turning a panic inside it into `None` and keeping it out of crash reporting.
///
/// Wrap ONLY the foreign parser calls, never our own shapers around them: a panic in our
/// code is a bug and has to keep reporting. The mark is restored (not just cleared) on the
/// way out, so a nested call can't unmark its caller.
pub fn contain_panics<T>(f: impl FnOnce() -> T) -> Option<T> {
    let outer = CONTAINED.replace(true);
    let outcome = catch_unwind(AssertUnwindSafe(f));
    CONTAINED.set(outer);
    outcome.ok()
}

/// Whether the panic the hook is looking at happened inside a [`contain_panics`] closure
/// on this thread.
pub(super) fn panic_is_contained() -> bool {
    CONTAINED.get()
}
