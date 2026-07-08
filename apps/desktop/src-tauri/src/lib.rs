// Deny unused code to catch dead code early (like knip for TS)
#![deny(unused)]
// Warn on unused dependencies to catch platform-specific cfg mismatches
#![warn(unused_crate_dependencies)]
// Warn on redundant path prefixes (like std::path::Path when Path is imported)
#![warn(unused_qualifications)]
// Use log::* macros instead of println!/eprintln!/dbg! for proper log level control
// and so error-report bundles capture the context. See `logging/CLAUDE.md` for the rules.
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// No leftover `todo!()` / `unimplemented!()` stubs reaching a build (`unreachable!()` stays allowed).
#![deny(clippy::todo, clippy::unimplemented)]
// Require justification for all #[allow] attributes
#![warn(clippy::allow_attributes_without_reason)]
// Require a `// SAFETY:` comment on every `unsafe {}` block: each site must state the concrete
// invariant that makes it sound. Rote FFI is documented per-site, never blanket-exempted.
#![warn(clippy::undocumented_unsafe_blocks)]
// No silent `.unwrap()` in production: each must be a handled error or a documented `.expect("why")`.
// Extends the lock-poison discipline to all unwraps. Tests are exempt via clippy.toml.
#![warn(clippy::unwrap_used)]

//noinspection RsUnusedImport
// Silence false positives for dev dependencies (used only in benches/, not lib)
// and transitive dependencies (notify is used by notify-debouncer-full)
#[cfg(test)]
use criterion as _;
//noinspection RsUnusedImport
// Property-based testing. Used in module-local `mod proptests` blocks; the
// crate-root marker keeps `unused_crate_dependencies` quiet for builds that
// happen to compile a subset of test modules.
#[cfg(test)]
use proptest as _;
//noinspection RsUnusedImport
// Dev-only log-routing shim. Used by the phase4 bench's optional
// `env_logger::try_init()` (commented-in when collecting wire traces) and by
// ad-hoc debug-logging in tests. Harmless otherwise.
#[cfg(test)]
use env_logger as _;
//noinspection RsUnusedImport
use mimalloc as _;
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
mod ipc_collectors;

mod logging;

#[cfg(target_os = "macos")]
mod accent_color;
#[cfg(target_os = "linux")]
mod accent_color_linux;
mod ai;
mod analytics;
pub mod benchmark;
mod child_window_state;
mod clipboard;
mod commands;
pub mod config;
mod crash_reporter;
mod diagnostics_snapshot;
mod downloads;
#[cfg(target_os = "macos")]
mod drag_image_detection;
#[cfg(target_os = "macos")]
mod drag_image_swap;
mod error_reporter;
mod favorites;
mod fda_gate;
mod feedback;
mod file_system;
pub mod file_viewer;
mod font_metrics;
mod go_to_path;
pub mod icons;
pub mod importance;
pub mod indexing;
mod install_id;
pub mod licensing;
#[cfg(target_os = "linux")]
pub(crate) mod linux_distro;
#[cfg(target_os = "linux")]
mod linux_icons;
mod location;
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
mod platform;
mod pluralize;
mod quick_look;
mod redact;
#[cfg(target_os = "macos")]
mod reduce_transparency;
mod restricted_paths;
pub mod search;
mod secrets;
pub mod selection;
mod settings;
mod short_id;
mod space_poller;
mod system_events;
mod system_memory;
mod system_strings;
pub mod test_mode;
#[cfg(target_os = "macos")]
mod text_size;
#[cfg(target_os = "macos")]
mod updater;
mod usb_speed;
mod volume_broadcast;
#[cfg(target_os = "macos")]
mod volumes;
#[cfg(target_os = "linux")]
mod volumes_linux;
mod whats_new;
mod window_events;

// Non-macOS stubs (Linux has real implementations for everything;
// other platforms use stubs for all platform-specific features)
#[cfg(not(target_os = "macos"))]
mod stubs;

use menu::{MenuState, ViewMode};
use tauri::Manager;

