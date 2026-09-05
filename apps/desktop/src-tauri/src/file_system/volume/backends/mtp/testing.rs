//! What a test outside this backend needs from inside it.
//!
//! Gated on `any(test, feature = "testing")`, ❌ never `cfg(test)` alone:
//! `cfg(test)` is set only for a crate's OWN test target, so a consumer's test
//! build would see these vanish and the production branch run inside their suite.
//!
//! Both instruments here are process-wide statics, which is fine for what they
//! do and would not be for anything else. Every read test wants a small window
//! and none of them asserts the production default, so a value one test sets
//! can't break another; the call counter is only ever read between an explicit
//! reset and the assertion that follows it.
//!
//! ❌ Don't grow this into a way to reach the backend's state. It hands out two
//! numbers and takes one, and that shape is what keeps an app-side test from
//! quietly depending on the backend's internals.

// The three `pub` functions below are the module's whole point, and they're read
// from test modules that a narrower build doesn't compile. While this backend
// still lives inside the app crate, `file_system::` is private, so `pub` here
// isn't reachable from outside and `deny(unused)` calls them dead.
#![allow(dead_code, reason = "read from test modules across a still-crate-private path")]

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Bytes per read window, or 0 for the production default.
static READ_WINDOW_OVERRIDE: AtomicU32 = AtomicU32::new(0);

/// How many `MtpVolume::list_directory` calls have happened since the last reset.
static LIST_DIRECTORY_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Shrinks the bounded read window to `window` bytes, so a small fixture file
/// spans several windows; 0 restores the production
/// [`MTP_READ_WINDOW`](crate::mtp::connection::MTP_READ_WINDOW).
///
/// Set it back to 0 when the test is done. A leftover override doesn't break a
/// later test's assertions, but it does make its reads needlessly chatty.
pub fn set_read_window(window: u32) {
    READ_WINDOW_OVERRIDE.store(window, Ordering::Relaxed);
}

/// The override, or 0 when nothing set one.
pub(super) fn read_window_override() -> u32 {
    READ_WINDOW_OVERRIDE.load(Ordering::Relaxed)
}

/// How many times `MtpVolume::list_directory` has been called since
/// [`reset_list_directory_call_count`].
///
/// The instrument the app's fresh-listing oracle is asserted with: an oracle
/// that decided a directory was covered must have issued ZERO of these, and no
/// wrapper `Volume` can see the call, because the scan reaches
/// `MtpVolume::list_directory` by static dispatch.
pub fn list_directory_call_count() -> usize {
    LIST_DIRECTORY_CALL_COUNT.load(Ordering::Relaxed)
}

/// Starts the count over. Call it immediately before the run under assertion.
pub fn reset_list_directory_call_count() {
    LIST_DIRECTORY_CALL_COUNT.store(0, Ordering::Relaxed);
}

/// One more `MtpVolume::list_directory` call.
pub(super) fn bump_list_directory_call_count() {
    LIST_DIRECTORY_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
}
