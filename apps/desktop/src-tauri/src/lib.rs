// Deny unused code to catch dead code early (like knip for TS)
#![deny(unused)]
// Warn on unused dependencies to catch platform-specific cfg mismatches
#![warn(unused_crate_dependencies)]
// Warn on redundant path prefixes (like std::path::Path when Path is imported)
#![warn(unused_qualifications)]
// Use log::* macros instead of println!/eprintln!/dbg! for proper log level control
// and so error-report bundles capture the context. See `logging/CLAUDE.md` for the rules.
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Require justification for all #[allow] attributes
#![warn(clippy::allow_attributes_without_reason)]

//noinspection RsUnusedImport
// Silence false positives for dev dependencies (used only in benches/, not lib)
// and transitive dependencies (notify is used by notify-debouncer-full)
#[cfg(test)]
use criterion as _;
//noinspection RsUnusedImport
// Dev-only log-routing shim. Used by the phase4 bench's optional
// `env_logger::try_init()` (commented-in when collecting wire traces) and by
// ad-hoc debug-logging in tests. Harmless otherwise.
#[cfg(test)]
use env_logger as _;
//noinspection RsUnusedImport
use mimalloc as _;
//noinspection RsUnusedImport
use notify as _;
//noinspection ALL
// smb2 crate is used in network/smb_client module (macOS + Linux)
#[cfg(any(target_os = "macos", target_os = "linux"))]
use smb2 as _;

//noinspection ALL
// trash crate is used in write_operations/trash.rs (Linux only)
#[cfg(target_os = "linux")]
use trash as _;

//noinspection ALL
// keyring-core + the zbus secret-service backend are used in secrets/keyring_linux.rs
// for credential storage (Linux only).
#[cfg(target_os = "linux")]
use keyring_core as _;
#[cfg(target_os = "linux")]
use zbus_secret_service_keyring_store as _;
//noinspection ALL
// MCP Bridge is only used in debug builds, so silence the warning in release builds
#[cfg(not(debug_assertions))]
use tauri_plugin_mcp_bridge as _;
//noinspection ALL
// tauri_plugin_updater is only registered on non-macOS (custom updater handles macOS)
#[cfg(target_os = "macos")]
use tauri_plugin_updater as _;
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

mod ipc;

mod logging;

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
mod error_reporter;
mod fda_gate;
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
#[cfg(target_os = "macos")]
mod native_drag;
mod net;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network;
#[cfg(target_os = "macos")]
mod permissions;
#[cfg(target_os = "linux")]
mod permissions_linux;
mod redact;
pub mod search;
mod secrets;
mod settings;
mod space_poller;
mod system_memory;
#[cfg(target_os = "macos")]
mod text_size;
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
    SORT_DESCENDING_ID, TAB_CLOSE_ID, TAB_CLOSE_OTHERS_ID, TAB_PIN_ID, VIEW_MODE_BRIEF_LEFT_ID,
    VIEW_MODE_BRIEF_RIGHT_ID, VIEW_MODE_FULL_LEFT_ID, VIEW_MODE_FULL_RIGHT_ID, VIEWER_WORD_WRAP_ID, ViewMode,
    menu_id_to_command,
};
use tauri::{Emitter, Manager};

