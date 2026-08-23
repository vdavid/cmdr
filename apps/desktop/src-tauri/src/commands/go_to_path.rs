//! IPC commands for the "Go to path" dialog.
//!
//! Thin pass-throughs over the `go_to_path` module. `resolve_go_to_path` touches
//! the filesystem (`metadata` / `exists`), so it runs on the blocking pool with a
//! timeout: a hung mount must never freeze IPC.

use tokio::time::Duration;

use crate::commands::util::{DeadlineError, blocking_typed_result_with_timeout};
use crate::go_to_path::history::{MAX_RECENTS, RECENT_PATHS, RecentPathEntry};
use crate::go_to_path::{self, GoToPathResolution};

/// 2s matches the read timeout other filesystem-touching commands use.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

/// Resolves a typed input against the focused pane's `base_dir`. Serves the
/// live as-you-type warning, the actual jump, and the clipboard-prefill check.
///
/// `go_to_path::resolve` always answers (an unreachable path is a `GoToPathResolution`
/// variant, not an error), so a missed deadline is the only thing this can report.
#[tauri::command]
#[specta::specta]
pub async fn resolve_go_to_path(input: String, base_dir: String) -> Result<GoToPathResolution, DeadlineError> {
    blocking_typed_result_with_timeout(
        RESOLVE_TIMEOUT,
        || DeadlineError::TimedOut,
        |detail| DeadlineError::Unexpected { detail },
        move || Ok(go_to_path::resolve(&input, &base_dir)),
    )
    .await
}

/// Reads the persisted recent-path entries (newest first).
#[tauri::command]
#[specta::specta]
pub fn get_recent_paths() -> Vec<RecentPathEntry> {
    RECENT_PATHS.entries(None)
}

/// Adds a recent-path entry. Dedupes by resolved path, moves the match to the
/// top, and trims to the fixed cap.
#[tauri::command]
#[specta::specta]
pub fn add_recent_path(app: tauri::AppHandle, entry: RecentPathEntry) -> Result<(), String> {
    RECENT_PATHS.add(&app, entry, MAX_RECENTS);
    Ok(())
}

/// Removes a recent-path entry by id. No-op when the id isn't present.
#[tauri::command]
#[specta::specta]
pub fn remove_recent_path(app: tauri::AppHandle, id: String) -> Result<(), String> {
    RECENT_PATHS.remove(&app, &id);
    Ok(())
}

/// Clears every recent-path entry.
#[tauri::command]
#[specta::specta]
pub fn clear_recent_paths(app: tauri::AppHandle) -> Result<(), String> {
    RECENT_PATHS.clear(&app);
    Ok(())
}
