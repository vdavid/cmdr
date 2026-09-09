//! IPC commands over the macOS Dock.
//!
//! Pass-throughs: every decision about whether Cmdr may be offered a Dock tile, and what tile to
//! write, lives in `../dock/`. See `../dock/CLAUDE.md`.

use tokio::time::Duration;

use crate::commands::util::{blocking_typed_result_with_timeout, blocking_with_timeout};
use crate::dock::{DockPinBlocker, DockPinFailure, DockPinState};

/// Reading the Dock's preferences goes through `cfprefsd` over XPC, which can wait on a busy
/// daemon, so it carries the standard 2 s read deadline.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// The pin writes preferences and then waits for the Dock to acknowledge a restart, so it gets the
/// 5 s write tier rather than the read one.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether we may offer to put Cmdr in the Dock, and whether it's already there.
///
/// A slow or unreachable `cfprefsd` answers `PreferencesUnreadable`, which keeps the nudge silent
/// rather than firing on a guess.
#[tauri::command]
#[specta::specta]
pub async fn get_dock_pin_state() -> DockPinState {
    blocking_with_timeout(
        READ_TIMEOUT,
        DockPinState::Unavailable {
            reason: DockPinBlocker::PreferencesUnreadable,
        },
        crate::dock::pin_state,
    )
    .await
}

/// Adds Cmdr to the Dock as the first tile and restarts the Dock so it appears.
///
/// Only ever called after the person says yes to the nudge. It re-checks everything
/// [`get_dock_pin_state`] checked, so a stale answer can't turn into a write we'd never have
/// offered.
#[tauri::command]
#[specta::specta]
pub async fn add_cmdr_to_dock() -> Result<(), DockPinFailure> {
    blocking_typed_result_with_timeout(
        WRITE_TIMEOUT,
        || DockPinFailure::TimedOut,
        |e| {
            log::warn!(target: "dock", "The Dock pin task didn't finish: {e}");
            DockPinFailure::TimedOut
        },
        crate::dock::pin_cmdr,
    )
    .await
}
