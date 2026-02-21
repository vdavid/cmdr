// Deny unused code to catch dead code early (like knip for TS)
#![deny(unused)]
// Warn on unused dependencies to catch platform-specific cfg mismatches
#![warn(unused_crate_dependencies)]
// Warn on redundant path prefixes (like std::path::Path when Path is imported)
#![warn(unused_qualifications)]
// Use log::* macros instead of println!/eprintln! for proper log level control
#![deny(clippy::print_stdout, clippy::print_stderr)]
// Require justification for all #[allow] attributes
#![warn(clippy::allow_attributes_without_reason)]

//noinspection RsUnusedImport
// Silence false positives for dev dependencies (used only in benches/, not lib)
// and transitive dependencies (notify is used by notify-debouncer-full)
#[cfg(test)]
use criterion as _;
//noinspection RsUnusedImport
use notify as _;
//noinspection RsUnusedImport
// drag is used by tauri-plugin-drag for drag-and-drop support
use drag as _;
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
//noinspection ALL
// mtp-rs is used in mtp/ module for Android device support (macOS only, Phase 1 foundation)
#[cfg(target_os = "macos")]
use mtp_rs as _;
//noinspection ALL
// nusb is used in mtp/watcher.rs for USB hotplug detection
#[cfg(target_os = "macos")]
use nusb as _;

mod ignore_poison;
pub use ignore_poison::IgnorePoison;

#[cfg(target_os = "macos")]
mod accent_color;
mod ai;
pub mod benchmark;
mod commands;
pub mod config;
#[cfg(target_os = "macos")]
mod drag_image_detection;
#[cfg(target_os = "macos")]
mod drag_image_swap;
mod file_system;
pub(crate) mod file_viewer;
mod font_metrics;
pub mod icons;
mod indexing;
pub mod licensing;
#[cfg(target_os = "macos")]
mod macos_icons;
mod mcp;
mod menu;
#[cfg(target_os = "macos")]
mod mtp;
#[cfg(target_os = "macos")]
mod network;
#[cfg(target_os = "macos")]
mod permissions;
mod settings;
#[cfg(target_os = "macos")]
mod volumes;

// Linux/non-macOS stubs for E2E testing
#[cfg(not(target_os = "macos"))]
mod stubs;

