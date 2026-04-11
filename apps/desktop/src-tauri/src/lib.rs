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
use mimalloc as _;
//noinspection RsUnusedImport
use notify as _;
//noinspection RsUnusedImport
// drag is used by tauri-plugin-drag for drag-and-drop support
use drag as _;
//noinspection ALL
// smb2 crate is used in network/smb_client module (macOS + Linux)
#[cfg(any(target_os = "macos", target_os = "linux"))]
use smb2 as _;

//noinspection ALL
// trash crate is used in write_operations/trash.rs (Linux only)
#[cfg(target_os = "linux")]
use trash as _;

//noinspection ALL
// keyring crate is used in network/keychain_linux.rs for credential storage (Linux only)
#[cfg(target_os = "linux")]
use keyring as _;
//noinspection ALL
// cocoon is used in network/keychain_linux.rs for encrypted file-based credential fallback
#[cfg(target_os = "linux")]
use cocoon as _;

//noinspection ALL
// MCP Bridge is only used in debug builds, so silence the warning in release builds
#[cfg(not(debug_assertions))]
use tauri_plugin_mcp_bridge as _;
//noinspection ALL
// tauri_plugin_updater is only registered on non-macOS (custom updater handles macOS)
#[cfg(target_os = "macos")]
use tauri_plugin_updater as _;
//noinspection ALL
// security_framework is used in network/keychain.rs for Keychain integration
#[cfg(target_os = "macos")]
use security_framework as _;
//noinspection ALL
// mtp-rs is used in mtp/ module for Android device support (macOS + Linux)
#[cfg(any(target_os = "macos", target_os = "linux"))]
use mtp_rs as _;
//noinspection ALL
// nusb is used in mtp/watcher.rs for USB hotplug detection
#[cfg(any(target_os = "macos", target_os = "linux"))]
use nusb as _;

mod ignore_poison;
pub use ignore_poison::IgnorePoison;

#[cfg(target_os = "macos")]
mod accent_color;
#[cfg(target_os = "linux")]
mod accent_color_linux;
mod ai;
pub mod benchmark;
mod clipboard;
mod commands;
pub mod config;
mod crash_reporter;
#[cfg(target_os = "macos")]
mod drag_image_detection;
#[cfg(target_os = "macos")]
mod drag_image_swap;
mod file_system;
pub(crate) mod file_viewer;
mod font_metrics;
pub mod icons;
pub mod indexing;
pub mod licensing;
#[cfg(target_os = "linux")]
pub(crate) mod linux_distro;
#[cfg(target_os = "linux")]
mod linux_icons;
#[cfg(target_os = "macos")]
mod macos_icons;
mod mcp;
mod menu;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod mtp;
mod net;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network;
#[cfg(target_os = "macos")]
mod permissions;
#[cfg(target_os = "linux")]
mod permissions_linux;
pub mod search;
mod settings;
#[cfg(target_os = "macos")]
mod updater;
mod volume_broadcast;
#[cfg(target_os = "macos")]
mod volumes;
#[cfg(target_os = "linux")]
mod volumes_linux;

// Non-macOS stubs (Linux has real implementations for everything;
// other platforms use stubs for all platform-specific features)
#[cfg(not(target_os = "macos"))]
mod stubs;

