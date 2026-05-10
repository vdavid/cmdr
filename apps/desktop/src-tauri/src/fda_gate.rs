//! Full Disk Access gate.
//!
//! At first launch on macOS, Cmdr shows an in-app modal that walks the user
//! through granting Full Disk Access (FDA) before the indexer scans `/`.
//! The same gate applies more broadly: any launch-time code that touches
//! TCC-protected paths (Downloads, Documents, Desktop, ...) or NSWorkspace
//! icon/LaunchServices APIs on those paths must skip work while the FDA
//! decision is pending. Otherwise macOS stacks several native permission
//! popups (MediaLibrary, AppData, Desktop, Documents, Downloads, ...) on
//! top of our in-app modal — exactly the onboarding-flood UX we want to
//! avoid.
//!
//! The gate has two pieces:
//!
//! 1. `is_fda_pending(fda_choice, os_fda_granted)` — pure decision used at
//!    startup and by tests. Pending iff the user hasn't decided AND the OS
//!    reports FDA isn't granted.
//! 2. A process-global `AtomicBool` set once at startup (and cleared when
//!    the user denies FDA in-session). Read by code that runs after startup
//!    via `is_fda_pending_runtime()`.
//!
//! On non-macOS platforms FDA doesn't exist; the runtime gate is always
//! `false` (open) so cross-platform callers get the right behaviour without
//! cfg-guards at every site.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::settings::FullDiskAccessChoice;

static FDA_PENDING: OnceLock<AtomicBool> = OnceLock::new();

/// Pure decision: is the FDA decision still pending at this moment?
///
/// Returns `true` only when the user hasn't decided AND the OS confirms FDA
/// isn't currently granted. If the OS check returns `true` we know the
/// per-folder TCC services are subsumed by FDA, so it's safe to access
/// protected paths even if no in-app choice has been recorded yet.
pub fn is_fda_pending(fda_choice: FullDiskAccessChoice, os_fda_granted: bool) -> bool {
    fda_choice == FullDiskAccessChoice::NotAskedYet && !os_fda_granted
}

/// Set the runtime gate. Call once at startup with the result of
/// `is_fda_pending(...)`, and again with `false` after the user makes a
/// choice in-session (deny path — the allow path requires a restart and
/// re-enters startup).
pub fn set_fda_pending(pending: bool) {
    FDA_PENDING
        .get_or_init(|| AtomicBool::new(pending))
        .store(pending, Ordering::Release);
}

/// Read the runtime gate. Returns `false` until `set_fda_pending` has been
/// called — safe default for tests and any non-macOS build that never sets
/// it.
pub fn is_fda_pending_runtime() -> bool {
    FDA_PENDING.get().is_some_and(|f| f.load(Ordering::Acquire))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_only_when_not_asked_and_os_denies() {
        assert!(is_fda_pending(FullDiskAccessChoice::NotAskedYet, false));
        assert!(!is_fda_pending(FullDiskAccessChoice::NotAskedYet, true));
        assert!(!is_fda_pending(FullDiskAccessChoice::Allow, false));
        assert!(!is_fda_pending(FullDiskAccessChoice::Allow, true));
        assert!(!is_fda_pending(FullDiskAccessChoice::Deny, false));
        assert!(!is_fda_pending(FullDiskAccessChoice::Deny, true));
    }
}
