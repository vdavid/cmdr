//! Memory watchdog: monitors the app's resident memory and takes action
//! at safety thresholds to prevent unbounded memory growth.
//!
//! - 8 GB: logs a warning.
//! - 16 GB: stops EVERY volume's index and emits a user-visible event.
//!
//! **The budget is GLOBAL, not per-volume** (plan rabbit hole #8, resolved by
//! David). Scans run in parallel — the network/USB wire is the bottleneck, not
//! RAM — so there's no one-at-a-time serialization; instead a single process-
//! wide budget is the safety net that stops ALL indexing if total resident
//! memory crosses the catastrophe line. The 16 GB number is a machine-protection
//! stop, NOT expected usage (real scan memory is the accumulator maps + the 20K
//! writer channel — hundreds of MB per normal volume).
//!
//! On non-macOS platforms this is a no-op stub (platform memory queries
//! differ and can be added later).

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

/// 8 GB in bytes.
#[cfg(target_os = "macos")]
const WARN_THRESHOLD: u64 = 8 * 1024 * 1024 * 1024;

/// 16 GB in bytes.
#[cfg(target_os = "macos")]
const STOP_THRESHOLD: u64 = 16 * 1024 * 1024 * 1024;

/// How often the watchdog checks memory (seconds).
#[cfg(target_os = "macos")]
const CHECK_INTERVAL_SECS: u64 = 5;

/// Whether the single global watchdog task is already running. The watchdog is
/// process-wide (one global budget over all volumes), so the first `start()`
/// wins and later per-volume `start_indexing_for` calls are no-ops — without
/// this, every volume start would spawn a redundant watchdog loop all racing to
/// stop indexing.
#[cfg(target_os = "macos")]
static WATCHDOG_RUNNING: AtomicBool = AtomicBool::new(false);

/// Start the global memory watchdog as a fire-and-forget background task.
///
/// On macOS, spawns ONE task (idempotent across volumes) that checks resident
/// memory every 5 seconds using `mach_task_info`. On other platforms, no-op.
#[cfg(target_os = "macos")]
pub fn start(app: tauri::AppHandle) {
    // Idempotent: only the first caller spawns the single global watchdog.
    if WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        run_watchdog(app).await;
        // Let a future `start()` respawn it (e.g. after the budget stop returned).
        WATCHDOG_RUNNING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn start(_app: tauri::AppHandle) {
    // No-op on non-macOS platforms
}

#[cfg(target_os = "macos")]
async fn run_watchdog(app: tauri::AppHandle) {
    use std::time::Duration;

    let mut interval = tokio::time::interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    let mut warned = false;

    loop {
        interval.tick().await;

        let resident_bytes = match get_resident_memory() {
            Some(b) => b,
            None => continue,
        };

        if resident_bytes >= STOP_THRESHOLD {
            // Drives a user-visible toast; exactly the kind of error we want to ship
            // diagnostic context for when the user has opted in.
            crate::log_error!(
                "Memory watchdog: resident memory is {} GB, exceeding {} GB safety limit. \
                 Stopping all indexing to prevent a system crash.",
                resident_bytes / (1024 * 1024 * 1024),
                STOP_THRESHOLD / (1024 * 1024 * 1024),
            );

            // Emit user-visible event
            use tauri_specta::Event;
            let _ = super::IndexMemoryWarningEvent {
                resident_gb: resident_bytes / (1024 * 1024 * 1024),
                action: "stopped_indexing".to_string(),
            }
            .emit(&app);

            // Global budget: stop EVERY registered volume's index, not just
            // `root`. Scans run in parallel (the wire, not RAM, is the
            // bottleneck), so the safety net is one process-wide stop rather than
            // per-volume serialization (plan rabbit hole #8).
            super::state::stop_all_indexing();
            return;
        }

        if resident_bytes >= WARN_THRESHOLD && !warned {
            warned = true;
            log::warn!(
                "Memory watchdog: resident memory is {} MB ({} GB threshold approaching). \
                 Indexing continues but the system may be under pressure.",
                resident_bytes / (1024 * 1024),
                STOP_THRESHOLD / (1024 * 1024 * 1024),
            );
        }

        // Reset warning flag if memory drops back below the threshold
        if resident_bytes < WARN_THRESHOLD && warned {
            warned = false;
        }
    }
}

/// Query the current task's resident memory using `mach_task_basic_info`.
///
/// Uses raw FFI because the `libc` crate doesn't expose `MACH_TASK_BASIC_INFO`.
#[cfg(target_os = "macos")]
fn get_resident_memory() -> Option<u64> {
    // Mach task info constants (from <mach/task_info.h>)
    const MACH_TASK_BASIC_INFO: u32 = 20;

    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time_seconds: i32,
        user_time_microseconds: i32,
        system_time_seconds: i32,
        system_time_microseconds: i32,
        policy: i32,
        suspend_count: i32,
    }

    let info_count = (size_of::<MachTaskBasicInfo>() / size_of::<libc::c_int>()) as u32;

    #[allow(deprecated, reason = "mach_task_self is deprecated in libc but works fine")]
    // SAFETY: `info` is zeroed before use, and `count` is set to the struct's size measured in
    // `c_int` (natural_t) words, the count layout `task_info` with `MACH_TASK_BASIC_INFO` expects;
    // `MachTaskBasicInfo` is `#[repr(C)]` and matches the `mach_task_basic_info` layout, so the
    // kernel writes only within `info`. We read `info.resident_size` only when `result == 0`.
    unsafe {
        let mut info: MachTaskBasicInfo = std::mem::zeroed();
        let mut count = info_count;
        let result = libc::task_info(
            libc::mach_task_self(),
            MACH_TASK_BASIC_INFO,
            &mut info as *mut MachTaskBasicInfo as *mut i32,
            &mut count,
        );
        if result == 0 {
            Some(info.resident_size)
        } else {
            log::debug!("Memory watchdog: task_info failed with code {result}");
            None
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn get_resident_memory_returns_positive_value() {
        let mem = get_resident_memory();
        assert!(mem.is_some(), "should be able to query resident memory");
        assert!(mem.unwrap() > 0, "resident memory should be positive");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn thresholds_are_ordered() {
        const {
            assert!(
                WARN_THRESHOLD < STOP_THRESHOLD,
                "warn threshold must be below stop threshold"
            )
        };
    }
}
