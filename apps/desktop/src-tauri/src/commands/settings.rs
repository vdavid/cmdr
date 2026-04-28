//! Settings-related commands.

use tauri::{AppHandle, Manager};

use crate::file_system::{
    set_direct_smb_enabled, set_filter_safe_save_artifacts, set_smb_concurrency, update_debounce_ms,
};
use crate::ignore_poison::IgnorePoison;
use crate::menu::{
    MenuState, command_id_to_menu_id, frontend_shortcut_to_accelerator, update_menu_item_accelerator,
    update_view_mode_accelerator,
};
#[cfg(target_os = "macos")]
use crate::network::mdns_discovery::update_resolve_timeout;

/// Check if a port is available for binding.
#[tauri::command]
pub fn check_port_available(port: u16) -> bool {
    crate::net::is_port_available(port)
}

/// Find an available port starting from the given port.
/// Scans up to 100 ports from the start port.
#[tauri::command]
pub fn find_available_port(start_port: u16) -> Option<u16> {
    crate::net::find_available_port(start_port)
}

/// Updates the file watcher debounce duration in milliseconds.
/// This affects newly created watchers; existing watchers keep their original duration.
#[tauri::command]
pub fn update_file_watcher_debounce(debounce_ms: u64) {
    update_debounce_ms(debounce_ms);
}

/// Updates the mDNS service resolve timeout in milliseconds.
/// This affects future service resolutions; ongoing resolutions keep their original timeout.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn update_service_resolve_timeout(timeout_ms: u64) {
    update_resolve_timeout(timeout_ms);
}

/// Stub for non-macOS platforms - network discovery is not supported.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn update_service_resolve_timeout(_timeout_ms: u64) {
    // No-op on non-macOS platforms
}

/// Enable or disable automatic upgrade of SMB mounts to direct smb2 connections.
/// Pushed live from the frontend whenever `network.directSmbConnection` changes.
#[tauri::command]
pub fn set_direct_smb_connection(enabled: bool) {
    set_direct_smb_enabled(enabled);
}

/// Toggle filtering of macOS safe-save artifacts (`.sb-*` files) in the SMB watcher.
/// Pushed live from the frontend whenever `advanced.filterSafeSaveArtifacts` changes.
#[tauri::command]
pub fn set_filter_safe_save_artifacts_cmd(enabled: bool) {
    set_filter_safe_save_artifacts(enabled);
}

/// Update the SMB concurrency limit used by `SmbVolume::max_concurrent_ops()`.
/// Clamped to `1..=32` by `set_smb_concurrency`. Pushed live from the frontend
/// whenever `network.smbConcurrency` changes.
#[tauri::command]
pub fn set_smb_concurrency_cmd(value: u16) {
    set_smb_concurrency(value as usize);
}

/// Updates the in-RAM log-storage cap and runs an eager prune so the user sees excess files
/// disappear immediately when they lower the cap.
///
/// Note: the actual rotation strategy is fixed at app start (`file-rotate` is constructed
/// once with its keep-N value). Restart-to-apply is therefore unavoidable for
/// `0 ↔ non-zero` transitions and for raising the cap above the previously baked-in value.
/// The frontend toasts a "restart required" notice for those cases.
///
/// `value` is in MB. `0` means "log storage disabled".
#[tauri::command]
pub fn set_max_log_storage_mb(value: u64) -> Result<(), String> {
    use crate::logging;

    let new_keep = if value == 0 { 0 } else { value.div_ceil(50) as usize };
    logging::set_keep_count(new_keep);

    // Eager-prune is purely cosmetic — files would also vanish on the next rotation, but
    // the user just changed a setting, so we should show the effect now.
    if let Some(dir) = logging::log_dir() {
        match logging::eager_prune(dir, new_keep) {
            Ok(0) => {}
            Ok(n) => log::info!(
                target: "cmdr_lib::logging",
                "Eager-pruned {n} log files after cap change ({value} MB → keep {new_keep}).",
            ),
            Err(err) => return Err(format!("Failed to eager-prune log files: {err}")),
        }
    }
    Ok(())
}

/// Enable or disable the Flow B error-report auto-dispatcher.
/// Pushed live from the frontend whenever `updates.errorReports` changes.
#[tauri::command]
pub fn set_error_reports_enabled(value: bool) {
    crate::error_reporter::auto_dispatcher::set_enabled(value);
}