// `greet` and the rest of the Tauri command surface live in `ipc.rs`, which
// exposes them through a typed `tauri_specta::Builder`. See `ipc.rs` for the
// migration recipe.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Refuse to start an E2E run with no isolated data dir: it would resolve every persisted
    // store to the developer's real prod dir and corrupt it (e.g. a screenshot `favorites.add`
    // bleeding "left" favorites into prod). Must run before anything resolves a data dir.
    test_mode::guard_e2e_requires_data_dir();

    // Type-safe IPC: collect every command and event into a tauri-specta Builder.
    // The same Builder is attached to `tauri::Builder::default()` below.
    // `bindings.ts` is regenerated explicitly via `pnpm bindings:regen` (which
    // invokes the ignored `ipc::tests::export_bindings_test` and post-processes
    // with oxfmt); CI's `bindings-fresh` check fails when it drifts. Don't
    // re-export at runtime. Without the test's header + oxfmt postprocess, that
    // path silently overwrites the committed file with raw specta output on
    // every dev launch.
    let specta_builder = ipc::builder();
    // `invoke_handler()` returns an owned closure (it clones the command map
    // internally), so we grab it here before moving `specta_builder` into the
    // `setup` closure where `mount_events` registers the typed events.
    let invoke_handler = specta_builder.invoke_handler();
    let builder = tauri::Builder::default();

    // Register the `cmdr-media://` async URI scheme the file viewer serves images and
    // PDFs through. Registered before any window exists (correct: `viewer-*` windows
    // are created lazily and inherit the app-wide scheme). The handler is a thin shell
    // over `file_viewer::media_protocol`; access is gated by an unguessable per-open
    // token, not the path. See `file_viewer/media_protocol.rs`.
    let builder = builder.register_asynchronous_uri_scheme_protocol(
        file_viewer::media_protocol::SCHEME,
        |_ctx, request, responder| {
            file_viewer::media_protocol::handle_request(request, responder);
        },
    );

    // Window state plugin is only available on desktop platforms. The filter
    // restricts persistence to the main window: Settings, Debug, and viewer
    // windows are deliberately reset on every launch. Within a session they
    // remember position via `child_window_state` (in-memory only).
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(
        tauri_plugin_window_state::Builder::new()
            .with_filter(|label| label == "main")
            .build(),
    );

    // MCP Bridge plugin is only available in debug builds for security.
    //
    // Two non-obvious things to keep in mind here:
    //   1. The plugin's `Config::default()` is `bind_address: "0.0.0.0"`, which exposes
    //      the WebSocket bridge (DOM inspection, JS execution, IPC monitoring) to anyone
    //      on the LAN. We always force `127.0.0.1` so the bridge is localhost-only. This
    //      is a security fix; do NOT remove it even when adding remote-device support.
    //   2. The plugin has no public method to query the bound port, and its internal
    //      `find_available_port` silently returns `base_port` on exhaustion (no error).
    //      We therefore let `tauri-wrapper.js` allocate an ephemeral port up front via
    //      `net.createServer().listen(0)`, pass it as `CMDR_MCP_BRIDGE_PORT`, AND have
    //      the wrapper write `<data_dir>/tauri-mcp.port` BEFORE Tauri launches. After
    //      plugin setup we run a 500 ms post-bind `TcpStream::connect` probe and
    //      warn-log on mismatch so a silent fallback is visible in the logs.
    //
    // See docs/tooling/instance-isolation.md § "Per-resource breakdown" (Tauri MCP
    // bridge port row) for the wrapper-writes-port-file contract.
    #[cfg(debug_assertions)]
    let builder = {
        let mut bridge_builder = tauri_plugin_mcp_bridge::Builder::new().bind_address("127.0.0.1");
        let expected_bridge_port: Option<u16> = std::env::var("CMDR_MCP_BRIDGE_PORT").ok().and_then(|v| v.parse().ok());
        if let Some(port) = expected_bridge_port {
            bridge_builder = bridge_builder.base_port(port);
        }
        let plugin = bridge_builder.build::<tauri::Wry>();

        // Post-bind probe: 500 ms after registration, try to connect on the expected port.
        // On success: log info. On failure: warn that the wrapper-written port file may be
        // stale (the plugin silently fell back to a different port; readers will discover
        // it on first request via `ECONNREFUSED`).
        if let Some(port) = expected_bridge_port {
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                match tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    tokio::net::TcpStream::connect(addr),
                )
                .await
                {
                    Ok(Ok(_)) => log::info!(
                        target: "mcp::bridge",
                        "tauri-MCP bridge bound to 127.0.0.1:{port}",
                    ),
                    Ok(Err(err)) => log::warn!(
                        target: "mcp::bridge",
                        "tauri-MCP bridge did not bind 127.0.0.1:{port} within 500 ms ({err}); the port file at <data_dir>/tauri-mcp.port may be stale",
                    ),
                    Err(_) => log::warn!(
                        target: "mcp::bridge",
                        "tauri-MCP bridge probe to 127.0.0.1:{port} timed out after 500 ms; the port file at <data_dir>/tauri-mcp.port may be stale",
                    ),
                }
            });
        }

        builder.plugin(plugin)
    };

    // Playwright E2E testing plugin: socket bridge for direct webview injection.
    // Socket path is overridable via CMDR_PLAYWRIGHT_SOCKET so parallel E2E shards
    // can each spawn their own Tauri instance bound to a distinct socket.
    #[cfg(feature = "playwright-e2e")]
    let builder = {
        let mut pw_config = tauri_plugin_playwright::PluginConfig::new();
        if let Ok(socket_path) = std::env::var("CMDR_PLAYWRIGHT_SOCKET") {
            pw_config = pw_config.socket_path(socket_path);
        }
        builder.plugin(tauri_plugin_playwright::init_with_config(pw_config))
    };

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
        .plugin(tauri_plugin_notification::init())
        .plugin(downloads::global_shortcut::plugin_builder())
        .setup(move |app| {
            // Mount the typed `tauri-specta` events onto the app. Required before
            // any `Event::emit` / `Event::listen` call resolves the event name
            // from the registry. See `ipc.rs` for the event collection.
            specta_builder.mount_events(app);

            // E2E: keep a test run's swarm of windows from stealing the developer's
            // focus. The activation policy is the robust lever — a `Prohibited` app
            // can never become the active application, which defeats every focus path
            // at once (launch-time activation, a child window's `makeKeyAndOrderFront`)
            // regardless of how many windows a run opens. Paired with per-window
            // `orderBack:` so the windows also stay visually behind (see
            // `commands::window_ordering::show_main_window` and `order_window_to_back`).
            // Strictly additive: gated on `CMDR_E2E_MODE`, so production is untouched.
            #[cfg(target_os = "macos")]
            if test_mode::is_e2e_mode() {
                app.set_activation_policy(tauri::ActivationPolicy::Prohibited);
            }

            // === Logging setup ===
            //
            // Hand-rolled fern dispatch tree (`logging::dispatch::init`) replaces
            // `tauri-plugin-log`. Why: per-output level filtering. File target locked at
            // Debug (error reports need the context); stdout defaults to Info (clean for
            // `pnpm dev`) with `RUST_LOG` per-module overrides applied to stdout only.
            // The verbose toggle bumps stdout to Debug via an AtomicU8, no logger
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
            // RUST_LOG always wins. Only bump if RUST_LOG didn't set a base level.
            if std::env::var("RUST_LOG").is_err() && verbose_default {
                logging::dispatch::set_stdout_threshold(log::LevelFilter::Debug);
            }
            if let Err(err) = init_result {
                // Don't panic. A logger collision (rare; tests, double-init) is recoverable.
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
                    "Log storage enabled: keep up to {} × 50 MB ({} MB cap)",
                    pluralize::pluralize(n as u64, "file"),
                    n * 50,
                ),
            }

            // Snapshot the diagnostics id into a cheap static before anything that might crash,
            // so the panic hook can read it without allocating or locking. Mints both install
            // ids on first launch.
            install_id::init();

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

            // Backstop reaper for orphaned directory listings. The primary, fast
            // eviction is the FE-fired `list_directory_end` IPC; this only catches
            // listings whose close IPC was never delivered (a thrown FE handler, an
            // `$effect` teardown that threw), so their entry vector + OS watcher would
            // otherwise pin for the rest of the session. Mirrors the search index's
            // backstop timer and the viewer's window-`Destroyed` net.
            file_system::start_orphan_listing_reaper();

            // Stash the AppHandle for the viewer's per-session watcher manager
            // threads so they can emit `viewer:file-changed:<sid>` events.
            file_viewer::init_app_handle(app.handle().clone());

            // Point preview-in-zip temp-extraction at a per-instance dir under the app
            // data dir (so side-by-side dev/prod/worktree instances never reap each
            // other's live temps), and reap any `.cmdr-viewer-*` orphan left by a crash.
            if let Ok(data_dir) = config::resolved_app_data_dir(app.handle()) {
                file_viewer::init_archive_extract_dir(data_dir.join("viewer-extract"));
            }

            // Initialize the volume manager with the root volume
            file_system::init_volume_manager();

            // Stash the AppHandle so SmbVolume can emit `smb-connection-changed`
            // events when sessions die or come back. The frontend reconnect
            // manager listens for these to drive its per-volume backoff cycle.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            file_system::volume::smb::set_app_handle(app.handle().clone());

            // Stash the AppHandle so the drag-out file-promise machinery can
            // dispatch session cleanup (freeing the retained promise delegates)
            // back to the AppKit main thread once a fulfillment drains.
            #[cfg(target_os = "macos")]
            native_drag::set_app_handle(app.handle().clone());

            // Network discovery (mDNS) startup is deferred. See the post-`load_settings`
            // block below. Starting mDNS here would trigger macOS's "Cmdr wants to find devices
            // on local networks" prompt at app launch even on first install before the user has
            // shown any interest in networking. We only start at launch for returning users (who
            // already answered the OS prompt at least once, tracked via `network.firstTriggerDone`).
            //
            // For E2E builds, virtual SMB hosts also live alongside discovery. They're only
            // injected once discovery is up.

            // Initialize volume broadcast (must be before watchers so they can emit)
            volume_broadcast::init(app.handle());

            // Wire the "busy volumes" emitter so write ops can broadcast
            // `volumes-busy-changed` (drives disabling Eject while a transfer
            // touches a device). Before any write op can run.
            file_system::init_busy_volume_emitter(app.handle());

            // Operation-manager event emitter for `operations-changed` (drives
            // the queue window's row set). Before any write op can run.
            file_system::init_operation_event_emitter(app.handle());

            // Restricted-paths tracker (TCC-gated paths the user has been
            // denied access to). Installs an NSApplicationDidBecomeActive
            // observer that re-probes the set when the user returns from
            // System Settings. See `crate::restricted_paths`.
            restricted_paths::init(app.handle());

            // Start volume mount/unmount watcher
            #[cfg(target_os = "macos")]
            volumes::watcher::start_volume_watcher(app.handle());

            #[cfg(target_os = "linux")]
            volumes_linux::watcher::start_volume_watcher(app.handle());

            // Register the virtual MTP device (before the watcher so it's in the initial
            // snapshot) when requested. Two activation paths, unified in
            // `activate_from_env_if_requested`: an E2E run (CMDR_E2E_MODE=1) or a dev opt-in
            // (CMDR_VIRTUAL_MTP=1, or =<dir> for a custom backing dir). Non-MTP E2E shards opt
            // out via CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP to avoid racing the shared backing dir.
            // See `mtp/virtual_device.rs::decide_startup_root` and `docs/tooling/virtual-mtp.md`.
            #[cfg(feature = "virtual-mtp")]
            mtp::virtual_device::activate_from_env_if_requested();

            // Ensure ptpcamerad is re-enabled in case a previous session crashed
            // while it was suppressed. No-op if it was already enabled.
            #[cfg(target_os = "macos")]
            mtp::macos_workaround::ensure_ptpcamerad_enabled();

            // Load persisted settings early so MTP enabled flag is set before the watcher starts
            let saved_settings = settings::load_settings(app.handle());

            // Set the FDA gate before the first `emit_volumes_changed_now()` below.
            // The gate suppresses path-based icon fetches in `volumes::list_locations`
            // while the user hasn't decided about FDA. Without it, NSWorkspace icon
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

            // Start the Downloads watcher if the FDA gate is open. The
            // window-focus listener (registered below in `on_window_event`)
            // re-runs this on every transition, so a missed start here
            // recovers as soon as the user focuses the main window.
            if let Err(err) = downloads::refresh_runtime(app.handle()) {
                log::warn!(target: "downloads::watcher", "Initial start failed: {err}");
            }

            // Register the global go-to-latest-download shortcut (default
            // ⌃⌥⌘J). FDA-gated: `apply_global_go_to_latest_shortcut` no-ops when
            // the gate is closed, and the focus-event listener below
            // re-runs the check on every transition.
            downloads::refresh_global_go_to_latest_shortcut(app.handle());

            // Apply the Flow B opt-in flag *before* any user-visible error path can fire.
            // Default off (opt-in by design: Flow B sends data without per-event consent).
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

            // Load persisted recent search history into the in-memory cache.
            search::history::load_history(app.handle());

            // Same for recent selections (Selection dialog history).
            selection::history::load_history(app.handle());

            // Same for recent paths (Go to path dialog history).
            go_to_path::history::load_history(app.handle());

            // Load manually-added servers and inject into discovery state
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::manual_servers::load_manual_servers(app.handle());

            // Drag image detection swizzle is installed in RunEvent::Ready (not here)
            // because wry 0.54+ registers the WryWebView ObjC class lazily. It doesn't
            // exist in the runtime until the first webview is created, which happens after
            // setup() returns.

            // Observe system accent color changes and emit events to frontend
            #[cfg(target_os = "macos")]
            accent_color::observe_accent_color_changes(app.handle().clone());
            #[cfg(target_os = "linux")]
            accent_color_linux::observe_accent_color_changes(app.handle().clone());

            // Observe macOS Accessibility > Display > Reduce transparency changes
            #[cfg(target_os = "macos")]
            reduce_transparency::observe_reduce_transparency_changes(app.handle().clone());
            #[cfg(not(target_os = "macos"))]
            stubs::reduce_transparency::observe_reduce_transparency_changes(app.handle().clone());

            // Observe macOS Accessibility > Display > Text Size changes
            #[cfg(target_os = "macos")]
            text_size::observe_system_text_size_changes(app.handle().clone());

            // Initialize font metrics. Loads the default 12px set plus any other
            // sizes the user has previously picked via the text-size slider.
            font_metrics::init_font_metrics(app.handle(), "system-400-12");
            font_metrics::load_all_metrics_from_disk(app.handle());

            // Sync the runtime `network.enabled` flag from settings so BE-side upgrade paths
            // can gate themselves correctly (default `true`).
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::set_network_enabled_flag(saved_settings.network_enabled.unwrap_or(true));

            // Start mDNS network discovery only for returning users who've already answered the
            // OS Local Network prompt at least once. Fresh installs stay quiet at launch. The
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

            // Initialize disk space poller (live status bar updates + low-disk-space warning)
            space_poller::init(app.handle());
            space_poller::set_threshold_mb(saved_settings.disk_space_change_threshold_mb.unwrap_or(1));
            space_poller::configure_low_disk_space(
                saved_settings.low_disk_space_enabled(),
                saved_settings.low_disk_space_threshold_percent.unwrap_or(5),
            );
            space_poller::start();

            // Start the anonymous beta-analytics heartbeat (launch beat + hourly). Consent-gated
            // and suppressed in dev/CI; see `analytics/CLAUDE.md`.
            analytics::init(app.handle());
            analytics::start();
            // One PostHog `app_launched` event per startup, through the same consent gate.
            analytics::posthog::capture("app_launched", serde_json::json!({}));

            // Upgrade existing SMB mounts to direct smb2 connections (background, non-blocking).
            // No `firstTriggerDone` gate here: the function is a no-op when there are no SMB
            // mounts (no network activity, no prompt). When there ARE mounts and direct-SMB is
            // enabled, the function kicks off mDNS itself so the Keychain lookup can resolve
            // hostnames — same shape as the manual "Connect directly" and mount-time paths.
            // The macOS Local Network prompt fires once per app and only when an SMB mount is
            // present at launch; subsequent launches start mDNS eagerly via `firstTriggerDone`.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            file_system::upgrade_existing_smb_mounts(app.handle().clone());

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
            // On macOS, keep a clone of the main menu so `activate_window_menu` can swap the
            // app-level menu bar back to it on main / Settings / Debug focus-gain. The clone shares
            // the same underlying items (Tauri's `Menu` is reference-counted), so the item refs
            // stored below keep mutating the live menu. macOS has a single app-level menu bar
            // (tauri-apps/tauri#5768), so there's no per-window menu to set here.
            #[cfg(target_os = "macos")]
            let main_menu_clone = menu_items.menu.clone();
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
            *menu_state.reopen_closed_tab.lock_ignore_poison() = Some(menu_items.reopen_closed_tab);
            *menu_state.items.lock_ignore_poison() = menu_items.items;
            *menu_state.sort_submenu.lock_ignore_poison() = Some(menu_items.sort_submenu);

            // On macOS, build the shared viewer menu once and store it (plus the main-menu clone and
            // the viewer word-wrap ref). `activate_window_menu` swaps the app-level menu bar between
            // these on window focus-gain; `viewer_set_word_wrap` flips the stored CheckMenuItem.
            #[cfg(target_os = "macos")]
            {
                *menu_state.main_menu.lock_ignore_poison() = Some(main_menu_clone);
                let viewer_menu_items = menu::build_viewer_menu(app.handle())?;
                *menu_state.viewer_word_wrap.lock_ignore_poison() = Some(viewer_menu_items.word_wrap);
                *menu_state.viewer_menu.lock_ignore_poison() = Some(viewer_menu_items.menu);
            }

            app.manage(menu_state);

            // Set window title based on license status
            let license_status = licensing::get_app_status(app.handle());
            let title = licensing::get_window_title(&license_status);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(&title);
            }

            // titleBarStyle is "Overlay" in JSON for macOS (needed so trafficLightPosition
            // is applied at window creation time. Setting it at runtime resets the position.
            // On Linux/GTK, Overlay hides native window controls, so revert to Visible.
            #[cfg(target_os = "linux")]
            if let Some(window) = app.get_webview_window("main") {
                use tauri::TitleBarStyle;
                let _ = window.set_title_bar_style(TitleBarStyle::Visible);
            }

            // Initialize custom updater state (shared between download and install commands)
            #[cfg(target_os = "macos")]
            app.manage(updater::UpdateState::new());

            // Native Quick Look controller. Empty on init; populated lazily
            // when the user presses Shift+Space. macOS-only state machine;
            // on other platforms the type is `Mutex<()>` so this compiles
            // everywhere.
            app.manage(quick_look::init_state());

            // In-session position cache for Settings + Debug windows. See
            // `child_window_state.rs` for the why.
            app.manage(child_window_state::ChildWindowRectStore::new());

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
            indexing::init(app.handle());

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
        .on_menu_event(menu::handle_menu_event)
        .invoke_handler(invoke_handler)
        .on_window_event(|window, event| {
            // Main-window focus re-checks the FDA gate so the Downloads
            // watcher starts/stops on transitions. Covers the "user
            // toggled FDA in System Settings, came back to Cmdr" path
            // without polling. Idempotent when nothing changed.
            if let tauri::WindowEvent::Focused(true) = event
                && window.label() == "main"
            {
                if let Err(err) = downloads::refresh_runtime(window.app_handle()) {
                    log::warn!(
                        target: "downloads::watcher",
                        "Focus-driven gate re-check failed: {err}",
                    );
                }
                // Re-evaluate the global-shortcut registration too: if FDA
                // flipped between blur and focus, register/unregister to
                // match. Idempotent when nothing changed.
                downloads::refresh_global_go_to_latest_shortcut(window.app_handle());
            }
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
            // Free a viewer session when its window is destroyed. Closing a viewer
            // via the titlebar X never fires the FE `viewer_close` IPC (that only
            // runs from the in-app close path), so without this the `ViewerSession`
            // (backend, line index, watcher thread) leaked until app quit.
            // `close_session_for_window` is idempotent: if the FE already closed the
            // session via IPC, the lookup is a no-op.
            if let tauri::WindowEvent::Destroyed = event
                && window.label().starts_with("viewer-")
            {
                file_viewer::close_session_for_window(window.label());
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
