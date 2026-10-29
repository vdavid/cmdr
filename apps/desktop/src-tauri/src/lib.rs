// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// It's there rather than here so the crates under `crates/` share one definition.
//
// This one can't go with them: it's judged per compilation unit, so as a
// package-wide flag every bin, integration test, and bench would report ~100
// "unused extern crate" errors for deps only the lib uses. It catches
// platform-specific cfg mismatches, and the `use foo as _;` markers it needs
// live in `crate_deps.rs`.
#![warn(unused_crate_dependencies)]

// These host primitives live in `cmdr-fs` so every crate in the workspace shares
// one copy, and are re-exported here at their original paths: poison-free
// locking, the count/noun formatter, thread QoS, the process-memory readers, and
// the SQLite connection factories over the one process-wide page-cache slab.
// `crate::ignore_poison::…`, `crate::pluralize::…`, `crate::thread_qos::…`,
// `crate::process_memory::…`, and `crate::sqlite_util::…` all still resolve.
pub use cmdr_fs::ignore_poison::IgnorePoison;
pub use cmdr_fs::{ignore_poison, pluralize, process_memory, sqlite_util, thread_qos};

mod ipc;

mod logging;

#[cfg(target_os = "macos")]
mod accent_color;
#[cfg(target_os = "linux")]
mod accent_color_linux;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod adb;
pub mod agent;
mod ai;
mod analytics;
mod app_lifecycle;
// Core Foundation property lists as `plist::Value`, shared by every CFPreferences reader.
/// Test-only: invariants over the `capabilities/` manifests, which no other
/// code references (Tauri reads them at build time).
#[cfg(test)]
mod capabilities;
#[cfg(target_os = "macos")]
mod cf_plist;
mod child_window_state;
// The `cmdr` terminal command: the URL its script sends and the app menu's install link.
// macOS-only, like the `RunEvent::Opened` delivery it feeds (`reveal/`).
#[cfg(target_os = "macos")]
mod cli;
mod clipboard;
mod commands;
pub mod config;
mod crash_reporter;
mod crate_deps;
mod data_wipe;
/// The dialog gallery's fixture tree (Debug > Soft dialogs). Dev and E2E builds:
/// `dialog-inset.spec.ts` drives the gallery, and the disk-backed dialogs need a
/// real tree to scan. Never in a shipped build.
#[cfg(any(debug_assertions, feature = "playwright-e2e"))]
pub mod dev_fixtures;
mod device_volumes;
mod diagnostics_snapshot;
#[cfg(target_os = "macos")]
mod display_accessibility;
// The macOS Dock: reading `persistent-apps` and putting Cmdr in it. No other platform has one.
#[cfg(target_os = "macos")]
mod dock;
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
// Is this copy of Cmdr in an Applications folder? Shared by the Dock pin and the reveal handler,
// which must not disagree about it. macOS-only, like both of them and the `running_bundle()` it
// asks.
#[cfg(target_os = "macos")]
mod glass_tint;
#[cfg(target_os = "macos")]
mod install_location;
mod instance_lock;
mod intl;
pub mod licensing;
#[cfg(target_os = "linux")]
pub(crate) mod linux_distro;
#[cfg(target_os = "linux")]
mod linux_icons;
mod listing_host;
mod listing_index_sizes;
mod location;
#[cfg(target_os = "macos")]
mod macos_icons;
mod main_window_show;
mod main_window_visibility;
// What an organization's MDM profile restricts (telemetry, updates, AI). Gated like `network`, whose
// deps (`url`, `semver`) it shares.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod managed_policy;
mod mcp;
mod menu;
#[cfg(target_os = "macos")]
mod mouse_nav;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod mtp;
mod multi_rename;
#[cfg(target_os = "macos")]
mod native_drag;
mod net;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network;
mod notifications;
#[cfg(feature = "playwright-e2e")]
mod open_mock;
mod operation_log_host;
mod operations_host;
#[cfg(target_os = "macos")]
mod permissions;
#[cfg(target_os = "linux")]
mod permissions_linux;
mod platform;
pub mod priority;
mod prod_instance;
mod quick_look;
mod quit;
pub mod recents;
mod redact;
mod restricted_paths;
// Not `#[cfg(target_os = "macos")]`, though every part of the mechanism is: `RevealDelivered`
// has to resolve on every platform for `ipc.rs`'s `collect_events!`, which can't cfg-gate
// inline. The three macOS-only submodules are gated inside (`reveal/mod.rs`).
mod reveal;
mod s3_costs;
pub mod search;
mod secrets;
mod security_advisories;
mod security_log;
mod security_notices;
pub mod selection;
mod send_schedule;
// `Cmdr > Services`: what Cmdr tells AppKit it can hand a service, and what the selection is at the
// moment one is picked. macOS only. An outer `///` here would merge with the module's own `//!`
// header and break its intra-doc links (see the `rustdoc` check's hint).
mod server_request;
// A saved SFTP, WebDAV, or S3 server moving to a new address, and everything
// that follows it. Gated with `network`, whose stores it moves.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod server_move;
// Saved servers: listed, saved, edited, dialed. Below `commands` and `agent`. Gated with `network`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod saved_servers;
#[cfg(target_os = "macos")]
pub mod services_menu;
mod settings;
// The saved-and-live SFTP and WebDAV servers, as volume rows. Gated with
// `network`, whose stores it reads.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod server_volumes;
mod short_id;
mod space_poller;
/// Ask Spotlight which folders this user has been working in. One narrow question,
/// coupled to nothing: see the module's own docs.
mod spotlight;
mod state_recovery;
mod subprocess;
mod system_events;
mod system_memory;
mod system_strings;
mod tauri_builder;
pub mod test_mode;
/// The sanctioned way to wait for background work in a Rust test. See `docs/testing.md`.
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(target_os = "macos")]
mod text_size;
/// Why a volume id has no registered volume: not connected yet, or gone.
mod unregistered_volumes;
mod update_schedule;
#[cfg(target_os = "macos")]
mod updater;
mod usage;
mod usb_speed;
mod volume_broadcast;
mod volume_host;
mod volume_listing;
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

