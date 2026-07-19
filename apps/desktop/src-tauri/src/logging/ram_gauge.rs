//! Optional per-log-line RAM gauge.
//!
//! When `CMDR_LOG_RAM_USE=1` (or `true`/`yes`/`on`) is set in the environment,
//! every log line is prefixed with the process's current memory use, right after
//! the level:
//!
//! ```text
//! 2026-07-16T22:56:21.829+02:00 DEBUG (374 MB) smb2::client::tree  tree: fs_info done, ...
//! ```
//!
//! It's a debugging tool for keeping RAM at bay: the inline number turns the log
//! into a memory timeline, so you can see which operation coincided with a jump.
//! (For *what* allocated, reach for Instruments or a heap profiler; this answers
//! *when and near what*.) Works in dev, E2E, and prod builds alike, since it lives
//! in the logger.
//!
//! ## What it measures
//!
//! `phys_footprint` of THIS process, via [`crate::process_memory`] (the same
//! metric the indexing watchdog and Activity Monitor's "Memory" column use, not
//! RSS). Cmdr is multi-process (Tauri): this is the Rust backend only, not the
//! WebView helper processes, so it's "backend RAM," not "total Cmdr RAM."
//!
//! ## Cost
//!
//! A background thread samples [`crate::process_memory::current_phys_footprint`]
//! every 100 ms into an atomic; the format closures read that atomic lock-free.
//! So the read is a syscall-free atomic load per line, and the syscall itself
//! happens 10x/second regardless of log volume. When the env flag is unset (the
//! default), [`tag`] returns an empty `String` with no allocation and no atomic
//! contention.
//!
//! ## Interaction with file-log dedup
//!
//! The file chain coalesces identical lines to defend against runaway-loop CPU
//! burn (`super::coalesce`), keyed on the line text. With the gauge ON, the
//! ever-changing RAM number makes lines non-identical, so a flood coalesces less.
//! Two reasons that's fine: the flag is an explicit debug opt-in (off in normal
//! runs, where dedup is fully intact), and 100 ms sampling caps distinct values
//! to ~10 per one-second window, so a tight loop still collapses from thousands
//! of writes to a few dozen, not back to thousands.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// Env var that turns the gauge on.
const ENV_VAR: &str = "CMDR_LOG_RAM_USE";

/// How often the background thread refreshes the reading.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

/// Whether the gauge is on (env flag was truthy at [`init`]).
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether the sampler thread has been started (guards against double-spawn if
/// [`init`] runs more than once, e.g. across test setups).
static STARTED: AtomicBool = AtomicBool::new(false);

/// Latest sampled `phys_footprint` in bytes. `0` means "not sampled yet."
static CURRENT_BYTES: AtomicU64 = AtomicU64::new(0);

/// Read [`ENV_VAR`] and, if truthy, enable the gauge and start the sampler.
///
/// Idempotent: only the first call that finds the flag set spawns the thread.
/// Safe to call before the async runtime exists (uses a plain OS thread).
pub fn init() {
    if !env_flag_enabled() {
        return;
    }
    ENABLED.store(true, Ordering::Relaxed);

    // Only the first caller spawns the single sampler thread.
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    // Prime an initial value so the very first log lines aren't "?? MB".
    sample_once();
    std::thread::Builder::new()
        .name("ram-gauge".to_string())
        .spawn(|| {
            loop {
                std::thread::sleep(SAMPLE_INTERVAL);
                sample_once();
            }
        })
        // If the thread can't spawn, the gauge just shows the primed value
        // forever; not worth failing app startup over a debug aid.
        .ok();
}

/// The prefix to splice in after the level, e.g. `"(374 MB) "`, or an empty
/// string when the gauge is off. Called once per emitted log line.
pub fn tag() -> String {
    if !ENABLED.load(Ordering::Relaxed) {
        return String::new();
    }
    format!("({}) ", format_footprint(CURRENT_BYTES.load(Ordering::Relaxed)))
}

/// Take one reading into [`CURRENT_BYTES`]. A failed query leaves the last value
/// in place (better a slightly stale number than a flapping `0`).
fn sample_once() {
    if let Some(bytes) = crate::process_memory::current_phys_footprint() {
        CURRENT_BYTES.store(bytes, Ordering::Relaxed);
    }
}

/// Whether [`ENV_VAR`] is set to a truthy value.
fn env_flag_enabled() -> bool {
    std::env::var(ENV_VAR).is_ok_and(|v| is_truthy(&v))
}

/// Whether an env-var value means "on."
fn is_truthy(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

/// Format bytes as `MB` below 1 GiB, else `GB` with two decimals. Binary units,
/// matching the watchdog's `gb()`/`mb()` and Activity Monitor's rounding intent.
fn format_footprint(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;
    if bytes == 0 {
        "?? MB".to_string()
    } else if bytes < GIB {
        format!("{} MB", bytes / MIB)
    } else {
        format!("{:.2} GB", bytes as f64 / GIB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_footprint_uses_mb_below_a_gib() {
        assert_eq!(format_footprint(374 * 1024 * 1024), "374 MB");
        assert_eq!(format_footprint(0), "?? MB");
        // Just under 1 GiB still reads as MB.
        assert_eq!(format_footprint(1023 * 1024 * 1024), "1023 MB");
    }

    #[test]
    fn format_footprint_uses_gb_at_and_above_a_gib() {
        assert_eq!(format_footprint(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_footprint(3 * 1024 * 1024 * 1024 + 512 * 1024 * 1024), "3.50 GB");
    }

    #[test]
    fn tag_is_empty_when_disabled() {
        // ENABLED defaults false and no test enables it, so tag() must not allocate a prefix.
        assert_eq!(tag(), "");
    }

    #[test]
    fn is_truthy_accepts_common_on_values_case_and_space_insensitively() {
        for on in ["1", "true", "TRUE", "Yes", " on ", "on"] {
            assert!(is_truthy(on), "{on:?} should be truthy");
        }
        for off in ["0", "false", "no", "off", "", "2", "enabled"] {
            assert!(!is_truthy(off), "{off:?} should be falsy");
        }
    }
}
