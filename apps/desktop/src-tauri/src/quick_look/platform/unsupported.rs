//! The answers of an OS with no Quick Look panel: opening answers `false`, and the rest are
//! no-ops. Shared by the Linux and Windows arms.

use std::sync::Mutex;

use tauri::AppHandle;

/// Nothing to keep.
pub(crate) type QuickLookState = Mutex<()>;

/// Nothing to keep.
pub(crate) fn init_state() -> QuickLookState {
    Mutex::new(())
}

/// No panel opened.
pub(crate) async fn open(_app: AppHandle, _path: String, _volume_id: String) -> Result<bool, String> {
    Ok(false)
}

/// No panel to re-target.
pub(crate) async fn set_path(_app: AppHandle, _path: String, _volume_id: String) -> Result<(), String> {
    Ok(())
}

/// No panel to close.
pub(crate) async fn close(_app: AppHandle) -> Result<(), String> {
    Ok(())
}
