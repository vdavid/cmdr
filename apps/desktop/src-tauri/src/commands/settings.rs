//! Settings-related commands.

use tauri::{AppHandle, Manager};

use crate::file_system::{
    set_direct_smb_enabled, set_filter_safe_save_artifacts, set_smb_concurrency, update_debounce_ms,
};
use crate::ignore_poison::IgnorePoison;
use crate::menu::{
    MenuState, command_id_to_menu_id, frontend_shortcut_to_accelerator, rebuild_view_mode_items,
    update_menu_item_accelerator,
};
#[cfg(target_os = "macos")]
use crate::network::mdns_discovery::update_resolve_timeout;

/// Check if a port is available for binding.
#[tauri::command]
#[specta::specta]
pub fn check_port_available(port: u16) -> bool {
    crate::net::is_port_available(port)
}

/// Find an available port starting from the given port.
/// Scans up to 100 ports from the start port.
#[tauri::command]
#[specta::specta]
pub fn find_available_port(start_port: u16) -> Option<u16> {
    crate::net::find_available_port(start_port)
}

/// Updates the file watcher debounce duration in milliseconds.
/// This affects newly created watchers; existing watchers keep their original duration.
#[tauri::command]
#[specta::specta]
pub fn update_file_watcher_debounce(debounce_ms: u64) {
    update_debounce_ms(debounce_ms);
}

/// Returns the absolute path the frontend's `tauri-plugin-store` should load for
/// a given store file (for example `settings.json`, `shortcuts.json`,
/// `app-status.json`, `viewer-tail.json`), but ONLY when this is an isolated
/// instance (dev, per-worktree dev, or E2E — anything that sets
/// `CMDR_DATA_DIR`). Returns `None` for production so each store keeps resolving
/// via `BaseDirectory::AppData` exactly as before.
///
/// Why this exists: `tauri-plugin-store` resolves a bare store name against
/// Tauri's `app_data_dir()` (identifier-driven), which ignores `CMDR_DATA_DIR`.
/// The Rust-side data dir (`resolved_app_data_dir`) already honors
/// `CMDR_DATA_DIR`, so without this the frontend stores and the backend read
/// *different* files in any isolated instance. In E2E that means the suite reads
/// the developer's real `~/Library/Application Support/com.veszelovszki.cmdr/…`
/// files — so a locally-flipped setting (for example
/// `fileExplorer.suppressQuickLookHint`) or a remapped shortcut leaks into tests
/// and makes them fail on that machine while passing in CI (which has no such
/// file). Pointing each store at the resolved data dir closes that gap.
/// Production is unaffected: `CMDR_DATA_DIR` is unset there, so this returns
/// `None` and the store path is byte-identical to before.
///
/// Security: `store_name` crosses the IPC boundary from the frontend. The
/// returned path must always land *inside* the resolved data dir, so we reject
/// anything that isn't a plain filename — see `sanitize_store_name`. A rejected
/// name yields `None`, which the frontend treats exactly like production (it
/// falls back to the bare name), so a bad input can never escape the data dir.
#[tauri::command]
#[specta::specta]
pub fn get_isolated_store_path(app: AppHandle, store_name: String) -> Option<String> {
    std::env::var("CMDR_DATA_DIR").ok().filter(|s| !s.is_empty())?;
    let name = sanitize_store_name(&store_name)?;
    let dir = crate::config::resolved_app_data_dir(&app).ok()?;
    Some(dir.join(name).to_string_lossy().into_owned())
}

/// Validates that `store_name` is a single plain filename safe to join onto the
/// data dir, returning it unchanged on success. Rejects absolute paths, any name
/// containing a path separator (`/` or `\`) or a `..` component, and the special
/// `.`/`..` names — anything that could let the joined path escape the data dir.
/// We don't strip to the last component; a name with separators is a frontend
/// bug or an attack, so we reject outright rather than silently reinterpret it.
fn sanitize_store_name(store_name: &str) -> Option<&str> {
    let trimmed = store_name.trim();
    if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") || trimmed == "."
    {
        return None;
    }
    // Final guard: the OS-level parse must yield exactly one normal component.
    let mut components = std::path::Path::new(trimmed).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(c)), None) if c == trimmed => Some(trimmed),
        _ => None,
    }
}

/// Updates the mDNS service resolve timeout in milliseconds.
/// This affects future service resolutions; ongoing resolutions keep their original timeout.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub fn update_service_resolve_timeout(timeout_ms: u64) {
    update_resolve_timeout(timeout_ms);
}

/// Stub for non-macOS platforms - network discovery is not supported.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub fn update_service_resolve_timeout(_timeout_ms: u64) {
    // No-op on non-macOS platforms
}