use menu::{
    ABOUT_ID, COMMAND_PALETTE_ID, ENTER_LICENSE_KEY_ID, GO_BACK_ID, GO_FORWARD_ID, GO_PARENT_ID, MenuState, RENAME_ID,
    SETTINGS_ID, SHOW_HIDDEN_FILES_ID, SORT_ASCENDING_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID,
    SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID, SORT_BY_SIZE_ID, SORT_DESCENDING_ID, SWAP_PANES_ID, SWITCH_PANE_ID,
    VIEW_MODE_BRIEF_ID, VIEW_MODE_FULL_ID, VIEWER_WORD_WRAP_ID, ViewMode,
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

    // Skip updater plugin in CI to avoid network dependency and latency during E2E tests
    let builder = if std::env::var("CI").is_ok() {
        builder
    } else {
        builder.plugin(tauri_plugin_updater::Builder::new().build())
    };

    builder
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
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

            // Start MTP device hotplug watcher (Android device support)
            #[cfg(target_os = "macos")]
            mtp::start_mtp_watcher(app.handle());

            // Load known network shares from disk
            #[cfg(target_os = "macos")]
            network::known_shares::load_known_shares(app.handle());

            // Install drag image detection swizzle (macOS only)
            #[cfg(target_os = "macos")]
            drag_image_detection::install(app.handle().clone());

            // Observe system accent color changes and emit events to frontend
            #[cfg(target_os = "macos")]
            accent_color::observe_accent_color_changes(app.handle().clone());

            // Initialize font metrics for default font (system font at 12px)
            font_metrics::init_font_metrics(app.handle(), "system-400-12");

            // Load persisted settings to initialize menu with correct state
            let saved_settings = settings::load_settings(app.handle());

            // Check if there's an existing license (for menu text)
            let has_existing_license = licensing::get_license_info(app.handle()).is_some();

            // Build and set the application menu with persisted showHiddenFiles
            // Note: view mode is per-pane and managed by frontend, so we default to Brief here
            let menu_items = menu::build_menu(
                app.handle(),
                saved_settings.show_hidden_files,
                ViewMode::Brief,
                has_existing_license,
            )?;
            app.set_menu(menu_items.menu)?;

            // Store the CheckMenuItem references in app state
            let menu_state = MenuState::default();
            *menu_state.show_hidden_files.lock_ignore_poison() = Some(menu_items.show_hidden_files);
            *menu_state.view_mode_full.lock_ignore_poison() = Some(menu_items.view_mode_full);
            *menu_state.view_mode_brief.lock_ignore_poison() = Some(menu_items.view_mode_brief);
            *menu_state.view_submenu.lock_ignore_poison() = Some(menu_items.view_submenu);
            *menu_state.view_mode_full_position.lock_ignore_poison() = menu_items.view_mode_full_position;
            *menu_state.view_mode_brief_position.lock_ignore_poison() = menu_items.view_mode_brief_position;
            app.manage(menu_state);

            // Set window title based on license status
            let license_status = licensing::get_app_status(app.handle());
            let title = licensing::get_window_title(&license_status);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(&title);
            }

            // Initialize pane state store for MCP context tools
            app.manage(mcp::PaneStateStore::new());

            // Initialize soft dialog tracker for MCP (overlays like about, license, confirmations)
            app.manage(mcp::SoftDialogTracker::new());

            // Initialize settings state store for MCP settings tools
            app.manage(mcp::SettingsStateStore::new());

            // Start MCP server for AI agent integration
            // Use settings from user preferences, with env vars as override for dev
            let mcp_config = mcp::McpConfig::from_settings_and_env(
                saved_settings.developer_mcp_enabled,
                saved_settings.developer_mcp_port,
            );
            mcp::start_mcp_server(app.handle().clone(), mcp_config);

            // Initialize AI manager (starts llama-server if model is installed)
            ai::manager::init(app.handle());

            // Register indexing state (does not start scanning; controlled by settings + env var)
            indexing::init(app.handle());

            // Auto-start indexing if conditions are met (settings + dev env var)
            if indexing::should_auto_start(saved_settings.indexing_enabled) {
                let app_handle = app.handle().clone();
                // Use tauri's runtime spawn instead of tokio::spawn since setup()
                // runs synchronously before the Tokio runtime is fully available
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = indexing::start_indexing(&app_handle) {
                        log::warn!("Failed to auto-start indexing: {e}");
                    }
                });
            } else {
                log::info!("Drive indexing auto-start skipped (disabled in settings or set CMDR_DRIVE_INDEX=1 in dev)");
            }

            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if id == SHOW_HIDDEN_FILES_ID {
                // Get the CheckMenuItem from app state
                let menu_state = app.state::<MenuState<tauri::Wry>>();
                let guard = menu_state.show_hidden_files.lock_ignore_poison();
                let Some(check_item) = guard.as_ref() else {
                    return;
                };

                // CheckMenuItem auto-toggles on click, so is_checked() returns the NEW state
                // We just need to read and emit it, not toggle again
                let new_state = check_item.is_checked().unwrap_or(true);

                // Emit event to frontend with the new state (main window only)
                let _ = app.emit_to(
                    "main",
                    "settings-changed",
                    serde_json::json!({ "showHiddenFiles": new_state }),
                );
            } else if id == VIEW_MODE_FULL_ID || id == VIEW_MODE_BRIEF_ID {
                // Handle view mode toggle (radio button behavior)
                let menu_state = app.state::<MenuState<tauri::Wry>>();

                let (full_guard, brief_guard) = (
                    menu_state.view_mode_full.lock_ignore_poison(),
                    menu_state.view_mode_brief.lock_ignore_poison(),
                );

                if let (Some(full_item), Some(brief_item)) = (full_guard.as_ref(), brief_guard.as_ref()) {
                    // Set the correct check state (radio behavior)
                    let is_full = id == VIEW_MODE_FULL_ID;
                    let _ = full_item.set_checked(is_full);
                    let _ = brief_item.set_checked(!is_full);

                    // Emit event to frontend (main window only)
                    let mode = if is_full { "full" } else { "brief" };
                    let _ = app.emit_to("main", "view-mode-changed", serde_json::json!({ "mode": mode }));
                }
            } else if id == GO_BACK_ID || id == GO_FORWARD_ID || id == GO_PARENT_ID {
                // Handle Go menu navigation actions - only when main window is focused
                // This prevents shortcuts from affecting main window when viewer is open
                if let Some(main_window) = app.get_webview_window("main")
                    && main_window.is_focused().unwrap_or(false)
                {
                    let action = match id {
                        GO_BACK_ID => "back",
                        GO_FORWARD_ID => "forward",
                        GO_PARENT_ID => "parent",
                        _ => return,
                    };
                    let _ = app.emit_to("main", "navigation-action", serde_json::json!({ "action": action }));
                }
            } else if id == ABOUT_ID {
                // Emit event to show our custom About window (main window only)
                let _ = app.emit_to("main", "show-about", ());
            } else if id == ENTER_LICENSE_KEY_ID {
                // Emit event to show the license key entry dialog (main window only)
                let _ = app.emit_to("main", "show-license-key-dialog", ());
            } else if id == SETTINGS_ID {
                // Open settings window (emits to main window to handle)
                let _ = app.emit_to("main", "open-settings", ());
            } else if id == COMMAND_PALETTE_ID {
                // Emit event to show the command palette (main window only)
                let _ = app.emit_to("main", "show-command-palette", ());
            } else if id == SWITCH_PANE_ID {
                // Emit event to switch pane (main window only)
                let _ = app.emit_to("main", "switch-pane", ());
            } else if id == RENAME_ID {
                // Emit event to start rename (main window only, when focused)
                if let Some(main_window) = app.get_webview_window("main")
                    && main_window.is_focused().unwrap_or(false)
                {
                    let _ = app.emit_to("main", "start-rename", ());
                }
            } else if id == SWAP_PANES_ID {
                // Emit event to swap panes (main window only)
                let _ = app.emit_to("main", "swap-panes", ());
            } else if id == SORT_BY_NAME_ID
                || id == SORT_BY_EXTENSION_ID
                || id == SORT_BY_SIZE_ID
                || id == SORT_BY_MODIFIED_ID
                || id == SORT_BY_CREATED_ID
            {
                // Handle sort by column
                let column = match id {
                    SORT_BY_NAME_ID => "name",
                    SORT_BY_EXTENSION_ID => "extension",
                    SORT_BY_SIZE_ID => "size",
                    SORT_BY_MODIFIED_ID => "modified",
                    SORT_BY_CREATED_ID => "created",
                    _ => return,
                };
                let _ = app.emit_to(
                    "main",
                    "menu-sort",
                    serde_json::json!({ "action": "sortBy", "value": column }),
                );
            } else if id == SORT_ASCENDING_ID || id == SORT_DESCENDING_ID {
                // Handle sort order (main window only)
                let order = if id == SORT_ASCENDING_ID { "asc" } else { "desc" };
                let _ = app.emit_to(
                    "main",
                    "menu-sort",
                    serde_json::json!({ "action": "sortOrder", "value": order }),
                );
            } else if id == VIEWER_WORD_WRAP_ID {
                // Toggle word wrap on the focused viewer window (if any).
                // Safe: unwrap_or(false) handles destroyed windows, `let _ =` ignores emit errors.
                for (label, window) in app.webview_windows() {
                    if label.starts_with("viewer-") && window.is_focused().unwrap_or(false) {
                        let _ = app.emit_to(&label, "viewer-word-wrap-toggled", ());
                        break;
                    }
                }
            } else {
                // Handle file actions
                commands::ui::execute_menu_action(app, id);
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::file_system::list_directory_start,
            commands::file_system::list_directory_start_streaming,
            commands::file_system::cancel_listing,
            commands::file_system::list_directory_end,
            commands::file_system::get_file_range,
            commands::file_system::get_file_at,
            commands::file_system::get_total_count,
            commands::file_system::get_max_filename_width,
            commands::file_system::find_file_index,
            commands::file_system::resort_listing,
            commands::file_system::path_exists,
            commands::file_system::create_directory,
            commands::file_system::benchmark_log,
            commands::file_system::copy_files,
            commands::file_system::move_files,
            commands::file_system::delete_files,
            commands::file_system::cancel_write_operation,
            commands::file_system::start_scan_preview,
            commands::file_system::cancel_scan_preview,
            commands::file_system::resolve_write_conflict,
            commands::file_system::list_active_operations,
            commands::file_system::get_operation_status,
            // Unified volume copy commands
            commands::file_system::copy_between_volumes,
            commands::file_system::scan_volume_for_copy,
            commands::file_system::scan_volume_for_conflicts,
            commands::file_system::get_listing_stats,
            commands::file_system::start_selection_drag,
            commands::file_system::prepare_self_drag_overlay,
            commands::file_system::clear_self_drag_overlay,
            // Rename operations
            commands::rename::check_rename_permission,
            commands::rename::check_rename_validity,
            commands::rename::rename_file,
            commands::rename::move_to_trash,
            commands::file_viewer::viewer_open,
            commands::file_viewer::viewer_get_lines,
            commands::file_viewer::viewer_get_status,
            commands::file_viewer::viewer_search_start,
            commands::file_viewer::viewer_search_poll,
            commands::file_viewer::viewer_search_cancel,
            commands::file_viewer::viewer_close,
            commands::file_viewer::viewer_setup_menu,
            commands::file_viewer::viewer_set_word_wrap,
            commands::font_metrics::store_font_metrics,
            commands::font_metrics::has_font_metrics,
            commands::icons::get_icons,
            commands::icons::refresh_directory_icons,
            commands::icons::clear_extension_icon_cache,
            commands::ui::show_file_context_menu,
            commands::ui::show_main_window,
            commands::ui::update_menu_context,
            commands::ui::toggle_hidden_files,
            commands::ui::set_view_mode,
            commands::ui::show_in_finder,
            commands::ui::copy_to_clipboard,
            commands::ui::quick_look,
            commands::ui::get_info,
            commands::ui::open_in_editor,
            mcp::pane_state::update_left_pane_state,
            mcp::pane_state::update_right_pane_state,
            mcp::pane_state::update_focused_pane,
            mcp::dialog_state::notify_dialog_opened,
            mcp::dialog_state::notify_dialog_closed,
            mcp::dialog_state::register_known_dialogs,
            mcp::settings_state::mcp_update_settings_state,
            mcp::settings_state::mcp_update_settings_open,
            mcp::settings_state::mcp_update_settings_section,
            mcp::settings_state::mcp_update_settings_sections,
            mcp::settings_state::mcp_update_current_settings,
            mcp::settings_state::mcp_update_shortcuts,
            // Sync status (macOS uses real implementation, others use stub in commands)
            commands::sync_status::get_sync_status,
            // MTP commands (macOS only - Android device support)
            #[cfg(target_os = "macos")]
            commands::mtp::list_mtp_devices,
            #[cfg(target_os = "macos")]
            commands::mtp::connect_mtp_device,
            #[cfg(target_os = "macos")]
            commands::mtp::disconnect_mtp_device,
            #[cfg(target_os = "macos")]
            commands::mtp::get_mtp_device_info,
            #[cfg(target_os = "macos")]
            commands::mtp::get_ptpcamerad_workaround_command,
            #[cfg(target_os = "macos")]
            commands::mtp::get_mtp_storages,
            #[cfg(target_os = "macos")]
            commands::mtp::list_mtp_directory,
            #[cfg(target_os = "macos")]
            commands::mtp::download_mtp_file,
            #[cfg(target_os = "macos")]
            commands::mtp::upload_to_mtp,
            #[cfg(target_os = "macos")]
            commands::mtp::delete_mtp_object,
            #[cfg(target_os = "macos")]
            commands::mtp::create_mtp_folder,
            #[cfg(target_os = "macos")]
            commands::mtp::rename_mtp_object,
            #[cfg(target_os = "macos")]
            commands::mtp::move_mtp_object,
            #[cfg(target_os = "macos")]
            commands::mtp::scan_mtp_for_copy,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::list_mtp_devices,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::connect_mtp_device,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::disconnect_mtp_device,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::get_mtp_device_info,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::get_ptpcamerad_workaround_command,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::get_mtp_storages,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::list_mtp_directory,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::download_mtp_file,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::upload_to_mtp,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::delete_mtp_object,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::create_mtp_folder,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::rename_mtp_object,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::move_mtp_object,
            #[cfg(not(target_os = "macos"))]
            stubs::mtp::scan_mtp_for_copy,
            // Volume commands (platform-specific)
            #[cfg(target_os = "macos")]
            commands::volumes::list_volumes,
            #[cfg(target_os = "macos")]
            commands::volumes::get_default_volume_id,
            #[cfg(target_os = "macos")]
            commands::volumes::find_containing_volume,
            #[cfg(target_os = "macos")]
            commands::volumes::get_volume_space,
            #[cfg(not(target_os = "macos"))]
            stubs::volumes::list_volumes,
            #[cfg(not(target_os = "macos"))]
            stubs::volumes::get_default_volume_id,
            #[cfg(not(target_os = "macos"))]
            stubs::volumes::find_containing_volume,
            #[cfg(not(target_os = "macos"))]
            stubs::volumes::get_volume_space,
            // Network commands (platform-specific)
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
            #[cfg(not(target_os = "macos"))]
            stubs::network::list_network_hosts,
            #[cfg(not(target_os = "macos"))]
            stubs::network::get_network_discovery_state,
            #[cfg(not(target_os = "macos"))]
            stubs::network::resolve_host,
            #[cfg(not(target_os = "macos"))]
            stubs::network::list_shares_on_host,
            #[cfg(not(target_os = "macos"))]
            stubs::network::prefetch_shares,
            #[cfg(not(target_os = "macos"))]
            stubs::network::get_host_auth_mode,
            #[cfg(not(target_os = "macos"))]
            stubs::network::get_known_shares,
            #[cfg(not(target_os = "macos"))]
            stubs::network::get_known_share_by_name,
            #[cfg(not(target_os = "macos"))]
            stubs::network::update_known_share,
            #[cfg(not(target_os = "macos"))]
            stubs::network::get_username_hints,
            #[cfg(not(target_os = "macos"))]
            stubs::network::save_smb_credentials,
            #[cfg(not(target_os = "macos"))]
            stubs::network::get_smb_credentials,
            #[cfg(not(target_os = "macos"))]
            stubs::network::has_smb_credentials,
            #[cfg(not(target_os = "macos"))]
            stubs::network::delete_smb_credentials,
            #[cfg(not(target_os = "macos"))]
            stubs::network::list_shares_with_credentials,
            #[cfg(not(target_os = "macos"))]
            stubs::network::mount_network_share,
            // Accent color command (macOS reads system color, others return fallback)
            #[cfg(target_os = "macos")]
            accent_color::get_accent_color,
            #[cfg(not(target_os = "macos"))]
            stubs::accent_color::get_accent_color,
            // Permission commands (platform-specific)
            #[cfg(target_os = "macos")]
            permissions::check_full_disk_access,
            #[cfg(target_os = "macos")]
            permissions::open_privacy_settings,
            #[cfg(not(target_os = "macos"))]
            stubs::permissions::check_full_disk_access,
            #[cfg(not(target_os = "macos"))]
            stubs::permissions::open_privacy_settings,
            // Licensing commands
            commands::licensing::get_license_status,
            commands::licensing::get_window_title,
            commands::licensing::activate_license,
            commands::licensing::get_license_info,
            commands::licensing::mark_expiration_modal_shown,
            commands::licensing::mark_commercial_reminder_dismissed,
            commands::licensing::reset_license,
            commands::licensing::needs_license_validation,
            commands::licensing::validate_license_with_server,
            // AI commands
            ai::manager::get_ai_status,
            ai::manager::get_ai_model_info,
            ai::manager::start_ai_download,
            ai::manager::cancel_ai_download,
            ai::manager::dismiss_ai_offer,
            ai::manager::uninstall_ai,
            ai::manager::opt_out_ai,
            ai::manager::opt_in_ai,
            ai::manager::is_ai_opted_out,
            ai::suggestions::get_folder_suggestions,
            // Settings commands
            commands::settings::check_port_available,
            commands::settings::find_available_port,
            commands::settings::update_file_watcher_debounce,
            commands::settings::update_service_resolve_timeout,
            commands::settings::update_menu_accelerator,
            // Drive indexing commands
            commands::indexing::start_drive_index,
            commands::indexing::stop_drive_index,
            commands::indexing::get_index_status,
            commands::indexing::get_dir_stats,
            commands::indexing::get_dir_stats_batch,
            commands::indexing::prioritize_dir,
            commands::indexing::cancel_nav_priority,
            commands::indexing::clear_drive_index,
            commands::indexing::set_indexing_enabled,
        ])
        .on_window_event(|window, event| {
            // When the main window is closed, quit the entire app (including settings/debug/viewer windows)
            if let tauri::WindowEvent::CloseRequested { .. } = event
                && window.label() == "main"
            {
                ai::manager::shutdown();
                #[cfg(target_os = "macos")]
                network::mdns_discovery::stop_discovery();
                window.app_handle().exit(0);
            }
            // Clean up app-wide resources only when the main window is destroyed
            if let tauri::WindowEvent::Destroyed = event
                && window.label() == "main"
            {
                ai::manager::shutdown();
                #[cfg(target_os = "macos")]
                network::mdns_discovery::stop_discovery();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
