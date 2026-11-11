//! Windows: the system-look observers, which follow nothing yet (accent color, text scale, and
//! high contrast arrive with their Windows arms), so the frontend keeps the defaults it starts
//! with, as every other OS does before its first event.

use tauri::AppHandle;

/// Starts every Windows installer.
pub(crate) fn install(app: &AppHandle) {
    crate::system_look::observe_changes(app.clone());
}
