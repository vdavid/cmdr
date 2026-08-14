// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// It's there rather than here so the crates under `crates/` share one definition.
//
// This one can't go with them: it's judged per compilation unit, so as a
// package-wide flag every bin, integration test, and bench would report ~100
// "unused extern crate" errors for deps only the lib uses. It catches
// platform-specific cfg mismatches, hence the `use foo as _;` markers below.
#![warn(unused_crate_dependencies)]

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
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the shipped binary (see `Cargo.toml`). That makes `cmdr_lib` an extern
// crate of its own test target, which `unused_crate_dependencies` then reports.
#[cfg(test)]
use cmdr_lib as _;
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

// These host primitives live in `cmdr-fs` so every crate in the workspace shares
// one copy, and are re-exported here at their original paths: poison-free
// locking, the count/noun formatter, thread QoS, the process-memory readers, and
// the SQLite connection factories over the one process-wide page-cache slab.
// `crate::ignore_poison::…`, `crate::pluralize::…`, `crate::thread_qos::…`,
// `crate::process_memory::…`, and `crate::sqlite_util::…` all still resolve.
pub use cmdr_fs::ignore_poison::IgnorePoison;
pub use cmdr_fs::{ignore_poison, pluralize, process_memory, sqlite_util, thread_qos};

mod ipc;
mod ipc_collectors;

mod logging;