use tauri::Manager;

// The Tauri command surface lives in `ipc.rs`, which
// exposes them through a typed `tauri_specta::Builder`. See `ipc.rs` for the
// migration recipe.

// ── On this file's length ────────────────────────────────────────────────────
//
// What's left is the crate's module map plus `run()`, and `run()` is one strictly
// ORDERED sequence whose order is load-bearing in ways the code says out loud (the
// panic hook goes in before anything in this crate can panic; the E2E data-dir
// guard goes in before anything resolves persisted state).
//
// ❌ Don't shorten it by lifting stretches of that sequence into `startup/*.rs`
// helpers. That buys line count and pays for it by making the ordering constraints
// invisible at the call site, which is exactly the class of silent detachment this
// repo keeps getting bitten by. The cut that IS right is by phase, for phases that
// are genuinely independent of each other: `tauri_builder::configure` holds the
// pre-`setup` registrations, where nothing observes app state or cares what ran
// before it. A new order-free registration belongs there; anything that has to
// happen after another step belongs in the spine below.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before anything in this crate can panic. It can't write a crash file until
    // `crash_reporter::init` resolves the data dir; see `crash_reporter/CLAUDE.md`.
    crash_reporter::install_panic_hook();

    // Every OS-default dir hangs off the build's own identifier (#820): record it first.
    let context = tauri::generate_context!();
    config::set_app_identifier(&context.config().identifier);

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

    // The `cmdr-media://` scheme and every plugin. Order-free by construction, so
    // it lives in one place off the spine; see `tauri_builder.rs`.
    tauri_builder::configure(tauri::Builder::default())
        .setup(move |app| {
            // Everything the listing engine needs from this app, first: nothing may
            // open a listing before its events and runtime are wired. Mirror of
            // `cmdr_listing::host`, which declares the other side of each seam.
            listing_host::install(app.handle());

            // Everything a storage backend needs from this app, in one place.
            // Must run before any volume is constructed. Mirror of
            // `cmdr_fs::volume::host`, which declares the other side of each seam.
            volume_host::install(app.handle());

            // The operation log, detached until `operation_log_host::start` opens it
            // below: anything that journals or rolls back reaches this one handle.
            let operation_log = operation_log_host::install();

            // The write engine, journaling into the log above (passed by value,
            // never re-read). ❗ Startup order is a contract: the operation log,
            // then this, then the leftover sweep below, then `init_volume_manager`.
            // Its runtime is the volume host's, so after it; unconditional, so a
            // data-dir failure still leaves an app that can copy. Mirror of
            // `cmdr_operations::host`.
            operations_host::install(app.handle(), operation_log.clone());

            // Mount the typed `tauri-specta` events onto the app. Required before any
            // `Event::emit` / `Event::listen` resolves a name. Collection: `ipc.rs`.
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

            // Claim the data, cache, and log dirs before anything opens a database AND before the logger: two
            // processes on one index corrupt it silently, and a refused one must not touch the
            // running app's `cmdr.log` either (#812). It exits before the (hidden) main window is
            // ever shown. See `instance_lock.rs`.
            let claim = instance_lock::claim_app_dirs_or_exit(app.handle());

            logging::startup::init();
            claim.log(); // what the claim found, now that a logger can record it

            // Everything the index needs from this app, in one place. Must run before anything
            // can start background work, and AFTER the instance lock: building the index moves a
            // drive index an older build left in the data dir, which is only safe while no other
            // process can have those files open. Mirror of `indexing/host/`, which declares the
            // other side of each seam.
            index_host::install(app.handle());

            // Snapshot the diagnostics id into a cheap static before anything that might crash,
            // so the panic hook can read it without allocating or locking. Mints both install
            // ids on first launch.
            install_id::init();

            // The security event log, before anything that records into it: the managed policy
            // right below notes itself there at launch.
            security_log::init(app.handle());
            // The in-app notice for a refused access attempt hangs off the same log (#457).
            security_notices::init(app.handle());

            // Set an unreadable settings, shortcuts, app-status, or license file aside before
            // anything loads one: the store plugin would read it as empty and its next save would
            // overwrite the evidence. After the instance lock (one process renames) and the
            // security log (each recovery is recorded), before the policy and crash reporter
            // below read `settings.json`. The notice waits in a queue until the main window asks.
            state_recovery::init(app.handle());
            state_recovery::recover_plugin_stores(app.handle());

            // The organization's managed policy, before anything that might send: the crash
            // reporter's next-launch path below is the first. The read is lazy anyway (no caller
            // can see an unloaded policy); this puts its one main-thread `cfprefsd` trip here.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            managed_policy::init(app.handle());

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
            cmdr_fs::benchmark::init_benchmarking();

            // Initialize the file watcher manager with app handle for events
            file_system::init_watcher_manager(app.handle().clone());

            // Backstop reaper for orphaned directory listings. The primary, fast
            // eviction is the FE-fired `list_directory_end` IPC; this only catches
            // listings whose close IPC was never delivered (a thrown FE handler, an
            // `$effect` teardown that threw), so their entry vector + OS watcher would
            // otherwise pin for the rest of the session. Mirrors the search index's
            // backstop timer and the viewer's window-`Destroyed` net.
            listing_host::listings().start_orphan_reaper();

            // Stash the AppHandle for the viewer's per-session watcher manager
            // threads so they can emit `viewer:file-changed:<sid>` events.
            file_viewer::init_app_handle(app.handle().clone());

            // Finish a "Delete all Cmdr data" that a crash cut short: its tombstones sit beside
            // the data dirs, which nothing reads. Background thread; a few directory reads.
            data_wipe::sweep_interrupted(app.handle());

            // Point preview-in-zip temp-extraction at a per-instance dir under the app
            // data dir (so side-by-side dev/prod/worktree instances never reap each
            // other's live temps), and reap any `.cmdr-viewer-*` orphan left by a crash.
            let data_dir = config::resolved_app_data_dir(app.handle()).ok();
            if let Some(data_dir) = &data_dir {
                file_viewer::init_materialize_dir(data_dir.join("viewer-extract"));
                // "Open with" on a file inside an archive gets its own dir and prefix
                // for the same reason, and its own startup-only reaper.
                file_viewer::init_open_with_extract_dir(data_dir.join("open-with-extract"));
                // The report dialogs' "see everything" files: per instance too, cleared at startup.
                error_reporter::preview_file::init_report_preview_dir(data_dir.join("report-preview"));
            }

            // The write engine claims its leftover ledger: points it at the data dir
            // and settles what an earlier run left (a quit or a crash mid-copy),
            // each under the rules its kind gets. After the instance lock above (no
            // other process holds this data dir) and before any copy can start, so
            // nothing we're about to write is in the set we sweep. Unconditional: with
            // no data dir the ledger runs unpersisted and the engine reports it. Runs
            // before the volume registry below by design: a leftover on a share is
            // held and settled when that volume arrives, rather than chased from here.
            // See `operations_host::claim_ledger`.
            operations_host::claim_ledger(data_dir.as_deref());

            // Initialize the volume manager with the root volume
            file_system::init_volume_manager();

            // Read which File Provider domains exist and what their extensions declare, on
            // a thread of its own, so the first right-click in Dropbox or Google Drive
            // already finds its actions.
            #[cfg(target_os = "macos")]
            file_system::file_provider_actions::warm();

            // Stash the AppHandle so a share that stays on the slow kernel mount
            // can say so. Session-state transitions need no handle here: they go
            // through the volume host's event seam, wired in `volume_host`.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::os_mount_notice::set_app_handle(app.handle().clone());

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
            mtp::install_connection_manager(app.handle());
            // File MTP as a device provider, so the volume list, eject, and path
            // resolution see its storages. `device_volumes` is the seam.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            mtp::volume_wiring::install_device_provider();
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            adb::volume_wiring::install_device_provider();

            // `volumes-ejecting-changed`, the twin of the write engine's
            // `volumes-busy-changed`: which Eject controls show an eject still
            // running. Before the frontend can ask for one.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            file_system::volume::eject::init_ejecting_volume_emitter(app.handle());

            // The status-corner indicator subscribes to this rather than polling `main.db`.
            agent::suggested_ops::init_suggestions_event_emitter(app.handle());
            // Every Ask Cmdr turn — a rail send or a wake — streams over this one event, keyed
            // by conversation. Before the agent starts, since a wake can fire from launch replay.
            agent::chat::stream::init_turn_event_emitter(app.handle());
            // Same, for the status corner's wake indicator and the one thing a wake says out
            // loud: it staged something to review (agent/wake/{indicator,staged}.rs).
            agent::wake::init_wake_status_emitter(app.handle());
            agent::wake::init_wake_staged_emitter(app.handle());

            // Restricted-paths tracker (TCC-gated paths the user has been
            // denied access to). Installs an NSApplicationDidBecomeActive
            // observer that re-probes the set when the user returns from
            // System Settings. See `crate::restricted_paths`.
            restricted_paths::init(app.handle());

            // Start volume mount/unmount watcher
            #[cfg(target_os = "macos")]
            volumes::watcher::start_volume_watcher(app.handle());
            // Holds every DA-mediated unmount while the drive's index lets go (`volumes/unmount_approver/`).
            #[cfg(target_os = "macos")]
            volumes::unmount_approver::install_for_app();

            #[cfg(target_os = "linux")]
            volumes_linux::watcher::start_volume_watcher(app.handle());

            // Register the virtual MTP device (before the watcher so it's in the initial
            // snapshot) when requested. Two activation paths, unified in
            // `activate_from_env_if_requested`: an E2E run (CMDR_E2E_MODE=1) or a dev opt-in
            // (CMDR_VIRTUAL_MTP=1, or =<dir> for a custom backing dir). Non-MTP E2E shards opt
            // out via CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP to avoid racing the shared backing dir.
            // See `mtp/virtual_device.rs::decide_startup_root` and `docs/tooling/virtual-mtp.md`.
            #[cfg(feature = "virtual-mtp")]
            mtp::virtual_device::activate_from_env_if_requested(test_mode::is_e2e_mode());

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

            // Follow the ADB server's device list (`host:track-devices`). Talks
            // only to the local server socket, never to USB, so no TCC prompt and
            // no FDA gate; with no `adb` installed the tracker stops itself
            // rather than retrying for the session (`recheck_adb_install` revives it).
            // The configured binary path is seeded FIRST, or the first
            // subscription runs against whatever the environment offered.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                adb::set_adb_binary_path(saved_settings.adb_binary_path.clone());
                adb::set_adb_enabled_flag(saved_settings.adb_enabled.unwrap_or(true));
                if saved_settings.adb_enabled.unwrap_or(true) {
                    adb::start_adb_tracker();
                }
            }

            // Emit initial volume list (after watchers start so MTP devices can connect)
            volume_broadcast::emit_volumes_changed_now();

            // Load known network shares from disk
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::known_shares::load_known_shares(app.handle());

            // And the two SFTP stores: the approved host keys and the saved
            // servers. Before any volume is built, so the first dial of a session
            // recognizes a server the user already trusted instead of asking again.
            network::load_sftp_stores(app.handle());
            // And the WebDAV server list, for the same picker.
            network::load_webdav_stores(app.handle());
            network::load_s3_stores(app.handle());
            network::load_nfs_stores(app.handle());

            // The tools' location resolver remembers every root a volume held, seeded
            // with each saved share's last mount path, so a path a model got before a
            // remount still resolves after it. After the share store above (its seed).
            location::facts::start_root_history();
            // Saved shares recorded before Cmdr kept server GUIDs get one from a
            // credential-free probe, in the background once startup settles.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::smb_server_guid::schedule_startup_probe();

            // Load persisted recent search history into the in-memory cache.
            search::history::RECENT_SEARCHES.load(app.handle());

            // Same for recent selections (Selection dialog history).
            selection::history::RECENT_SELECTIONS.load(app.handle());
            multi_rename::preset_store::PRESETS.load(app.handle());
            multi_rename::presets::LAST_SPEC.load(app.handle());
            multi_rename::history::FIELD_HISTORY.load_seeded(app.handle(), multi_rename::history::history_seed);

            // Same for recent paths (Go to path dialog history).
            go_to_path::history::RECENT_PATHS.load(app.handle());

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

            // Observe macOS Accessibility > Display option changes (Reduce transparency,
            // Increase contrast, Differentiate without color)
            #[cfg(target_os = "macos")]
            display_accessibility::observe_display_accessibility_changes(app.handle().clone());
            #[cfg(not(target_os = "macos"))]
            stubs::display_accessibility::observe_display_accessibility_changes(app.handle().clone());

            // Follow the macOS 27 Appearance > Liquid Glass slider
            #[cfg(target_os = "macos")]
            glass_tint::observe_glass_tint_changes(app.handle().clone());
            #[cfg(not(target_os = "macos"))]
            stubs::glass_tint::observe_glass_tint_changes(app.handle().clone());

            // Watch the mouse's back / forward navigation. macOS only: the mouse's own
            // driver decides what the press becomes, and a Logi Options+ mouse posts a
            // swipe gesture with no button behind it, so AppKit is the only layer that
            // sees both shapes. On Linux the frontend reads the buttons off the DOM.
            #[cfg(target_os = "macos")]
            mouse_nav::install(app.handle().clone());

            // Observe macOS Accessibility > Display > Text Size changes
            #[cfg(target_os = "macos")]
            text_size::observe_system_text_size_changes(app.handle().clone());

            // Follow a live macOS language or region change, so `'system'` means the
            // language the user reads NOW and the conventions they format in NOW,
            // not the ones they had at launch. No-op off macOS (no equivalent signal
            // exists; see the module doc).
            intl::observe_os_locale_changes(app.handle().clone());

            // Initialize font metrics. Loads the default 12px set plus any other
            // sizes the user has previously picked via the text-size slider.
            font_metrics::init_font_metrics(app.handle(), "system-400-12");
            font_metrics::load_all_metrics_from_disk(app.handle());

            // Sync the runtime `network.enabled` flag from settings so BE-side upgrade paths
            // can gate themselves correctly (default `true`).
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::set_network_enabled_flag(saved_settings.network_enabled.unwrap_or(true));

            // Discovery browses only while something needs it (`network/discovery_gate.rs`).
            // At launch, returning users who've already answered the OS Local Network prompt
            // get a short warm-up browse, so the Servers view opens on known servers. Fresh
            // installs stay quiet: the first network action is what asks. E2E builds always
            // warm up so virtual hosts are populated before tests run.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            network::mdns_discovery::install(app.handle());

            #[cfg(any(target_os = "macos", target_os = "linux"))]
            let should_warm_network_at_launch = saved_settings.network_enabled.unwrap_or(true)
                && (saved_settings.network_first_trigger_done.unwrap_or(false) || cfg!(feature = "smb-e2e"));

            #[cfg(any(target_os = "macos", target_os = "linux"))]
            if should_warm_network_at_launch {
                network::discovery_gate::hold_for(network::discovery_gate::LAUNCH_WARM_UP);

                #[cfg(feature = "smb-e2e")]
                network::virtual_smb_hosts::setup_virtual_smb_hosts(app.handle());
            }

            // Apply direct SMB connection setting (default: true)
            file_system::set_direct_smb_enabled(saved_settings.direct_smb_connection.unwrap_or(true));
            file_system::git::wiring::set_virtual_portal_enabled(saved_settings.show_virtual_git_portal.unwrap_or(true));
            // The one portal every `.git` browse, IPC command, and watcher
            // subscription shares, reporting repo changes into this window.
            file_system::git::wiring::install_git_portal(app.handle());
            // Index size updates reach only the listings they touch (`listing_index_sizes`);
            // its open-listing observer is installed with the listing host.
            listing_index_sizes::start(app.handle().clone());
            listing_host::listings().set_scratch_visibility(cmdr_listing::host::ScratchVisibility {
                show_safe_save_files: saved_settings.show_safe_save_files.unwrap_or(true),
                show_staging_temps: saved_settings.show_staging_temp_files.unwrap_or(false),
            });
            file_system::set_smb_concurrency(saved_settings.smb_concurrency.unwrap_or(10) as usize);

            // Initialize disk space poller (live status bar updates + low-disk-space warning)
            space_poller::init(app.handle());
            space_poller::apply_saved_settings(&saved_settings);
            space_poller::start();

            // Start the anonymous beta-analytics heartbeat (launch beat + hourly). Consent-gated
            // and suppressed in dev/CI; see `analytics/CLAUDE.md`.
            analytics::init(app.handle());
            analytics::start();
            // One `app_launched` event per startup, through the same consent gate.
            analytics::events::capture("app_launched", serde_json::json!({}));
            // The session-length ladder, anchored HERE rather than at quit: a crash or a
            // force-quit leaves no moment to report in. See `analytics/session.rs`.
            analytics::session::start();

            // Upgrade existing SMB mounts to direct smb2 connections (background, non-blocking).
            // No `firstTriggerDone` gate here: the function is a no-op when there are no SMB
            // mounts (no network activity, no prompt). When there ARE mounts and direct-SMB is
            // enabled, the pass holds the mDNS browse itself so the Keychain lookup can resolve
            // hostnames — same shape as the manual "Connect directly" and mount-time paths.
            // The macOS Local Network prompt fires once per app and only when an SMB mount is
            // present at launch.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            file_system::upgrade_existing_smb_mounts();

            // Before the menu bar is built: AppKit decides what the Services submenu may
            // contain from the send types the app has registered.
            #[cfg(target_os = "macos")]
            services_menu::install(app.handle());

            menu::install::at_startup(app, &saved_settings)?;

            // Before the first status check: an older version may have left the license key in
            // license.json in plain text; this moves it into the secret store.
            licensing::move_license_key_into_secret_store(app.handle());

            // Set the window title from the licence status. The same call runs again on every
            // status change, so activating a licence mid-session clears "Personal use only"
            // from the Dock's window list without a relaunch.
            licensing::refresh_window_title(app.handle());

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

            // What the encrypted-archive password prompt is asking, so `cmdr://state`
            // can name the archive and `unlock` can answer it.
            app.manage(mcp::ArchivePasswordPromptStore::new());

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

            // Start the importance scheduler: it subscribes to the volume-registration
            // bus, then sweeps the index registry for already-ready volumes, so a
            // volume's folder weights recompute when its index finishes scanning (or is
            // Fresh at launch). Independent of whether indexing auto-starts here — the
            // bus fires whenever any volume registers or any scan completes.
            //
            // ⚠️ The root index above starts on a SPAWNED task, so the sweep here
            // usually sees an empty registry and root reaches the scheduler on the
            // registration bus instead. Everything the scheduler owes a volume
            // therefore hangs off `wire_volume`, which both paths share, never off the
            // sweep alone. See `crates/cmdr-index/src/importance/scheduler/DETAILS.md`.
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
                    // Note today's local calendar day in `usage.json`, the ledger usage-gated
                    // hints read. Once per launch, and it only writes on the first launch of a
                    // day. The data dir is the isolated one for a dev or E2E instance, so those
                    // runs can't pollute the real ledger. See `usage/CLAUDE.md`.
                    usage::record_launch(&data_dir);
                    search::set_drive_index_dir(index_host::drive_index_dir(&data_dir));
                    search::start_importance_weight_subscriber(data_dir);
                }
                Err(e) => log::warn!("search importance weights and the launch-day ledger not wired: {e}"),
            }

            // Open the durable operation log on the handle installed above and spawn
            // its writer thread; from here on, the write pipeline's operations journal.
            operation_log_host::start(app.handle(), &operation_log);

            // Open the agent's durable store (`main.db`, peer to `operation-log.db`) and
            // register its handle. The chat runtime and the Ask Cmdr IPC commands are its
            // consumers; opening here runs the migration ladder at startup and keeps the
            // DB current.
            agent::start(app.handle());

            // Restore the main window's saved position and size. Placement
            // only: the window is still hidden here. The frontend shows it once
            // it mounts, and `main_window_show` shows it anyway if that never
            // happens. Only the main window persists across launches; Settings,
            // Debug, and viewer windows deliberately start fresh (in-session
            // position lives in `child_window_state`). See `window_state/`.
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
                main_window_show::arm_fallback(window);
            }

            Ok(())
        })
        .on_menu_event(menu::handle_menu_event)
        .invoke_handler(invoke_handler)
        .on_window_event(app_lifecycle::on_window_event)
        .build(context)
        .expect("error while building tauri application")
        .run(app_lifecycle::on_run_event);
}
