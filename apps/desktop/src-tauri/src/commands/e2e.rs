//! E2E test support commands.

/// Returns the `CMDR_E2E_START_PATH` env var if set.
/// The frontend uses this to override startup paths for E2E tests.
/// Always compiled in — reading an unset env var is a no-op in production.
#[tauri::command]
pub fn get_e2e_start_path() -> Option<String> {
    std::env::var("CMDR_E2E_START_PATH").ok()
}