#[cfg(target_os = "macos")]
mod accent_color;
#[cfg(target_os = "linux")]
mod accent_color_linux;
pub mod agent;
mod ai;
mod analytics;
pub mod benchmark;
mod child_window_state;
mod clipboard;
mod commands;
pub mod config;
mod crash_reporter;
/// The dialog gallery's fixture tree (Debug > Soft dialogs). Dev and E2E builds:
/// `dialog-inset.spec.ts` drives the gallery, and the disk-backed dialogs need a
/// real tree to scan. Never in a shipped build.
#[cfg(any(debug_assertions, feature = "playwright-e2e"))]
pub mod dev_fixtures;
mod diagnostics_snapshot;
mod downloads;
#[cfg(target_os = "macos")]
mod drag_image_detection;
#[cfg(target_os = "macos")]
mod drag_image_swap;
mod error_reporter;
pub mod events;
mod favorites;
mod fda_gate;
mod feedback;
mod file_system;
pub mod file_viewer;
mod font_metrics;
mod go_to_path;
pub mod icons;
mod index_host;
mod install_id;
mod instance_lock;
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
pub mod operation_log;
#[cfg(target_os = "macos")]
mod permissions;
#[cfg(target_os = "linux")]
mod permissions_linux;
mod platform;
pub mod priority;
mod quick_look;
mod quit;
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
mod subprocess;
mod system_events;
mod system_memory;
mod system_strings;
pub mod test_mode;
/// The sanctioned way to wait for background work in a Rust test. See `docs/testing.md`.
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(target_os = "macos")]
mod text_size;
#[cfg(target_os = "macos")]
mod updater;
mod usb_speed;
mod volume_broadcast;
mod volume_host;
#[cfg(target_os = "macos")]
mod volumes;
#[cfg(target_os = "linux")]
mod volumes_linux;
mod whats_new;
mod window_events;
mod window_state;

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
            // Everything the index needs from this app, in one place. Must run
            // before anything can start background work. Mirror of
            // `indexing/host/`, which declares the other side of each seam.
            index_host::install(app.handle());

            // Everything a storage backend needs from this app, in one place.
            // Must run before any volume is constructed. Mirror of
            // `cmdr_fs::volume::host`, which declares the other side of each seam.
            volume_host::install(app.handle());

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
                // allowed-discarded-outcome: the count is logged per file inside; a startup sweep has nobody to report a total to.
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

            // Claim the data dir before anything opens a database. Two processes on one data dir
            // means two index writers handing out the same entry IDs, which corrupts the index
            // silently. This is the earliest point that has both a logger (so the refusal is
            // recorded) and a resolved data dir. Tauri already created the config-declared main
            // window before `setup`, but it's `"visible": false`, and a refused process exits here
            // without ever showing it or touching an index file. See `instance_lock.rs`.
            match config::resolved_app_data_dir(app.handle()) {
                Ok(dir) => instance_lock::claim_data_dir_or_exit(&dir),
                Err(e) => log::warn!(
                    target: "instance_lock",
                    "Couldn't resolve the data dir for the instance lock: {e}. Continuing without it."
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

                // Point the in-flight transfer-partial ledger at the data dir and
                // clear the `.cmdr-tmp-*` partials an earlier run recorded and never
                // finished (a quit or a crash mid-copy). Before any copy can start,
                // so nothing we're about to write is in the list we sweep. See
                // `file_system/write_operations/in_flight_temps.rs`.
                file_system::write_operations::init_and_sweep_in_flight_temps(&data_dir);
            }

            // Initialize the volume manager with the root volume
            file_system::init_volume_manager();

            // Stash the AppHandle so the MCP `indexing` tool can drive
            // enable/rescan (which need a concrete handle) from its generic
            // executor. Disable/forget need no handle.
            commands::indexing::set_app_handle(app.handle().clone());

            // Stash the AppHandle so SmbVolume can emit `volume-connection-changed`
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

            // Teach the MTP session layer how an attached storage becomes a
            // volume. Backends never register themselves, so without this an
            // MTP device would connect and show no volumes at all. Must be
            // before the virtual device and the hotplug watcher below, which
            // are the two things that can connect one.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            mtp::volume_wiring::install_volume_registrar();

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
            file_system::staging::set_show_safe_save_files(saved_settings.show_safe_save_files.unwrap_or(true));
            file_system::staging::set_show_staging_temps(saved_settings.show_staging_temp_files.unwrap_or(false));
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

            // Reuse the OS FDA result already captured for the gate above; this
            // call is on `/Library/Mail` which is cheap, but a fresh probe here
            // would race the user's decision in System Settings between the two
            // probes (allow path: granted at probe-1, still gating-pending at
            // probe-2 → indexer skips even though it shouldn't).
            let os_fda_granted = os_fda_granted_for_gate;

            // The FDA rule is the app's, so it's resolved here and handed to the
            // index as a plain answer.
            let fda_pending = fda_gate::is_fda_pending(saved_settings.full_disk_access_choice, os_fda_granted);
            // Use tauri's runtime spawn instead of tokio::spawn since setup() runs
            // synchronously before the Tokio runtime is fully available.
            let full_disk_access_choice = saved_settings.full_disk_access_choice;
            tauri::async_runtime::spawn(async move {
                match index_host::index().start_root_at_launch(fda_pending) {
                    Ok(true) => {}
                    Ok(false) => log::info!(
                        "Drive indexing auto-start skipped (indexing enabled: {:?}, FDA choice: {full_disk_access_choice:?}, OS-granted: {os_fda_granted})",
                        saved_settings.indexing_enabled,
                    ),
                    Err(e) => log::warn!("Failed to auto-start indexing: {e}"),
                }
            });

            // Start the importance scheduler: it sweeps the index registry for
            // already-ready volumes and subscribes to the scan-completion bus, so a
            // volume's folder weights recompute when its index finishes scanning (or
            // is Fresh at launch). Independent of whether indexing auto-starts here —
            // the bus fires whenever any scan completes. See
            // `importance/scheduler.rs` and the plan (Decision 4 / 5).
            if let Some(scheduler) = cmdr_index::importance::scheduler::ImportanceScheduler::start() {
                // Reachable from the IPC layer: `record_visit` resolves it here.
                app.manage(scheduler);
            }

            // Start the media-ML enrichment scheduler: it runs on the policy applied
            // above (off by default), hooks its
            // cancellation into the shared indexing memory watchdog, and subscribes
            // to the same scan-completion bus so images enrich when a local volume's
            // index finishes scanning. Off by default, so no work runs until the
            // toggle is enabled. See `media_index/CLAUDE.md`.
            if let Some(scheduler) = cmdr_index::media_index::scheduler::MediaScheduler::start() {
                app.manage(scheduler);
            }

            // Keep the search ranker's importance weight map fresh: subscribe to the
            // root volume's recompute-completed notifications and (re)load the
            // path→weight snapshot the search engine blends into result ordering
            // (subscribe-don't-poll). A missing/empty `importance.db` degrades search
            // ranking to match-quality + recency — today's behavior. See
            // `search/DETAILS.md` § importance ranking.
            match config::resolved_app_data_dir(app.handle()) {
                Ok(data_dir) => {
                    // Point the LLM call logger at `{data dir}/llm-logs/` so the tap in
                    // `ai::client` can write there. Enablement is separate (the `logLlmCalls`
                    // setting, dev-default-on); this only records where.
                    ai::llm_log::init(&data_dir);
                    search::start_importance_weight_subscriber(data_dir);
                }
                Err(e) => log::warn!("search importance weights not wired: {e}"),
            }

            // Open the durable operation log and spawn its writer thread. Nothing
            // journals through it yet (the durable store is the foundation); capture hooks
            // the write pipeline into the managed writer this places in state.
            operation_log::start(app.handle());

            // Open the agent's durable store (`main.db`, peer to `operation-log.db`) and
            // register its handle. The chat runtime and the Ask Cmdr IPC commands are its
            // consumers; opening here runs the migration ladder at startup and keeps the
            // DB current.
            agent::start(app.handle());

            // Restore the main window's saved position and size. Placement
            // only: the window is still hidden here, and `+page.svelte` shows
            // it once the webview confirms a first paint. Only the main window
            // persists across launches; Settings, Debug, and viewer windows
            // deliberately start fresh (in-session position lives in
            // `child_window_state`). See `window_state/`.
            window_state::init(app.handle());
            if let Some(window) = app.get_webview_window("main") {
                // Track BEFORE restoring: `restore` moves and resizes the
                // window, which comes back as `Moved`/`Resized` events. With
                // the handlers already live, the module's `restoring` flag
                // suppresses them instead of letting them overwrite the state
                // being restored. Registering afterwards would leave that flag
                // guarding nothing.
                window_state::track(&window);
                window_state::restore(&window);
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
            // Closing the main window quits the whole app (settings, debug, and
            // viewer windows included) — but only once the quit gate says so.
            // With work in flight the gate holds the exit, asks the user, and
            // runs its own countdown; the window must STAY OPEN for that, or
            // the dialog it's about to show goes with it. Nothing is torn down
            // on this path until the gate has waved the quit through, so a
            // "Keep working" leaves AI, MCP, and mDNS running. See `quit/`.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                if quit::request_quit(window.app_handle()) == quit::QuitOutcome::Held {
                    api.prevent_close();
                } else {
                    ai::manager::shutdown();
                    mcp::stop_mcp_server();
                    #[cfg(any(target_os = "macos", target_os = "linux"))]
                    network::mdns_discovery::stop_discovery();
                    window.app_handle().exit(0);
                }
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
                // ⌘Q, the app menu's Quit, the Dock's Quit, a logout or
                // restart, and every `AppHandle::exit` in the app all land
                // here. With non-instant work in flight the gate holds the
                // exit and takes the decision over (dialog + its own
                // countdown); with nothing running this is a pass-through and
                // the app quits exactly as it always did.
                //
                // A `restart()` carries `RESTART_EXIT_CODE`, for which Tauri
                // ignores `prevent_exit` outright — asking there would show a
                // dialog nobody could answer, so the gate never sees it.
                tauri::RunEvent::ExitRequested { ref api, code, .. } => {
                    if code != Some(tauri::RESTART_EXIT_CODE)
                        && quit::request_quit(_app) == quit::QuitOutcome::Held
                    {
                        api.prevent_exit();
                    }
                }
                tauri::RunEvent::Exit => {
                    // Flush window geometry synchronously: the debounced writer
                    // may have a pending change the process won't outlive.
                    window_state::save_on_exit(_app);

                    // Stop any live search walk. Coverage stays honest either way
                    // (a directory is marked listed only once its rows are
                    // written, so a walk cut off mid-flight claims nothing it
                    // didn't read), but a walk reading a disk for a window that
                    // no longer exists is work nobody asked for.
                    search::cancel_all_live_runs();

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
