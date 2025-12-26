// Warn on unused dependencies to catch platform-specific cfg mismatches
#![warn(unused_crate_dependencies)]

mod commands;
mod file_system;
mod menu;

use menu::{MenuState, SHOW_HIDDEN_FILES_ID};
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

    builder
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Build and set the application menu
            // Default to showing hidden files (true)
            let (menu, show_hidden_item) = menu::build_menu(app.handle(), true)?;
            app.set_menu(menu)?;

            // Store the CheckMenuItem reference in app state
            let menu_state = MenuState::default();
            *menu_state.show_hidden_files.lock().unwrap() = Some(show_hidden_item);
            app.manage(menu_state);

            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == SHOW_HIDDEN_FILES_ID {
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
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::file_system::list_directory_contents,
            commands::file_system::path_exists
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
