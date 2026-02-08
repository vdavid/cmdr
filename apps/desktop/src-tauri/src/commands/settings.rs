//! Settings-related commands.

use std::net::TcpListener;

use tauri::{AppHandle, Manager};

use crate::file_system::update_debounce_ms;
use crate::ignore_poison::IgnorePoison;
use crate::menu::{MenuState, frontend_shortcut_to_accelerator, update_view_mode_accelerator};
#[cfg(target_os = "macos")]
use crate::network::bonjour::update_resolve_timeout;

/// Check if a port is available for binding.
#[tauri::command]
pub fn check_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Find an available port starting from the given port.
/// Scans up to 100 ports from the start port.
#[tauri::command]
pub fn find_available_port(start_port: u16) -> Option<u16> {
    for offset in 0..100 {
        let port = start_port.saturating_add(offset);
        if check_port_available(port) {
            return Some(port);
        }
    }
    None
}

/// Updates the file watcher debounce duration in milliseconds.
/// This affects newly created watchers; existing watchers keep their original duration.
#[tauri::command]
pub fn update_file_watcher_debounce(debounce_ms: u64) {
    update_debounce_ms(debounce_ms);
}

/// Updates the Bonjour service resolve timeout in milliseconds.
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

/// Update menu accelerator for a command.
/// Called from frontend when keyboard shortcuts are changed.
/// Currently supports: view.fullMode, view.briefMode
#[tauri::command]
pub fn update_menu_accelerator(app: AppHandle, command_id: &str, shortcut: &str) -> Result<(), String> {
    let menu_state = app.state::<MenuState<tauri::Wry>>();

    // Convert frontend shortcut format to Tauri accelerator format
    let accelerator = frontend_shortcut_to_accelerator(shortcut);

    match command_id {
        "view.fullMode" => {
            // Get current checked state before updating
            let is_checked = menu_state
                .view_mode_full
                .lock_ignore_poison()
                .as_ref()
                .and_then(|item| item.is_checked().ok())
                .unwrap_or(false);

            let new_item = update_view_mode_accelerator(&app, &menu_state, true, accelerator.as_deref(), is_checked)
                .map_err(|e| format!("Failed to update Full view accelerator: {e}"))?;

            // Update the reference in MenuState
            *menu_state.view_mode_full.lock_ignore_poison() = Some(new_item);
            Ok(())
        }
        "view.briefMode" => {
            // Get current checked state before updating
            let is_checked = menu_state
                .view_mode_brief
                .lock_ignore_poison()
                .as_ref()
                .and_then(|item| item.is_checked().ok())
                .unwrap_or(true);

            let new_item = update_view_mode_accelerator(&app, &menu_state, false, accelerator.as_deref(), is_checked)
                .map_err(|e| format!("Failed to update Brief view accelerator: {e}"))?;

            // Update the reference in MenuState
            *menu_state.view_mode_brief.lock_ignore_poison() = Some(new_item);
            Ok(())
        }
        _ => {
            // Silently succeed for commands that don't have menu items
            // This allows the frontend to call this for all shortcuts without errors
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
