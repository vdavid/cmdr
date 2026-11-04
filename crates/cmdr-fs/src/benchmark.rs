//! Performance timeline for "file loading" benchmarks.
//!
//! Enable with the `RUSTY_COMMANDER_BENCHMARK=1` environment variable. All events
//! are logged to stderr with microsecond timestamps. Pure std diagnostics, here
//! so the listing engine, the local reader, and the app's IPC commands share one
//! timeline. How to read it: `docs/guides/benchmarking-file-loading.md`.

// Benchmarks intentionally use eprintln! for raw stderr output (not log framework)
#![allow(
    clippy::print_stderr,
    reason = "Benchmarks bypass log framework for raw stderr output"
)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Global start time for relative timestamps
static BENCHMARK_ENABLED: AtomicBool = AtomicBool::new(false);

/// Epoch timestamp in microseconds (from std::time::Instant converted to u64)
static EPOCH_NANOS: AtomicU64 = AtomicU64::new(0);

/// Get current time as nanos since process start (approximation)
fn now_nanos() -> u64 {
    crate::rt::wall_now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Initialize benchmarking from environment variable.
/// Call this once during app startup.
pub fn init_benchmarking() {
    let enabled = std::env::var("RUSTY_COMMANDER_BENCHMARK")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    BENCHMARK_ENABLED.store(enabled, Ordering::SeqCst);

    if enabled {
        EPOCH_NANOS.store(now_nanos(), Ordering::SeqCst);
        eprintln!("[BENCHMARK] Rust benchmarking enabled");
    }
}

/// Check if benchmarking is enabled
pub fn is_enabled() -> bool {
    BENCHMARK_ENABLED.load(Ordering::Relaxed)
}

/// Get current timestamp in microseconds since epoch
fn now_micros() -> u64 {
    let current = now_nanos();
    let epoch = EPOCH_NANOS.load(Ordering::Relaxed);
    (current.saturating_sub(epoch)) / 1000
}

/// Reset the epoch (call when user navigates to a folder)
pub fn reset_epoch() {
    if !is_enabled() {
        return;
    }
    EPOCH_NANOS.store(now_nanos(), Ordering::SeqCst);
    log_event("EPOCH_RESET");
}

/// Log a benchmark event with current timestamp
pub fn log_event(event: &str) {
    if !is_enabled() {
        return;
    }
    let ts = now_micros();
    eprintln!("[TIMELINE] {:>10}μs | RUST | {}", ts, event);
}

/// Log a benchmark event with a specific value
pub fn log_event_value(event: &str, value: impl std::fmt::Display) {
    if !is_enabled() {
        return;
    }
    let ts = now_micros();
    eprintln!("[TIMELINE] {:>10}μs | RUST | {} = {}", ts, event, value);
}

/// Helper for timing a block of code: logs `START` on creation and `END` with the
/// elapsed microseconds on drop.
pub struct TimedBlock {
    name: String,
    start: u64,
}

impl TimedBlock {
    /// Starts timing the block named `name`.
    pub fn new(name: &str) -> Self {
        let start = now_micros();
        if is_enabled() {
            eprintln!("[TIMELINE] {:>10}μs | RUST | {} START", start, name);
        }
        Self {
            name: name.to_string(),
            start,
        }
    }
}

impl Drop for TimedBlock {
    fn drop(&mut self) {
        if is_enabled() {
            let end = now_micros();
            let duration = end - self.start;
            eprintln!("[TIMELINE] {:>10}μs | RUST | {} END ({}μs)", end, self.name, duration);
        }
    }
}
