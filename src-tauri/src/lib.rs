// Deny unused code to catch dead code early (like knip for TS)
#![deny(unused)]
// Warn on unused dependencies to catch platform-specific cfg mismatches
#![warn(unused_crate_dependencies)]

//noinspection RsUnusedImport
// Silence false positives for dev dependencies (used only in benches/, not lib)
// and transitive dependencies (notify is used by notify-debouncer-full)
#[cfg(test)]
use criterion as _;
use notify as _;

mod commands;
pub mod config;
mod file_system;
pub mod icons;
#[cfg(target_os = "macos")]
mod macos_icons;
mod menu;
mod settings;

use menu::{MenuState, SHOW_HIDDEN_FILES_ID, VIEW_MODE_BRIEF_ID, VIEW_MODE_FULL_ID, ViewMode};
use tauri::{Emitter, Manager};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // Window state plugin is only available on desktop platforms
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_window_state::Builder::new().build());

    // MCP Bridge plugin is only available in debug builds for security
    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_mcp_bridge::init());

    builder
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Initialize the file watcher manager with app handle for events
            file_system::init_watcher_manager(app.handle().clone());

            // Load persisted settings to initialize menu with correct state
            let saved_settings = settings::load_settings(app.handle());

            // Build and set the application menu with persisted showHiddenFiles
            // Note: view mode is per-pane and managed by frontend, so we default to Brief here
            let menu_items = menu::build_menu(app.handle(), saved_settings.show_hidden_files, ViewMode::Brief)?;
            app.set_menu(menu_items.menu)?;

            // Store the CheckMenuItem references in app state
            let menu_state = MenuState::default();
            *menu_state.show_hidden_files.lock().unwrap() = Some(menu_items.show_hidden_files);
            *menu_state.view_mode_full.lock().unwrap() = Some(menu_items.view_mode_full);
            *menu_state.view_mode_brief.lock().unwrap() = Some(menu_items.view_mode_brief);
            app.manage(menu_state);

            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if id == SHOW_HIDDEN_FILES_ID {
                // Get the CheckMenuItem from app state
                let menu_state = app.state::<MenuState<tauri::Wry>>();
                let guard = menu_state.show_hidden_files.lock().unwrap();
                let Some(check_item) = guard.as_ref() else {
                    return;
                };

                // CheckMenuItem auto-toggles on click, so is_checked() returns the NEW state
                // We just need to read and emit it, not toggle again
                let new_state = check_item.is_checked().unwrap_or(true);

                // Emit event to frontend with the new state
                let _ = app.emit("settings-changed", serde_json::json!({ "showHiddenFiles": new_state }));
            } else if id == VIEW_MODE_FULL_ID || id == VIEW_MODE_BRIEF_ID {
                // Handle view mode toggle (radio button behavior)
                let menu_state = app.state::<MenuState<tauri::Wry>>();

                let (full_guard, brief_guard) = (
                    menu_state.view_mode_full.lock().unwrap(),
                    menu_state.view_mode_brief.lock().unwrap(),
                );

                if let (Some(full_item), Some(brief_item)) = (full_guard.as_ref(), brief_guard.as_ref()) {
                    // Set the correct check state (radio behavior)
                    let is_full = id == VIEW_MODE_FULL_ID;
                    let _ = full_item.set_checked(is_full);
                    let _ = brief_item.set_checked(!is_full);

                    // Emit event to frontend
                    let mode = if is_full { "full" } else { "brief" };
                    let _ = app.emit("view-mode-changed", serde_json::json!({ "mode": mode }));
                }
            } else {
                // Handle file actions
                commands::ui::execute_menu_action(app, id);
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::file_system::list_directory_start_session,
            commands::file_system::list_directory_next_chunk,
            commands::file_system::list_directory_end_session,
            commands::file_system::path_exists,
            commands::icons::get_icons,
            commands::icons::refresh_directory_icons,
            commands::ui::show_file_context_menu,
            commands::ui::update_menu_context,
            #[cfg(target_os = "macos")]
            commands::sync_status::get_sync_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
