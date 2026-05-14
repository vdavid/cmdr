//! E2E test support commands.

/// Returns the `CMDR_E2E_START_PATH` env var if set.
/// The frontend uses this to override startup paths for E2E tests.
/// Always compiled in — reading an unset env var is a no-op in production.
#[tauri::command]
#[specta::specta]
pub fn get_e2e_start_path() -> Option<String> {
    std::env::var("CMDR_E2E_START_PATH").ok()
}

/// Sets the per-file copy throttle (milliseconds) for the next write operation.
///
/// `None` clears the override. Tests use this to slow down the copy loop by a
/// known amount per file so they can click Cancel/Rollback deterministically
/// without staging large fixtures. Feature-gated to `playwright-e2e` so the
/// command isn't available in production binaries.
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub fn set_test_throttle(ms: Option<u64>) -> Result<(), String> {
    crate::test_mode::set_copy_throttle_override(ms);
    Ok(())
}

