//! Wiring the logger up at app startup.
//!
//! One call, `startup::init()`, run from `lib.rs`'s `setup` before anything can
//! log. It resolves the log directory, reads the two settings that shape the
//! logger (the storage cap and the verbose toggle) through the early-load
//! helpers, installs the fern dispatch tree, sweeps legacy files, and records
//! what it decided.
//!
//! ❗ It runs before the full settings load, which is why the early-load helpers
//! exist at all: `dispatch::init` reads `RUST_LOG` and resets the stdout
//! threshold, so the verbose default has to be known first or the first records
//! go out at the wrong level.
//!
//! Gotcha: the `CMDR_LOG_DIR` / `CMDR_DATA_DIR` / per-OS resolution below is
//! MIRRORED by `settings::loader`'s `early_load_*` helpers, which can't call
//! into here (no `AppHandle` yet, and they use `dirs::data_dir` + the bundle id
//! rather than Tauri's `app_data_dir`). A bundle-id change touches both.

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
}
