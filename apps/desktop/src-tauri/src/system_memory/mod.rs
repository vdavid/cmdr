//! System memory measurement, used by the AI RAM gauge and available for other features.
//!
//! Each OS reads it its own way behind the `platform/` seam (`docs/guides/platform-seams.md`):
//! macOS from the Mach VM statistics, for accurate non-overlapping categories; the others from
//! `sysinfo`, best effort.

mod platform;

/// System memory breakdown returned to frontend for the RAM gauge.
/// Categories are non-overlapping and sum to `total_bytes`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SystemMemoryInfo {
    pub total_bytes: u64,
    /// Wired + compressor-occupied memory (kernel, drivers; can't be freed).
    pub wired_bytes: u64,
    /// App memory: active + inactive - purgeable (process memory the user can free by quitting
    /// apps).
    pub app_bytes: u64,
    /// Free: free + purgeable + speculative (available for new allocations).
    pub free_bytes: u64,
}

/// Returns the system memory breakdown, in non-overlapping categories (unlike `sysinfo`'s, where
/// used + available > total).
#[tauri::command]
#[specta::specta]
pub fn get_system_memory_info() -> SystemMemoryInfo {
    get_system_memory_info_inner()
}

/// Testable inner function: this OS's reading.
pub fn get_system_memory_info_inner() -> SystemMemoryInfo {
    platform::system_memory_info()
}

/// The `sysinfo` breakdown: no wired category, everything used counted as app memory. The answer
/// on an OS without a finer reader, and macOS's fallback when its Mach call fails.
fn sysinfo_breakdown() -> SystemMemoryInfo {
    let (total, used) = sysinfo_memory();
    SystemMemoryInfo {
        total_bytes: total,
        wired_bytes: 0,
        app_bytes: used,
        free_bytes: total.saturating_sub(used),
    }
}

/// Total and used bytes from `sysinfo`, or zeros when it can't read the system (logged).
fn sysinfo_memory() -> (u64, u64) {
    match sysinfo::System::new() {
        Ok(mut sys) => {
            sys.refresh_memory();
            (sys.total_memory(), sys.used_memory())
        }
        Err(e) => {
            log::warn!("sysinfo couldn't read the system's memory: {e}");
            (0, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_memory_info_adds_up() {
        let info = get_system_memory_info_inner();

        // Total must be positive (every machine has RAM)
        assert!(info.total_bytes > 0, "total_bytes should be positive");

        // Non-overlapping segments must sum to total
        let sum = info.wired_bytes + info.app_bytes + info.free_bytes;
        assert_eq!(
            sum, info.total_bytes,
            "wired ({}) + app ({}) + free ({}) = {} != total ({})",
            info.wired_bytes, info.app_bytes, info.free_bytes, sum, info.total_bytes,
        );

        // Each segment should be reasonable (not more than total)
        assert!(info.wired_bytes <= info.total_bytes);
        assert!(info.app_bytes <= info.total_bytes);
        assert!(info.free_bytes <= info.total_bytes);
    }

    #[test]
    fn test_system_memory_info_serialization() {
        let info = SystemMemoryInfo {
            total_bytes: 68_719_476_736,
            wired_bytes: 5_000_000_000,
            app_bytes: 30_000_000_000,
            free_bytes: 33_719_476_736,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"totalBytes\":68719476736"));
        assert!(json.contains("\"wiredBytes\":5000000000"));
        assert!(json.contains("\"appBytes\":30000000000"));
        assert!(json.contains("\"freeBytes\":33719476736"));
    }
}
