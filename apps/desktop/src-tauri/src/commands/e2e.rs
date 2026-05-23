//! E2E test support commands.

/// Returns the `CMDR_E2E_START_PATH` env var if set.
/// The frontend uses this to override startup paths for E2E tests.
/// Always compiled in. Reading an unset env var is a no-op in production.
#[tauri::command]
#[specta::specta]
pub fn get_e2e_start_path() -> Option<String> {
    std::env::var("CMDR_E2E_START_PATH").ok()
}

/// Returns `true` when running under an E2E harness (`CMDR_E2E_MODE=1`).
/// The frontend uses this to switch the title-bar styling and decorate child
/// window titles so a tester can tell an automated window apart from prod or
/// dev. Always compiled in; reading an unset env var is a no-op in production.
#[tauri::command]
#[specta::specta]
pub fn is_e2e_mode() -> bool {
    crate::test_mode::is_e2e_mode()
}

/// Returns `true` when `CMDR_FORCE_ONBOARDING` is set, regardless of value.
///
/// The frontend uses this to bypass the `isOnboarded` gate and force the
/// onboarding wizard open on every launch (mirrors `CMDR_MOCK_LICENSE` /
/// `CMDR_E2E_MODE`). Pair with `CMDR_MOCK_FDA` (in `permissions.rs`) to
/// drive each step's variants without ever touching real System Settings.
///
/// Synchronous + no filesystem access, so no `blocking_with_timeout` needed.
#[tauri::command]
#[specta::specta]
pub fn is_force_onboarding() -> bool {
    std::env::var("CMDR_FORCE_ONBOARDING").is_ok()
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

/// Flushes any pending file-watcher events for E2E synchronization.
///
/// The notify-debouncer-full crate buffers events for `DEBOUNCE_MS` (200 ms by
/// default), plus the OS itself coalesces FSEvents on macOS over a longer
/// window, so a single FS mutation can take 1–10 s to land in the UI. For
/// tests, that's pure waste.
///
/// This command sidesteps the debouncer: it iterates every active listing and
/// calls `handle_directory_change` (re-reads via the Volume trait, computes
/// the diff, updates LISTING_CACHE, emits `directory-diff`). After it
/// returns, the frontend has the full delta.
///
/// Feature-gated to `playwright-e2e` so production builds can't accidentally
/// bypass the debouncer (which exists to prevent thrash on bursts of events).
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub async fn flush_file_watcher() -> Result<(), String> {
    crate::file_system::flush_all_watchers().await;
    Ok(())
}