// `greet` and the rest of the Tauri command surface live in `ipc.rs`, which
// exposes them through a typed `tauri_specta::Builder`. See `ipc.rs` for the
// migration recipe.

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
    // Type-safe IPC: collect every command and event into a tauri-specta Builder.
    // The same Builder is attached to `tauri::Builder::default()` below.
    // `bindings.ts` is regenerated explicitly via `pnpm bindings:regen` (which
    // invokes the ignored `ipc::tests::export_bindings_test` and post-processes
    // with oxfmt); CI's `bindings-fresh` check fails when it drifts. Don't
    // re-export at runtime — without the test's header + oxfmt postprocess, that
    // path silently overwrites the committed file with raw specta output on
    // every dev launch.
    let specta_builder = ipc::builder();
    let builder = tauri::Builder::default();

    // Window state plugin is only available on desktop platforms
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_window_state::Builder::new().build());

    // MCP Bridge plugin is only available in debug builds for security
    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_mcp_bridge::init());

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
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // === Logging setup ===
            //
            // Hand-rolled fern dispatch tree (`logging::dispatch::init`) replaces
            // `tauri-plugin-log`. Why: per-output level filtering. File target locked at
            // Debug (error reports need the context); stdout defaults to Info (clean for
            // `pnpm dev`) with `RUST_LOG` per-module overrides applied to stdout only.
            // The verbose toggle bumps stdout to Debug via an AtomicU8 — no logger
            // rebuild, no records lost.
            //
            // Log directory priority:
            // 1. CMDR_LOG_DIR env var (explicit override)
            // 2. CMDR_DATA_DIR env var → <CMDR_DATA_DIR>/logs/ (dev and E2E test isolation)
            // 3. Default per-OS app log dir (production)
            let resolved_log_dir: std::path::PathBuf = if let Ok(log_dir) = std::env::var("CMDR_LOG_DIR") {
                std::path::PathBuf::from(log_dir)
            } else if let Ok(data_dir) = std::env::var("CMDR_DATA_DIR") {
                std::path::PathBuf::from(data_dir).join("logs")
            } else {
                #[cfg(target_os = "macos")]
                {
                    dirs::home_dir()
                        .map(|h| h.join("Library/Logs/com.veszelovszki.cmdr"))
                        .unwrap_or_else(|| std::path::PathBuf::from("./logs"))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    dirs::data_local_dir()
                        .map(|d| d.join("com.veszelovszki.cmdr/logs"))
                        .unwrap_or_else(|| std::path::PathBuf::from("./logs"))
                }
            };

            // Read the log-storage cap from settings.json *before* the AppHandle is
            // wired into the rest of setup. 0 = disabled (drop the file chain entirely).
            // None = no setting yet → 200 MB default. Any other value = N MB cap, mapped
            // to keep-N where N = ceil(N / 50).
            let cap_mb = settings::early_load_max_log_storage_mb().unwrap_or(200);
            let file_logging_enabled = cap_mb > 0;
            let keep_count: usize = if file_logging_enabled {
                cap_mb.div_ceil(50) as usize
            } else {
                0
            };

            // Cache for the rest of the app (error-report bundle builder, eager-prune callers).
            logging::set_log_dir(resolved_log_dir.clone());
            logging::set_keep_count(keep_count);

            // Verbose-toggle default: if the saved setting is on, start with stdout at Debug.
            // We have to read settings *before* dispatch::init so the AtomicU8 is set
            // correctly before any logs fire. Use the early-load helper since the full
            // settings load happens later in setup().
            let verbose_default = settings::early_load_verbose_logging().unwrap_or(false);

            let init_result = logging::dispatch::init(logging::dispatch::InitOptions {
                log_dir: file_logging_enabled.then_some(resolved_log_dir),
                keep_count,
                rust_log: std::env::var("RUST_LOG").ok(),
            });
            // Apply verbose default after init (init resets the threshold from RUST_LOG).
            // RUST_LOG always wins — only bump if RUST_LOG didn't set a base level.
            if std::env::var("RUST_LOG").is_err() && verbose_default {
                logging::dispatch::set_stdout_threshold(log::LevelFilter::Debug);
            }
            if let Err(err) = init_result {
                // Don't panic — a logger collision (rare; tests, double-init) is recoverable.
                // The `log` macros become no-ops, which is exactly the behavior callers expect
                // when no logger is registered. Write directly to stderr; we don't have a
                // logger to fall back to.
                use std::io::Write as _;
                let _ = writeln!(std::io::stderr(), "Failed to install fern logger: {err}");
            }

            // One-shot startup sweep: pre-`319d5d37` `tauri-plugin-log` left rotated files
            // named `Cmdr_<timestamp>.log` behind. Idempotent. Logs INFO per file removed.
            if let Some(dir) = logging::log_dir() {
                logging::cleanup_legacy_log_files(dir);
            }

            // One-line marker so the resolved log-storage state is visible at startup.
            match logging::keep_count() {
                0 => log::info!(
                    target: "cmdr_lib::logging",
                    "Log storage disabled (advanced.maxLogStorageMb = 0). Error reports cannot be sent.",
                ),
                n => log::info!(
                    target: "cmdr_lib::logging",
                    "Log storage enabled: keep up to {n} files × 50 MB ({} MB cap)",
                    n * 50,
                ),
            }

            // Initialize crash reporter early, before anything that might crash
            crash_reporter::init(app.handle());

            // Stash the AppHandle for the error-report auto-dispatcher (Flow B). The
            // `log_error!` macro doesn't thread an AppHandle through, so we leave one
            // here for it to find. Setting the opt-in flag happens further down, once
            // we've loaded settings.
            error_reporter::auto_dispatcher::set_app_handle(app.handle().clone());

            // Log the resolved app data directory (shows -dev suffix in debug builds)
            config::log_app_data_dir(app.handle());

            // Initialize benchmarking (enabled by RUSTY_COMMANDER_BENCHMARK=1)
            benchmark::init_benchmarking();

            // Initialize the file watcher manager with app handle for events
            file_system::init_watcher_manager(app.handle().clone());

            // Initialize the volume manager with the root volume
            file_system::init_volume_manager();

            // Stash the AppHandle so SmbVolume can emit `smb-connection-changed`
            // events when sessions die or come back. The frontend reconnect
            // manager listens for these to drive its per-volume backoff cycle.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            file_system::volume::smb::set_app_handle(app.handle().clone());

            // Network discovery (mDNS) startup is deferred — see the post-`load_settings`
            // block below. Starting mDNS here would trigger macOS's "Cmdr wants to find devices
            // on local networks" prompt at app launch even on first install before the user has
            // shown any interest in networking. We only start at launch for returning users (who
            // already answered the OS prompt at least once, tracked via `network.firstTriggerDone`).
            //
            // For E2E builds, virtual SMB hosts also live alongside discovery — they're only
            // injected once discovery is up.

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

            // Set the FDA gate before the first `emit_volumes_changed_now()` below.
            // The gate suppresses path-based icon fetches in `volumes::list_locations`
            // while the user hasn't decided about FDA — without it, NSWorkspace icon
            // resolution stacks several TCC prompts (MediaLibrary, AppData, Desktop,
            // Documents, Downloads, ...) on top of our in-app onboarding modal.
            // See `crate::fda_gate` and `volumes/CLAUDE.md` § "FDA gate".
            #[cfg(target_os = "macos")]
            let os_fda_granted_for_gate = permissions::check_full_disk_access();
            #[cfg(target_os = "linux")]
            let os_fda_granted_for_gate = permissions_linux::check_full_disk_access();
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let os_fda_granted_for_gate = stubs::permissions::check_full_disk_access();
            fda_gate::set_fda_pending(fda_gate::is_fda_pending(
                saved_settings.full_disk_access_choice,
                os_fda_granted_for_gate,
            ));

            // Apply the Flow B opt-in flag *before* any user-visible error path can fire.
            // Default off (opt-in by design — Flow B sends data without per-event consent).
            error_reporter::auto_dispatcher::set_enabled(saved_settings.error_reports_enabled.unwrap_or(false));

            // Apply MTP enabled setting (default: true) before starting the watcher
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            mtp::set_mtp_enabled_flag(saved_settings.mtp_enabled.unwrap_or(true));

            // Start MTP device hotplug watcher (Android device support).
            // This also auto-connects any devices already plugged in at startup,
            // which probes the USB bus and trips the MacDroid File Provider TCC
            // prompt on macOS systems where MacDroid is installed. Skip while the
            // FDA gate is pending; `start_indexing_after_fda_decision` (deny path)
            // and a fresh launch with the FDA decision already made (allow path)
            // both call `start_mtp_watcher` after the gate has cleared.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            if !fda_gate::is_fda_pending_runtime() {
                mtp::start_mtp_watcher(app.handle());
            }

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

            // Observe macOS Accessibility > Display > Text Size changes
            #[cfg(target_os = "macos")]
            text_size::observe_system_text_size_changes(app.handle().clone());

            // Initialize font metrics. Loads the default 12px set plus any other
            // sizes the user has previously picked via the text-size slider.
            font_metrics::init_font_metrics(app.handle(), "system-400-12");
            font_metrics::load_all_metrics_from_disk(app.handle());

            // Start mDNS network discovery only for returning users who've already answered the
            // OS Local Network prompt at least once. Fresh installs stay quiet at launch — the
            // frontend calls `ensure_network_discovery_started` lazily on first user network
            // action (clicks "Network", opens "Connect to server…", upgrades a mounted share).
            // E2E builds always start so virtual hosts are populated before tests run.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            let should_start_network_at_launch = saved_settings.network_enabled.unwrap_or(true)
                && (saved_settings.network_first_trigger_done.unwrap_or(false) || cfg!(feature = "smb-e2e"));

            #[cfg(any(target_os = "macos", target_os = "linux"))]
            if should_start_network_at_launch {
                network::start_discovery(app.handle().clone());

                #[cfg(feature = "smb-e2e")]
                network::virtual_smb_hosts::setup_virtual_smb_hosts(app.handle());
            }

            // Apply direct SMB connection setting (default: true)
            file_system::set_direct_smb_enabled(saved_settings.direct_smb_connection.unwrap_or(true));
            file_system::git::set_virtual_portal_enabled(saved_settings.show_virtual_git_portal.unwrap_or(true));
            file_system::set_filter_safe_save_artifacts(saved_settings.filter_safe_save_artifacts.unwrap_or(true));
            file_system::set_smb_concurrency(saved_settings.smb_concurrency.unwrap_or(10) as usize);

            // Initialize disk space poller (live status bar updates)
            space_poller::init(app.handle());
            space_poller::set_threshold_mb(saved_settings.disk_space_change_threshold_mb.unwrap_or(1));
            space_poller::start();

            // Upgrade existing SMB mounts to direct smb2 connections (background, non-blocking).
            // Gated on the same lazy-startup conditions as mDNS above — opening a TCP socket to
            // a private-IP SMB server triggers macOS's Local Network prompt independently, so
            // we must not run this on fresh installs either. Returning users already answered
            // the prompt; lazy users wait until they click Network or use the picker's "Connect
            // directly" indicator.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            if should_start_network_at_launch {
                file_system::upgrade_existing_smb_mounts();
            }

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

            // Subscribe to NSWorkspace launch/terminate notifications so the "Open with"
            // candidate cache invalidates when the user installs or removes apps.
            #[cfg(target_os = "macos")]
            file_system::open_with::start_invalidation_observer();

            // Store the CheckMenuItem references in app state
            let menu_state = MenuState::default();
            *menu_state.show_hidden_files.lock_ignore_poison() = Some(menu_items.show_hidden_files);
            *menu_state.view_mode_full_left.lock_ignore_poison() = Some(menu_items.view_mode_full_left);
            *menu_state.view_mode_brief_left.lock_ignore_poison() = Some(menu_items.view_mode_brief_left);
            *menu_state.view_mode_full_right.lock_ignore_poison() = Some(menu_items.view_mode_full_right);
            *menu_state.view_mode_brief_right.lock_ignore_poison() = Some(menu_items.view_mode_brief_right);
            *menu_state.view_left_pane_submenu.lock_ignore_poison() = Some(menu_items.view_left_pane_submenu);
            *menu_state.view_right_pane_submenu.lock_ignore_poison() = Some(menu_items.view_right_pane_submenu);
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

            // Reuse the OS FDA result already captured for the gate above; this
            // call is on `/Library/Mail` which is cheap, but a fresh probe here
            // would race the user's decision in System Settings between the two
            // probes (allow path: granted at probe-1, still gating-pending at
            // probe-2 → indexer skips even though it shouldn't).
            let os_fda_granted = os_fda_granted_for_gate;

            if indexing::should_auto_start_indexing(
                saved_settings.indexing_enabled,
                saved_settings.full_disk_access_choice,
                os_fda_granted,
            ) {
                let app_handle = app.handle().clone();
                // Use tauri's runtime spawn instead of tokio::spawn since setup()
                // runs synchronously before the Tokio runtime is fully available
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = indexing::start_indexing(&app_handle) {
                        log::warn!("Failed to auto-start indexing: {e}");
                    }
                });
            } else if saved_settings.indexing_enabled == Some(false) {
                log::info!("Drive indexing auto-start skipped (disabled in settings)");
            } else {
                log::info!(
                    "Drive indexing auto-start deferred until Full Disk Access decision (FDA choice: {:?}, OS-granted: {})",
                    saved_settings.full_disk_access_choice,
                    os_fda_granted,
                );
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
            if id == VIEW_MODE_FULL_LEFT_ID
                || id == VIEW_MODE_BRIEF_LEFT_ID
                || id == VIEW_MODE_FULL_RIGHT_ID
                || id == VIEW_MODE_BRIEF_RIGHT_ID
            {
                // Per-pane view mode click. Sync the affected pane's pair (the muda click
                // already toggled the clicked item, so unchecking the sibling is enough),
                // store the new mode in MenuState, and notify the frontend with the target
                // pane so it can update without changing focus.
                let (pane, mode_str) = match id {
                    VIEW_MODE_FULL_LEFT_ID => ("left", "full"),
                    VIEW_MODE_BRIEF_LEFT_ID => ("left", "brief"),
                    VIEW_MODE_FULL_RIGHT_ID => ("right", "full"),
                    VIEW_MODE_BRIEF_RIGHT_ID => ("right", "brief"),
                    _ => unreachable!(),
                };
                let menu_state = app.state::<MenuState<tauri::Wry>>();
                let new_mode = if mode_str == "full" {
                    ViewMode::Full
                } else {
                    ViewMode::Brief
                };
                if pane == "left" {
                    *menu_state.view_mode_left.lock_ignore_poison() = new_mode;
                } else {
                    *menu_state.view_mode_right.lock_ignore_poison() = new_mode;
                }
                let _ = menu::sync_view_mode_check_states(&menu_state);
                let _ = app.emit_to(
                    "main",
                    "view-mode-changed",
                    serde_json::json!({ "mode": mode_str, "pane": pane }),
                );
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

            // === Open with submenu: dynamic IDs prefix-routed before unified dispatch ===
            // Items have IDs like `open-with:com.apple.Xcode` — too dynamic to enumerate
            // in `menu_id_to_command`. We resolve the bundle ID back to an app path via
            // `MenuState.context.open_with_apps` and call the launch helper directly.
            #[cfg(target_os = "macos")]
            if let Some(bundle_id) = id.strip_prefix(menu::open_with::OPEN_WITH_ID_PREFIX) {
                use crate::file_system::open_with::open_paths_with;
                use std::path::PathBuf;

                let menu_state = app.state::<MenuState<tauri::Wry>>();
                let ctx = menu_state.context.lock_ignore_poison();
                let app_path = ctx.open_with_apps.get(bundle_id).cloned();
                let paths: Vec<PathBuf> = ctx.paths.iter().map(PathBuf::from).collect();
                drop(ctx);

                if let Some(app_path) = app_path
                    && !paths.is_empty()
                {
                    if let Err(e) = open_paths_with(&paths, &app_path) {
                        log::warn!("Open with failed for {bundle_id}: {e}");
                    }
                } else {
                    log::warn!("Open with: missing app or paths for {bundle_id}");
                }
                return;
            }

            // === Open with → Other... : show NSOpenPanel, then launch ===
            #[cfg(target_os = "macos")]
            if id == menu::open_with::OPEN_WITH_OTHER_ID {
                use crate::file_system::open_with::{open_paths_with, pick_app_via_open_panel};
                use std::path::PathBuf;

                let menu_state = app.state::<MenuState<tauri::Wry>>();
                let paths: Vec<PathBuf> = menu_state
                    .context
                    .lock_ignore_poison()
                    .paths
                    .iter()
                    .map(PathBuf::from)
                    .collect();

                // NSOpenPanel must run on the main thread. on_menu_event is invoked on
                // the main thread by Tauri/muda, so this is safe.
                if let Some(app_path) = pick_app_via_open_panel()
                    && !paths.is_empty()
                    && let Err(e) = open_paths_with(&paths, &app_path)
                {
                    log::warn!("Open with (Other...) failed: {e}");
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
        .invoke_handler(specta_builder.invoke_handler())
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
