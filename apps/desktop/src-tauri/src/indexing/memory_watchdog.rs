//! Memory watchdog: monitors the app's resident memory and takes action
//! at safety thresholds to prevent unbounded memory growth.
//!
//! - 8 GB: logs a warning.
//! - 16 GB: stops all indexing and emits a user-visible event.
//!
//! On non-macOS platforms this is a no-op stub (platform memory queries
//! differ and can be added later).

/// 8 GB in bytes.
#[cfg(target_os = "macos")]
const WARN_THRESHOLD: u64 = 8 * 1024 * 1024 * 1024;

/// 16 GB in bytes.
#[cfg(target_os = "macos")]
const STOP_THRESHOLD: u64 = 16 * 1024 * 1024 * 1024;

/// How often the watchdog checks memory (seconds).
#[cfg(target_os = "macos")]
const CHECK_INTERVAL_SECS: u64 = 5;

/// Start the memory watchdog as a fire-and-forget background task.
///
/// On macOS, spawns a task that checks resident memory every 5 seconds
/// using `mach_task_info`. Runs until the app is stopped or indexing is
/// halted due to excessive memory usage. On other platforms, this is a no-op.
#[cfg(target_os = "macos")]
pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        run_watchdog(app).await;
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
            log::error!(
                "Memory watchdog: resident memory is {} GB, exceeding {} GB safety limit. \
                 Stopping all indexing to prevent a system crash.",
                resident_bytes / (1024 * 1024 * 1024),
                STOP_THRESHOLD / (1024 * 1024 * 1024),
            );

            // Emit user-visible event
            use tauri::Emitter;
            let _ = app.emit(
                "index-memory-warning",
                serde_json::json!({
                    "resident_gb": resident_bytes / (1024 * 1024 * 1024),
                    "action": "stopped_indexing",
                }),
            );

            // Stop indexing
            if let Err(e) = super::stop_indexing() {
                log::error!("Memory watchdog: stop_indexing failed: {e}");
            }
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
