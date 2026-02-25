//! E2E test support commands.

/// Returns the `CMDR_E2E_START_PATH` env var when the `automation` feature is enabled.
/// The frontend uses this to override startup paths for E2E tests.
#[tauri::command]
#[cfg(feature = "automation")]
pub fn get_e2e_start_path() -> Option<String> {
    std::env::var("CMDR_E2E_START_PATH").ok()
}

/// No-op fallback when the `automation` feature is disabled.
#[tauri::command]
#[cfg(not(feature = "automation"))]
pub fn get_e2e_start_path() -> Option<String> {
    None
}
