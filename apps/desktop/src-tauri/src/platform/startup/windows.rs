//! Windows: nothing yet. The volume watcher and the system-look observers (accent color, text
//! scale, high contrast) arrive with their Windows arms; until then the frontend keeps the
//! defaults it starts with, as every other OS does before its first event.

use tauri::AppHandle;

/// Starts every Windows installer: none yet.
pub(crate) fn install(_app: &AppHandle) {}