/// Enable or disable the virtual `.git` portal. When off, navigating into
/// `.git` shows the raw on-disk contents instead of the branches/tags/commits
/// virtual folders. Pushed live from the frontend whenever
/// `fileExplorer.git.showVirtualGitPortal` changes.
///
/// Flipping the atomic isn't enough on its own: panes already showing a
/// virtual `.git/...` listing keep their cached children until the next
/// navigation. We refresh every cached listing under any subscribed repo's
/// `.git/` so the change is visible immediately.
#[tauri::command]
pub fn set_show_virtual_git_portal(enabled: bool) {
    crate::file_system::git::set_virtual_portal_enabled(enabled);
    crate::file_system::git::watcher::refresh_all_virtual_listings_after_toggle();
}

/// Update menu accelerator for a command.
/// Called from frontend when keyboard shortcuts are changed.
#[tauri::command]
pub fn update_menu_accelerator(app: AppHandle, command_id: &str, shortcut: &str) -> Result<(), String> {
    let menu_state = app.state::<MenuState<tauri::Wry>>();

    // Convert frontend shortcut format to Tauri accelerator format
    let accelerator = frontend_shortcut_to_accelerator(shortcut);

    match command_id {
        // View mode CheckMenuItems need special handling to preserve checked state
        "view.fullMode" => {
            let is_checked = menu_state
                .view_mode_full
                .lock_ignore_poison()
                .as_ref()
                .and_then(|item| item.is_checked().ok())
                .unwrap_or(false);

            let new_item = update_view_mode_accelerator(&app, &menu_state, true, accelerator.as_deref(), is_checked)
                .map_err(|e| format!("Failed to update Full view accelerator: {e}"))?;

            *menu_state.view_mode_full.lock_ignore_poison() = Some(new_item);
            Ok(())
        }
        "view.briefMode" => {
            let is_checked = menu_state
                .view_mode_brief
                .lock_ignore_poison()
                .as_ref()
                .and_then(|item| item.is_checked().ok())
                .unwrap_or(true);

            let new_item = update_view_mode_accelerator(&app, &menu_state, false, accelerator.as_deref(), is_checked)
                .map_err(|e| format!("Failed to update Brief view accelerator: {e}"))?;

            *menu_state.view_mode_brief.lock_ignore_poison() = Some(new_item);
            Ok(())
        }
        // All other commands: use the generic HashMap-based update
        _ => {
            if let Some(menu_id) = command_id_to_menu_id(command_id) {
                update_menu_item_accelerator(&app, &menu_state, menu_id, accelerator.as_deref())
                    .map_err(|e| format!("Failed to update {command_id} accelerator: {e}"))?;
            }
            // Silently succeed for commands that don't have menu items
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_port_available() {
        // Port 0 should let OS pick an available port, so this should succeed
        // But we test a high port that's likely free
        let result = check_port_available(49999);
        // The function should return a valid boolean (either true or false)
        // This test verifies the function executes without panic
        let _ = result;
    }

    /// Covers the three live-apply commands in one test because they share
    /// process-global atomics — running them as separate `#[test]` fns would
    /// race under the default parallel test runner.
    #[test]
    fn test_live_apply_commands() {
        use std::sync::Mutex;
        // Serialize across any other test that might touch the same globals.
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // smb_concurrency clamps 0 → 1 (min)
        set_smb_concurrency_cmd(0);
        assert_eq!(crate::file_system::smb_concurrency(), 1);

        // smb_concurrency clamps 100 → 32 (max)
        set_smb_concurrency_cmd(100);
        assert_eq!(crate::file_system::smb_concurrency(), 32);

        // smb_concurrency accepts values within 1..=32 unchanged
        set_smb_concurrency_cmd(7);
        assert_eq!(crate::file_system::smb_concurrency(), 7);

        // direct_smb_connection round-trips
        set_direct_smb_connection(false);
        assert!(!crate::file_system::is_direct_smb_enabled());
        set_direct_smb_connection(true);
        assert!(crate::file_system::is_direct_smb_enabled());

        // filter_safe_save_artifacts round-trips
        set_filter_safe_save_artifacts_cmd(false);
        assert!(!crate::file_system::is_filter_safe_save_artifacts_enabled());
        set_filter_safe_save_artifacts_cmd(true);
        assert!(crate::file_system::is_filter_safe_save_artifacts_enabled());

        // Restore defaults so later tests see a predictable state.
        set_smb_concurrency_cmd(10);
    }

    #[test]
    fn test_find_available_port() {
        // Should find some available port
        let result = find_available_port(49000);
        // On most systems, we should find an available port in the high range
        assert!(result.is_some());
        if let Some(port) = result {
            assert!(port >= 49000);
            assert!(port < 49100);
        }
    }
}
