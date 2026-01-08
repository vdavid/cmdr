// Deny unused code to catch dead code early (like knip for TS)
#![deny(unused)]
// Warn on unused dependencies to catch platform-specific cfg mismatches
#![warn(unused_crate_dependencies)]

//noinspection RsUnusedImport
// Silence false positives for dev dependencies (used only in benches/, not lib)
// and transitive dependencies (notify is used by notify-debouncer-full)
#[cfg(test)]
use criterion as _;
//noinspection RsUnusedImport
use notify as _;
//noinspection ALL
// smb crates are used in network/smb_client module (macOS only)
#[cfg(target_os = "macos")]
use smb as _;
//noinspection ALL
#[cfg(target_os = "macos")]
use smb_rpc as _;

//noinspection ALL
// chrono is used in network/known_shares.rs for timestamps
#[cfg(target_os = "macos")]
use chrono as _;
//noinspection ALL
// MCP Bridge is only used in debug builds, so silence the warning in release builds
#[cfg(not(debug_assertions))]
use tauri_plugin_mcp_bridge as _;
//noinspection ALL
// security_framework is used in network/keychain.rs for Keychain integration
#[cfg(target_os = "macos")]
use security_framework as _;

pub mod benchmark;
mod commands;
pub mod config;
mod file_system;
mod font_metrics;
pub mod icons;
#[cfg(target_os = "macos")]
mod macos_icons;
mod menu;
#[cfg(target_os = "macos")]
mod network;
#[cfg(target_os = "macos")]
mod permissions;
mod settings;
#[cfg(target_os = "macos")]
mod volumes;

use menu::{
    GO_BACK_ID, GO_FORWARD_ID, GO_PARENT_ID, MenuState, SHOW_HIDDEN_FILES_ID, VIEW_MODE_BRIEF_ID, VIEW_MODE_FULL_ID,
    ViewMode,
};
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
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Initialize logging - respects RUST_LOG env var (default: info)
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                .format_timestamp_millis()
                .init();

            // Initialize benchmarking (enabled by RUSTY_COMMANDER_BENCHMARK=1)
            benchmark::init_benchmarking();

            // Initialize the file watcher manager with app handle for events
            file_system::init_watcher_manager(app.handle().clone());

            // Initialize the volume manager with the root volume
            file_system::init_volume_manager();

            // Start network host discovery (Bonjour)
            #[cfg(target_os = "macos")]
            network::start_discovery(app.handle().clone());

            // Start volume mount/unmount watcher
            #[cfg(target_os = "macos")]
            volumes::watcher::start_volume_watcher(app.handle());

            // Inject Docker SMB test hosts if enabled (dev mode only)
            // Enable with: RUSTY_INJECT_TEST_SMB=1 pnpm tauri dev
            #[cfg(target_os = "macos")]
            network::inject_test_hosts_if_enabled(app.handle());

            // Load known network shares from disk
            #[cfg(target_os = "macos")]
            network::known_shares::load_known_shares(app.handle());

            // Initialize font metrics for default font (system font at 12px)
            font_metrics::init_font_metrics(app.handle(), "system-400-12");

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
            } else if id == GO_BACK_ID || id == GO_FORWARD_ID || id == GO_PARENT_ID {
                // Handle Go menu navigation actions
                let action = match id {
                    GO_BACK_ID => "back",
                    GO_FORWARD_ID => "forward",
                    GO_PARENT_ID => "parent",
                    _ => return,
                };
                let _ = app.emit("navigation-action", serde_json::json!({ "action": action }));
            } else {
                // Handle file actions
                commands::ui::execute_menu_action(app, id);
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::file_system::list_directory_start,
            commands::file_system::list_directory_end,
            commands::file_system::get_file_range,
            commands::file_system::get_file_at,
            commands::file_system::get_total_count,
            commands::file_system::get_max_filename_width,
            commands::file_system::find_file_index,
            commands::file_system::resort_listing,
            commands::file_system::path_exists,
            commands::file_system::benchmark_log,
            commands::font_metrics::store_font_metrics,
            commands::font_metrics::has_font_metrics,
            commands::icons::get_icons,
            commands::icons::refresh_directory_icons,
            commands::ui::show_file_context_menu,
            commands::ui::show_main_window,
            commands::ui::update_menu_context,
            #[cfg(target_os = "macos")]
            commands::sync_status::get_sync_status,
            #[cfg(target_os = "macos")]
            commands::volumes::list_volumes,
            #[cfg(target_os = "macos")]
            commands::volumes::get_default_volume_id,
            #[cfg(target_os = "macos")]
            commands::volumes::find_containing_volume,
            #[cfg(target_os = "macos")]
            commands::network::list_network_hosts,
            #[cfg(target_os = "macos")]
            commands::network::get_network_discovery_state,
            #[cfg(target_os = "macos")]
            commands::network::resolve_host,
            #[cfg(target_os = "macos")]
            commands::network::list_shares_on_host,
            #[cfg(target_os = "macos")]
            commands::network::prefetch_shares,
            #[cfg(target_os = "macos")]
            commands::network::get_host_auth_mode,
            #[cfg(target_os = "macos")]
            commands::network::fe_log,
            #[cfg(target_os = "macos")]
            commands::network::get_known_shares,
            #[cfg(target_os = "macos")]
            commands::network::get_known_share_by_name,
            #[cfg(target_os = "macos")]
            commands::network::update_known_share,
            #[cfg(target_os = "macos")]
            commands::network::get_username_hints,
            #[cfg(target_os = "macos")]
            commands::network::save_smb_credentials,
            #[cfg(target_os = "macos")]
            commands::network::get_smb_credentials,
            #[cfg(target_os = "macos")]
            commands::network::has_smb_credentials,
            #[cfg(target_os = "macos")]
            commands::network::delete_smb_credentials,
            #[cfg(target_os = "macos")]
            commands::network::list_shares_with_credentials,
            #[cfg(target_os = "macos")]
            commands::network::mount_network_share,
            #[cfg(target_os = "macos")]
            permissions::check_full_disk_access,
            #[cfg(target_os = "macos")]
            permissions::open_privacy_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