/// Enable or disable automatic upgrade of SMB mounts to direct smb2 connections.
/// Pushed live from the frontend whenever `network.directSmbConnection` changes.
#[tauri::command]
#[specta::specta]
pub fn set_direct_smb_connection(enabled: bool) {
    set_direct_smb_enabled(enabled);
}

/// Toggle filtering of macOS safe-save artifacts (`.sb-*` files) in the SMB watcher.
/// Pushed live from the frontend whenever `advanced.filterSafeSaveArtifacts` changes.
#[tauri::command]
#[specta::specta]
pub fn set_filter_safe_save_artifacts_cmd(enabled: bool) {
    set_filter_safe_save_artifacts(enabled);
}

/// Update the SMB concurrency limit used by `SmbVolume::max_concurrent_ops()`.
/// Clamped to `1..=32` by `set_smb_concurrency`. Pushed live from the frontend
/// whenever `network.smbConcurrency` changes.
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
pub fn set_max_log_storage_mb(value: u64) -> Result<(), String> {
    use crate::logging;

    let new_keep = if value == 0 { 0 } else { value.div_ceil(50) as usize };
    logging::set_keep_count(new_keep);

    // Eager-prune is purely cosmetic: files would also vanish on the next rotation, but
    // the user just changed a setting, so we should show the effect now.
    if let Some(dir) = logging::log_dir() {
        match logging::eager_prune(dir, new_keep) {
            Ok(0) => {}
            Ok(n) => log::info!(
                target: "cmdr_lib::logging",
                "Eager-pruned {} after cap change ({value} MB → keep {new_keep}).",
                crate::pluralize::pluralize(n as u64, "log file"),
            ),
            Err(err) => return Err(format!("Failed to eager-prune log files: {err}")),
        }
    }
    Ok(())
}

/// Enable or disable the Flow B error-report auto-dispatcher.
/// Pushed live from the frontend whenever `updates.errorReports` changes.
#[tauri::command]
#[specta::specta]
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
/// `async` is load-bearing here: the body fans out to `notify_directory_changed`
/// which calls `tokio::spawn` to emit cache-refresh events. Sync Tauri commands
/// run on the IPC main thread, which has no Tokio reactor: `spawn` panicked
/// there with "there is no reactor running" (see crash report 2026-05-05).
/// Marking the command `async` puts it on a Tokio worker, where `spawn` works.
#[tauri::command]
#[specta::specta]
pub async fn set_show_virtual_git_portal(enabled: bool) {
    crate::file_system::git::set_virtual_portal_enabled(enabled);
    crate::file_system::git::watcher::refresh_all_virtual_listings_after_toggle();
}

/// Update menu accelerator for a command.
/// Called from frontend when keyboard shortcuts are changed.
#[tauri::command]
#[specta::specta]
pub fn update_menu_accelerator(app: AppHandle, command_id: &str, shortcut: &str) -> Result<(), String> {
    let menu_state = app.state::<MenuState<tauri::Wry>>();

    // Convert frontend shortcut format to Tauri accelerator format
    let accelerator = frontend_shortcut_to_accelerator(shortcut);

    match command_id {
        // View mode CheckMenuItems are per-pane and the accelerator only attaches to the
        // active pane's pair. Cache the new accel in MenuState and rebuild: the rebuild
        // re-reads cached accels and active-pane state in one shot.
        "view.fullMode" => {
            *menu_state.view_mode_full_accel.lock_ignore_poison() = accelerator;
            rebuild_view_mode_items(&app, &menu_state)
                .map_err(|e| format!("Failed to update Full view accelerator: {e}"))?;
            Ok(())
        }
        "view.briefMode" => {
            *menu_state.view_mode_brief_accel.lock_ignore_poison() = accelerator;
            rebuild_view_mode_items(&app, &menu_state)
                .map_err(|e| format!("Failed to update Brief view accelerator: {e}"))?;
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
    /// process-global atomics: running them as separate `#[test]` fns would
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
    fn test_sanitize_store_name_accepts_plain_filenames() {
        for name in [
            "settings.json",
            "shortcuts.json",
            "app-status.json",
            "viewer-tail.json",
            "no-extension",
        ] {
            assert_eq!(sanitize_store_name(name), Some(name), "should accept {name}");
        }
    }

    #[test]
    fn test_sanitize_store_name_rejects_traversal_and_absolute() {
        // None of these may produce a path that escapes the data dir, so all are rejected.
        for name in [
            "",
            "   ",
            ".",
            "..",
            "../settings.json",
            "../../etc/passwd",
            "foo/bar.json",
            "/etc/passwd",
            "/absolute.json",
            "sub/settings.json",
            r"..\windows.json",
            r"C:\windows.json",
            "a/../b.json",
        ] {
            assert_eq!(sanitize_store_name(name), None, "should reject {name:?}");
        }
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