use menu::{
    CLOSE_TAB_ID, CommandScope, EDIT_COPY_ID, EDIT_CUT_ID, EDIT_PASTE_ID, MenuState, NETWORK_HOST_DISCONNECT_ID,
    NETWORK_HOST_FORGET_PASSWORD_ID, NETWORK_HOST_FORGET_SERVER_ID, SHOW_HIDDEN_FILES_ID, SORT_ASCENDING_ID,
    SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID, SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID, SORT_BY_SIZE_ID,
    SORT_DESCENDING_ID, TAB_CLOSE_ID, TAB_CLOSE_OTHERS_ID, TAB_PIN_ID, VIEW_MODE_BRIEF_ID, VIEW_MODE_FULL_ID,
    VIEWER_WORD_WRAP_ID, ViewMode, menu_id_to_command,
};
use tauri::{Emitter, Manager};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Sends a native clipboard action (copy:/cut:/paste:) through the responder chain.
///
/// Used when a non-main window is focused: the custom Edit menu items can't use the native
/// responder chain like PredefinedMenuItems do, so we replicate it manually via
/// `NSApplication.sendAction:to:from:` with nil target (routes to the first responder).
#[cfg(target_os = "macos")]
fn send_native_clipboard_action(menu_id: &str) {
    use objc2::sel;
    use objc2_app_kit::NSApplication;

    let selector = match menu_id {
        EDIT_CUT_ID => sel!(cut:),
        EDIT_COPY_ID => sel!(copy:),
        EDIT_PASTE_ID => sel!(paste:),
        _ => return,
    };

    let mtm = objc2::MainThreadMarker::new().expect("send_native_clipboard_action must be called from the main thread");
    let ns_app = NSApplication::sharedApplication(mtm);

    // sendAction:to:from: with nil `to` sends to the first responder, exactly like
    // PredefinedMenuItems do internally. This lets WKWebView handle text clipboard natively.
    unsafe {
        let _: bool = objc2::msg_send![
            &ns_app,
            sendAction: selector,
            to: std::ptr::null::<objc2::runtime::AnyObject>(),
            from: std::ptr::null::<objc2::runtime::AnyObject>(),
        ];
    }
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

    // CrabNebula automation plugin for macOS E2E testing (feature-gated, never in release builds)
    #[cfg(feature = "automation")]
    let builder = builder.plugin(tauri_plugin_automation::init());

    // Playwright E2E testing plugin — socket bridge for direct webview injection
    #[cfg(feature = "playwright-e2e")]
    let builder = builder.plugin(tauri_plugin_playwright::init());

    // Skip Tauri updater plugin on macOS (custom updater preserves TCC permissions)
    // and in CI (avoids network dependency and latency during E2E tests)
    #[cfg(not(target_os = "macos"))]
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
        .plugin({
            use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

            fn parse_level_filter(s: &str) -> Option<log::LevelFilter> {
                match s.to_lowercase().as_str() {
                    "trace" => Some(log::LevelFilter::Trace),
                    "debug" => Some(log::LevelFilter::Debug),
                    "info" => Some(log::LevelFilter::Info),
                    "warn" | "warning" => Some(log::LevelFilter::Warn),
                    "error" => Some(log::LevelFilter::Error),
                    "off" => Some(log::LevelFilter::Off),
                    _ => None,
                }
            }

            // Log directory priority:
            // 1. CMDR_LOG_DIR env var (explicit override)
            // 2. CMDR_DATA_DIR env var → <CMDR_DATA_DIR>/logs/ (dev and E2E test isolation)
            // 3. Default Tauri log dir (production)
            let log_target = if let Ok(log_dir) = std::env::var("CMDR_LOG_DIR") {
                Target::new(TargetKind::Folder {
                    path: std::path::PathBuf::from(log_dir),
                    file_name: None,
                })
            } else if let Ok(data_dir) = std::env::var("CMDR_DATA_DIR") {
                let log_dir = std::path::PathBuf::from(data_dir).join("logs");
                Target::new(TargetKind::Folder {
                    path: log_dir,
                    file_name: None,
                })
            } else {
                Target::new(TargetKind::LogDir { file_name: None })
            };

            let mut builder = tauri_plugin_log::Builder::new()
                .targets([Target::new(TargetKind::Stdout), log_target])
                .rotation_strategy(RotationStrategy::KeepAll)
                .max_file_size(50_000_000) // 50 MB
                .format(|out, message, record| {
                    let now = chrono::Local::now();
                    let ts = now.format("%H:%M:%S%.3f"); // HH:MM:SS.mmm
                    let target = record.target().strip_prefix("cmdr_lib::").unwrap_or(record.target());
                    let level = record.level();
                    let color = match level {
                        log::Level::Error => "\x1b[31m", // red
                        log::Level::Warn => "\x1b[33m",  // yellow
                        log::Level::Info => "\x1b[32m",  // green
                        log::Level::Debug => "\x1b[36m", // cyan
                        log::Level::Trace => "\x1b[35m", // magenta
                    };
                    out.finish(format_args!("{ts} {color}{level:<5}\x1b[0m {target}  {message}"))
                })
                .level_for("nusb", log::LevelFilter::Warn)
                .level_for("zbus", log::LevelFilter::Warn)
                .level_for("tracing::span", log::LevelFilter::Warn)
                .level_for("smb2", log::LevelFilter::Warn)
                .level_for("tao", log::LevelFilter::Warn);

            // Parse RUST_LOG env var for per-module level overrides
            if let Ok(rust_log) = std::env::var("RUST_LOG") {
                let mut base_level = log::LevelFilter::Info;
                for directive in rust_log.split(',') {
                    let directive = directive.trim();
                    if directive.is_empty() {
                        continue;
                    }
                    if let Some((module, level_str)) = directive.split_once('=') {
                        if let Some(level) = parse_level_filter(level_str) {
                            builder = builder.level_for(module.to_string(), level);
                        }
                    } else if let Some(level) = parse_level_filter(directive) {
                        base_level = level;
                    }
                }
                builder = builder.level(base_level);
            } else {
                // No RUST_LOG: use Trace so set_max_level() controls filtering (enables verbose toggle)
                builder = builder.level(log::LevelFilter::Trace);
            }

            builder.build()
        })
        .setup(|app| {
            // When RUST_LOG is not set, restrict to Info by default (verbose toggle can raise to Debug)
            if std::env::var("RUST_LOG").is_err() {
                log::set_max_level(log::LevelFilter::Info);
            }

            // Initialize crash reporter early, before anything that might crash
            crash_reporter::init(app.handle());

            // Log the resolved app data directory (shows -dev suffix in debug builds)
            config::log_app_data_dir(app.handle());

            // Initialize benchmarking (enabled by RUSTY_COMMANDER_BENCHMARK=1)
            benchmark::init_benchmarking();

            // Initialize the file watcher manager with app handle for events
            file_system::init_watcher_manager(app.handle().clone());

            // Initialize the volume manager with the root volume
            file_system::init_volume_manager();

            // Start network host discovery (mDNS)
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::start_discovery(app.handle().clone());

            // Register virtual SMB hosts for E2E testing (after discovery start so they appear alongside real hosts)
            #[cfg(feature = "smb-e2e")]
            network::virtual_smb_hosts::setup_virtual_smb_hosts(app.handle());

            // Initialize volume broadcast (must be before watchers so they can emit)
            volume_broadcast::init(app.handle());

            // Start volume mount/unmount watcher
            #[cfg(target_os = "macos")]
            volumes::watcher::start_volume_watcher(app.handle());

            #[cfg(target_os = "linux")]
            volumes_linux::watcher::start_volume_watcher(app.handle());

            // Register virtual MTP device for E2E testing (before watcher so it's in the initial snapshot)
            #[cfg(feature = "virtual-mtp")]
            mtp::virtual_device::setup_virtual_mtp_device();

            // Ensure ptpcamerad is re-enabled in case a previous session crashed
            // while it was suppressed. No-op if it was already enabled.
            #[cfg(target_os = "macos")]
            mtp::macos_workaround::ensure_ptpcamerad_enabled();

            // Load persisted settings early so MTP enabled flag is set before the watcher starts
            let saved_settings = settings::load_settings(app.handle());

            // Apply MTP enabled setting (default: true) before starting the watcher
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            mtp::set_mtp_enabled_flag(saved_settings.mtp_enabled.unwrap_or(true));

            // Start MTP device hotplug watcher (Android device support)
            // This also auto-connects any devices already plugged in at startup
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            mtp::start_mtp_watcher(app.handle());

            // Emit initial volume list (after watchers start so MTP devices can connect)
            volume_broadcast::emit_volumes_changed_now();

            // Load known network shares from disk
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::known_shares::load_known_shares(app.handle());

            // Load manually-added servers and inject into discovery state
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::manual_servers::load_manual_servers(app.handle());

            // Drag image detection swizzle is installed in RunEvent::Ready (not here)
            // because wry 0.54+ registers the WryWebView ObjC class lazily — it doesn't
            // exist in the runtime until the first webview is created, which happens after
            // setup() returns.

            // Observe system accent color changes and emit events to frontend
            #[cfg(target_os = "macos")]
            accent_color::observe_accent_color_changes(app.handle().clone());
            #[cfg(target_os = "linux")]
            accent_color_linux::observe_accent_color_changes(app.handle().clone());

            // Initialize font metrics for default font (system font at 12px)
            font_metrics::init_font_metrics(app.handle(), "system-400-12");

            // Apply direct SMB connection setting (default: true)
            file_system::set_direct_smb_enabled(saved_settings.direct_smb_connection.unwrap_or(true));
            file_system::set_filter_safe_save_artifacts(saved_settings.filter_safe_save_artifacts.unwrap_or(true));

            // Upgrade existing SMB mounts to direct smb2 connections (background, non-blocking)
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            file_system::upgrade_existing_smb_mounts();

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

            // Remove macOS system-injected Edit menu items and register Help menu for search
            #[cfg(target_os = "macos")]
            menu::cleanup_macos_menus();

            // Set SF Symbol icons on menu items (macOS only)
            #[cfg(target_os = "macos")]
            menu::set_macos_menu_icons();

            // Store the CheckMenuItem references in app state
            let menu_state = MenuState::default();
            *menu_state.show_hidden_files.lock_ignore_poison() = Some(menu_items.show_hidden_files);
            *menu_state.view_mode_full.lock_ignore_poison() = Some(menu_items.view_mode_full);
            *menu_state.view_mode_brief.lock_ignore_poison() = Some(menu_items.view_mode_brief);
            *menu_state.view_submenu.lock_ignore_poison() = Some(menu_items.view_submenu);
            *menu_state.view_mode_full_position.lock_ignore_poison() = menu_items.view_mode_full_position;
            *menu_state.view_mode_brief_position.lock_ignore_poison() = menu_items.view_mode_brief_position;
            *menu_state.pin_tab.lock_ignore_poison() = Some(menu_items.pin_tab);
            *menu_state.items.lock_ignore_poison() = menu_items.items;
            *menu_state.sort_submenu.lock_ignore_poison() = Some(menu_items.sort_submenu);
            app.manage(menu_state);

            // Set window title based on license status
            let license_status = licensing::get_app_status(app.handle());
            let title = licensing::get_window_title(&license_status);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(&title);
            }

            // titleBarStyle is "Overlay" in JSON for macOS (needed so trafficLightPosition
            // is applied at window creation time — setting it at runtime resets the position).
            // On Linux/GTK, Overlay hides native window controls, so revert to Visible.
            #[cfg(target_os = "linux")]
            if let Some(window) = app.get_webview_window("main") {
                use tauri::TitleBarStyle;
                let _ = window.set_title_bar_style(TitleBarStyle::Visible);
            }

            // Initialize custom updater state (shared between download and install commands)
            #[cfg(target_os = "macos")]
            app.manage(updater::UpdateState::new());

            // Initialize pane state store for MCP context tools
            app.manage(mcp::PaneStateStore::new());

            // Initialize soft dialog tracker for MCP (overlays like about, license, confirmations)
            app.manage(mcp::SoftDialogTracker::new());

            // Start MCP server for AI agent integration
            // Use settings from user preferences, with env vars as override for dev
            let mcp_config = mcp::McpConfig::from_settings_and_env(
                saved_settings.developer_mcp_enabled,
                saved_settings.developer_mcp_port,
            );
            mcp::start_mcp_server_background(app.handle().clone(), mcp_config);

            // Initialize AI manager (starts llama-server if model is installed)
            ai::manager::init(app.handle());

            // Initialize indexing state (does not start scanning until explicitly started)
            indexing::init();

            // Auto-start indexing unless user disabled it in settings
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
                log::info!("Drive indexing auto-start skipped (disabled in settings)");
            }

            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();

            // === CheckMenuItem exceptions: sync checked state and emit directly ===
            // These must NOT go through "execute-command" — that would double-toggle.
            if id == SHOW_HIDDEN_FILES_ID {
                let menu_state = app.state::<MenuState<tauri::Wry>>();
                let guard = menu_state.show_hidden_files.lock_ignore_poison();
                if let Some(check_item) = guard.as_ref() {
                    let new_state = check_item.is_checked().unwrap_or(true);
                    let _ = app.emit_to(
                        "main",
                        "settings-changed",
                        serde_json::json!({ "showHiddenFiles": new_state }),
                    );
                }
                return;
            }
            if id == VIEW_MODE_FULL_ID || id == VIEW_MODE_BRIEF_ID {
                let menu_state = app.state::<MenuState<tauri::Wry>>();
                let (full_guard, brief_guard) = (
                    menu_state.view_mode_full.lock_ignore_poison(),
                    menu_state.view_mode_brief.lock_ignore_poison(),
                );
                if let (Some(full_item), Some(brief_item)) = (full_guard.as_ref(), brief_guard.as_ref()) {
                    let is_full = id == VIEW_MODE_FULL_ID;
                    let _ = full_item.set_checked(is_full);
                    let _ = brief_item.set_checked(!is_full);
                    let mode = if is_full { "full" } else { "brief" };
                    let _ = app.emit_to("main", "view-mode-changed", serde_json::json!({ "mode": mode }));
                }
                return;
            }

            // === Close-tab exception: close focused non-main window, or emit tab.close ===
            if id == CLOSE_TAB_ID {
                if let Some(main_window) = app.get_webview_window("main")
                    && main_window.is_focused().unwrap_or(false)
                {
                    let _ = app.emit_to(
                        "main",
                        "execute-command",
                        serde_json::json!({ "commandId": "tab.close" }),
                    );
                } else {
                    for (_label, window) in app.webview_windows() {
                        if window.is_focused().unwrap_or(false) {
                            let _ = window.close();
                            break;
                        }
                    }
                }
                return;
            }

            // === Viewer word wrap: emit to the focused viewer window ===
            if id == VIEWER_WORD_WRAP_ID {
                for (label, window) in app.webview_windows() {
                    if label.starts_with("viewer-") && window.is_focused().unwrap_or(false) {
                        let _ = app.emit_to(&label, "viewer-word-wrap-toggled", ());
                        break;
                    }
                }
                return;
            }

            // === Sort items: emit menu-sort directly (frontend has a dedicated listener) ===
            if id == SORT_BY_NAME_ID
                || id == SORT_BY_EXTENSION_ID
                || id == SORT_BY_SIZE_ID
                || id == SORT_BY_MODIFIED_ID
                || id == SORT_BY_CREATED_ID
            {
                let column = match id {
                    SORT_BY_NAME_ID => "name",
                    SORT_BY_EXTENSION_ID => "extension",
                    SORT_BY_SIZE_ID => "size",
                    SORT_BY_MODIFIED_ID => "modified",
                    _ => "created",
                };
                let _ = app.emit_to(
                    "main",
                    "menu-sort",
                    serde_json::json!({ "action": "sortBy", "value": column }),
                );
                return;
            }
            if id == SORT_ASCENDING_ID || id == SORT_DESCENDING_ID {
                let order = if id == SORT_ASCENDING_ID { "asc" } else { "desc" };
                let _ = app.emit_to(
                    "main",
                    "menu-sort",
                    serde_json::json!({ "action": "sortOrder", "value": order }),
                );
                return;
            }

            // === Tab context menu actions: emit tab-context-action directly ===
            if id == TAB_PIN_ID || id == TAB_CLOSE_OTHERS_ID || id == TAB_CLOSE_ID {
                let _ = app.emit_to("main", "tab-context-action", serde_json::json!({ "action": id }));
                return;
            }

            // === Network host context menu actions ===
            if id == NETWORK_HOST_FORGET_SERVER_ID
                || id == NETWORK_HOST_FORGET_PASSWORD_ID
                || id == NETWORK_HOST_DISCONNECT_ID
            {
                let menu_state = app.state::<MenuState<tauri::Wry>>();
                let ctx = menu_state.network_host_context.lock_ignore_poison();
                let action = if id == NETWORK_HOST_FORGET_SERVER_ID {
                    "forget-server"
                } else if id == NETWORK_HOST_FORGET_PASSWORD_ID {
                    "forget-password"
                } else {
                    "disconnect"
                };
                let _ = app.emit_to(
                    "main",
                    "network-host-context-action",
                    serde_json::json!({
                        "action": action,
                        "hostId": ctx.host_id,
                        "hostName": ctx.host_name,
                    }),
                );
                return;
            }

            // === Clipboard exception: file clipboard in main window, native text clipboard elsewhere ===
            // Custom MenuItems for Cut/Copy/Paste route through execute-command in the main window
            // so the frontend can decide between file and text clipboard. In non-main windows
            // (viewer, settings), we send the native action through the responder chain so
            // WKWebView handles text clipboard natively — just like PredefinedMenuItems would.
            if id == EDIT_CUT_ID || id == EDIT_COPY_ID || id == EDIT_PASTE_ID {
                let main_focused = app
                    .get_webview_window("main")
                    .is_some_and(|w| w.is_focused().unwrap_or(false));
                if main_focused {
                    let command_id = match id {
                        EDIT_CUT_ID => "edit.cut",
                        EDIT_COPY_ID => "edit.copy",
                        _ => "edit.paste",
                    };
                    let _ = app.emit_to(
                        "main",
                        "execute-command",
                        serde_json::json!({ "commandId": command_id }),
                    );
                } else {
                    // Send native clipboard action to the first responder chain
                    #[cfg(target_os = "macos")]
                    send_native_clipboard_action(id);
                }
                return;
            }

            // === Unified dispatch: look up command ID from the mapping ===
            if let Some((command_id, scope)) = menu_id_to_command(id) {
                if scope == CommandScope::FileScoped {
                    // Focus guard: only emit if main window has focus
                    let focused = app
                        .get_webview_window("main")
                        .is_some_and(|w| w.is_focused().unwrap_or(false));
                    if !focused {
                        return;
                    }
                }
                let _ = app.emit_to(
                    "main",
                    "execute-command",
                    serde_json::json!({ "commandId": command_id }),
                );
            }

            // Unknown menu ID — no-op (all known IDs are handled above)
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::file_system::list_directory_start,
            commands::file_system::list_directory_start_streaming,
            commands::file_system::cancel_listing,
            commands::file_system::list_directory_end,
            commands::file_system::refresh_listing,
            commands::file_system::get_file_range,
            commands::file_system::get_file_at,
            commands::file_system::get_total_count,
            commands::file_system::get_max_filename_width,
            commands::file_system::find_file_index,
            commands::file_system::find_file_indices,
            commands::file_system::resort_listing,
            commands::file_system::get_path_limits,
            commands::file_system::path_exists,
            commands::file_system::create_directory,
            commands::file_system::create_file,
            commands::file_system::benchmark_log,
            commands::file_system::copy_files,
            commands::file_system::move_files,
            commands::file_system::delete_files,
            commands::file_system::trash_files,
            commands::file_system::cancel_write_operation,
            commands::file_system::cancel_all_write_operations,
            commands::file_system::start_scan_preview,
            commands::file_system::cancel_scan_preview,
            commands::file_system::check_scan_preview_status,
            commands::file_system::resolve_write_conflict,
            commands::file_system::list_active_operations,
            commands::file_system::get_operation_status,
            // Unified volume copy commands
            commands::file_system::copy_between_volumes,
            commands::file_system::move_between_volumes,
            commands::file_system::scan_volume_for_copy,
            commands::file_system::scan_volume_for_conflicts,
            commands::file_system::get_listing_stats,
            commands::file_system::refresh_listing_index_sizes,
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
            commands::icons::clear_directory_icon_cache,
            commands::ui::show_file_context_menu,
            commands::ui::show_breadcrumb_context_menu,
            commands::ui::show_tab_context_menu,
            commands::ui::show_network_host_context_menu,
            commands::ui::update_pin_tab_menu,
            commands::ui::show_main_window,
            commands::ui::update_menu_context,
            commands::ui::set_menu_context,
            commands::ui::toggle_hidden_files,
            commands::ui::set_view_mode,
            commands::ui::sync_view_mode_menu,
            commands::ui::show_in_finder,
            commands::ui::copy_to_clipboard,
            commands::ui::quick_look,
            commands::ui::get_info,
            commands::ui::open_in_editor,
            mcp::pane_state::update_left_pane_state,
            mcp::pane_state::update_right_pane_state,
            mcp::pane_state::update_focused_pane,
            mcp::pane_state::update_pane_tabs,
            mcp::dialog_state::notify_dialog_opened,
            mcp::dialog_state::notify_dialog_closed,
            mcp::dialog_state::register_known_dialogs,
            // Sync status (macOS uses real implementation, others use stub in commands)
            commands::sync_status::get_sync_status,
            // MTP commands (macOS + Linux - Android device support)
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::set_mtp_enabled,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::list_mtp_devices,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::connect_mtp_device,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::disconnect_mtp_device,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::get_mtp_device_info,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::get_ptpcamerad_workaround_command,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::get_mtp_storages,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::list_mtp_directory,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::download_mtp_file,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::upload_to_mtp,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::delete_mtp_object,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::create_mtp_folder,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::rename_mtp_object,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::move_mtp_object,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::mtp::scan_mtp_for_copy,
            #[cfg(feature = "virtual-mtp")]
            commands::mtp::rescan_virtual_mtp,
            #[cfg(feature = "virtual-mtp")]
            commands::mtp::pause_virtual_mtp_watcher,
            #[cfg(feature = "virtual-mtp")]
            commands::mtp::resume_virtual_mtp_watcher,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::set_mtp_enabled,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::list_mtp_devices,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::connect_mtp_device,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::disconnect_mtp_device,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::get_mtp_device_info,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::get_ptpcamerad_workaround_command,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::get_mtp_storages,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::list_mtp_directory,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::download_mtp_file,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::upload_to_mtp,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::delete_mtp_object,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::create_mtp_folder,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::rename_mtp_object,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::move_mtp_object,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::mtp::scan_mtp_for_copy,
            // Volume broadcast (cross-platform)
            volume_broadcast::refresh_volumes,
            // Volume commands (platform-specific)
            #[cfg(target_os = "macos")]
            commands::volumes::list_volumes,
            #[cfg(target_os = "macos")]
            commands::volumes::get_default_volume_id,
            #[cfg(target_os = "macos")]
            commands::volumes::get_volume_space,
            #[cfg(target_os = "macos")]
            commands::volumes::resolve_path_volume,
            #[cfg(target_os = "linux")]
            commands::volumes_linux::list_volumes,
            #[cfg(target_os = "linux")]
            commands::volumes_linux::get_default_volume_id,
            #[cfg(target_os = "linux")]
            commands::volumes_linux::get_volume_space,
            #[cfg(target_os = "linux")]
            commands::volumes_linux::resolve_path_volume,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::volumes::list_volumes,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::volumes::get_default_volume_id,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::volumes::get_volume_space,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::volumes::resolve_path_volume,
            // Network commands (macOS + Linux, stubs for other platforms)
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::list_network_hosts,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::get_network_discovery_state,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::resolve_host,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::list_shares_on_host,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::prefetch_shares,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::get_host_auth_mode,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::get_known_shares,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::get_known_share_by_name,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::update_known_share,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::get_username_hints,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::save_smb_credentials,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::get_smb_credentials,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::has_smb_credentials,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::delete_smb_credentials,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::is_using_credential_file_fallback,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::list_shares_with_credentials,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::mount_network_share,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::upgrade_to_smb_volume,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::connect_to_server,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::remove_manual_server,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            commands::network::disconnect_network_host,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::list_network_hosts,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::get_network_discovery_state,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::resolve_host,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::list_shares_on_host,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::prefetch_shares,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::get_host_auth_mode,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::get_known_shares,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::get_known_share_by_name,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::update_known_share,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::get_username_hints,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::save_smb_credentials,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::get_smb_credentials,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::has_smb_credentials,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::delete_smb_credentials,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::is_using_credential_file_fallback,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::list_shares_with_credentials,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::mount_network_share,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::upgrade_to_smb_volume,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::connect_to_server,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::remove_manual_server,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::network::disconnect_network_host,
            // Accent color command (macOS reads NSColor, Linux reads gsettings, others return fallback)
            #[cfg(target_os = "macos")]
            accent_color::get_accent_color,
            #[cfg(target_os = "linux")]
            accent_color_linux::get_accent_color,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::accent_color::get_accent_color,
            // Permission commands (platform-specific)
            #[cfg(target_os = "macos")]
            permissions::check_full_disk_access,
            #[cfg(target_os = "macos")]
            permissions::open_privacy_settings,
            #[cfg(target_os = "macos")]
            permissions::open_appearance_settings,
            #[cfg(target_os = "linux")]
            permissions_linux::check_full_disk_access,
            #[cfg(target_os = "linux")]
            permissions_linux::open_privacy_settings,
            #[cfg(target_os = "linux")]
            permissions_linux::open_appearance_settings,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::permissions::check_full_disk_access,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::permissions::open_privacy_settings,
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            stubs::permissions::open_appearance_settings,
            // Crash reporter commands
            commands::crash_reporter::check_pending_crash_report,
            commands::crash_reporter::dismiss_crash_report,
            commands::crash_reporter::send_crash_report,
            // Licensing commands
            commands::licensing::get_license_status,
            commands::licensing::get_window_title,
            commands::licensing::activate_license,
            commands::licensing::verify_license,
            commands::licensing::commit_license,
            commands::licensing::get_license_info,
            commands::licensing::mark_expiration_modal_shown,
            commands::licensing::mark_commercial_reminder_dismissed,
            commands::licensing::reset_license,
            commands::licensing::needs_license_validation,
            commands::licensing::has_license_been_validated,
            commands::licensing::validate_license_with_server,
            // AI commands
            ai::manager::get_ai_status,
            ai::manager::get_ai_model_info,
            ai::manager::get_ai_runtime_status,
            ai::manager::configure_ai,
            ai::manager::start_ai_server,
            ai::manager::stop_ai_server,
            ai::manager::check_ai_connection,
            ai::manager::get_system_memory_info,
            ai::manager::start_ai_download,
            ai::manager::cancel_ai_download,
            ai::manager::dismiss_ai_offer,
            ai::manager::uninstall_ai,
            ai::manager::opt_out_ai,
            ai::manager::opt_in_ai,
            ai::manager::is_ai_opted_out,
            ai::suggestions::get_folder_suggestions,
            // MCP server live control
            commands::mcp::set_mcp_enabled,
            commands::mcp::set_mcp_port,
            commands::mcp::get_mcp_running,
            commands::mcp::get_mcp_port,
            // Settings commands
            commands::settings::check_port_available,
            commands::settings::find_available_port,
            commands::settings::update_file_watcher_debounce,
            commands::settings::update_service_resolve_timeout,
            commands::settings::update_menu_accelerator,
            // Logging commands (frontend log bridge + runtime level control)
            commands::logging::batch_fe_logs,
            commands::logging::set_log_level,
            // Drive indexing commands
            commands::indexing::start_drive_index,
            commands::indexing::stop_drive_index,
            commands::indexing::get_index_status,
            commands::indexing::get_dir_stats,
            commands::indexing::get_dir_stats_batch,
            commands::indexing::clear_drive_index,
            commands::indexing::set_indexing_enabled,
            commands::indexing::get_index_debug_status,
            // Drive search commands
            commands::search::prepare_search_index,
            commands::search::search_files,
            commands::search::release_search_index,
            commands::search::translate_search_query,
            commands::search::parse_search_scope,
            commands::search::get_system_dir_excludes,
            // E2E test support
            commands::e2e::get_e2e_start_path,
            #[cfg(feature = "playwright-e2e")]
            commands::file_system::inject_listing_error,
            // Clipboard file operations
            commands::clipboard::copy_files_to_clipboard,
            commands::clipboard::cut_files_to_clipboard,
            commands::clipboard::read_clipboard_files,
            commands::clipboard::read_clipboard_text,
            commands::clipboard::clear_clipboard_cut_state,
            // Custom updater commands (macOS rsync-into-bundle, preserves TCC permissions)
            #[cfg(target_os = "macos")]
            updater::check_for_update,
            #[cfg(target_os = "macos")]
            updater::download_update,
            #[cfg(target_os = "macos")]
            updater::install_update,
        ])
        .on_window_event(|window, event| {
            // When the main window is closed, quit the entire app (including settings/debug/viewer windows)
            if let tauri::WindowEvent::CloseRequested { .. } = event
                && window.label() == "main"
            {
                ai::manager::shutdown();
                mcp::stop_mcp_server();
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                network::mdns_discovery::stop_discovery();
                window.app_handle().exit(0);
            }
            // Clean up app-wide resources only when the main window is destroyed
            if let tauri::WindowEvent::Destroyed = event
                && window.label() == "main"
            {
                ai::manager::shutdown();
                mcp::stop_mcp_server();
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                network::mdns_discovery::stop_discovery();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            match event {
                tauri::RunEvent::Ready => {
                    // Install drag image detection swizzle. Needs a live webview to
                    // discover wry's ObjC class, so it runs at Ready (not setup).
                    #[cfg(target_os = "macos")]
                    drag_image_detection::install(_app.clone());
                }
                tauri::RunEvent::Exit => {
                    // Restore ptpcamerad before exit so we don't leave the system
                    // with the daemon disabled after Cmdr closes
                    #[cfg(target_os = "macos")]
                    if let Err(e) = mtp::macos_workaround::restore_ptpcamerad() {
                        log::warn!("Failed to restore ptpcamerad on exit: {}", e);
                    }

                    ai::manager::shutdown();
                    mcp::stop_mcp_server();
                    #[cfg(any(target_os = "macos", target_os = "linux"))]
                    network::mdns_discovery::stop_discovery();
                }
                _ => {}
            }
        });
}
