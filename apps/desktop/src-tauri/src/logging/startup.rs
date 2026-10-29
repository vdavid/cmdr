//! Wiring the logger up at app startup.
//!
//! One call, `startup::init()`, run from `lib.rs`'s `setup` right after the
//! instance lock claims the data dir (so a refused second instance never opens
//! the shared log files, #812). It resolves the log directory, reads the two settings that shape the
//! logger (the storage cap and the verbose toggle) through the early-load
//! helpers, installs the fern dispatch tree, sweeps legacy files, and records
//! what it decided.
//!
//! ❗ It runs before the full settings load, which is why the early-load helpers
//! exist at all: `dispatch::init` reads `RUST_LOG` and resets the stdout
//! threshold, so the verbose default has to be known first or the first records
//! go out at the wrong level.
//!
//! The log dir comes from `config::app_log_dir`, and the early-load helpers read
//! settings from `config::standalone_app_data_dir`; both hang off
//! `config::app_identifier`, the same identifier Tauri's `app_*_dir()` use.

use crate::logging;
use crate::pluralize;
use crate::settings;

/// Resolve the log directory, install the logger, and sweep what an older build left behind.
pub fn init() {
    // Hand-rolled fern dispatch tree (`logging::dispatch::init`) replaces
    // `tauri-plugin-log`. Why: per-output level filtering. File target locked at
    // Debug (error reports need the context); stdout defaults to Info (clean for
    // `pnpm dev`) with `RUST_LOG` per-module overrides applied to stdout only.
    // The verbose toggle bumps stdout to Debug via an AtomicU8, no logger
    // rebuild, no records lost.
    //
    // Priority and per-OS defaults: `config::app_log_dir`. The default follows the build's own
    // identifier, so a dev build never shares the release app's log folder (#820).
    let resolved_log_dir = crate::config::app_log_dir();

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
    // A logger collision installs nothing, file chain included.
    logging::set_file_logging_active(file_logging_enabled && init_result.is_ok());
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
}
