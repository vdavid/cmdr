//! System memory measurement — used by the AI RAM gauge and available for other features.
//!
//! On macOS, uses `host_statistics64` (Mach API) for accurate, non-overlapping
//! memory categories. Falls back to `sysinfo` on other platforms.

/// System memory breakdown returned to frontend for the RAM gauge.
/// Categories are non-overlapping and sum to `total_bytes`.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemMemoryInfo {
    pub total_bytes: u64,
    /// Wired + compressor-occupied memory (kernel, drivers — can't be freed).
    pub wired_bytes: u64,
    /// App memory: active + inactive - purgeable (process memory the user can free by quitting apps).
    pub app_bytes: u64,
    /// Free: free + purgeable + speculative (available for new allocations).
    pub free_bytes: u64,
}

/// Returns system memory breakdown using macOS `host_statistics64` for accurate,
/// non-overlapping categories (unlike `sysinfo` where used + available > total).
#[tauri::command]
pub fn get_system_memory_info() -> SystemMemoryInfo {
    get_system_memory_info_inner()
}

/// Testable inner function that reads macOS vm_statistics64 via Mach API.
pub fn get_system_memory_info_inner() -> SystemMemoryInfo {
    #[cfg(target_os = "macos")]
    {
        macos_memory_info()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Fallback for non-macOS: use sysinfo (best effort)
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total = sys.total_memory();
        let used = sys.used_memory();
        let free = total.saturating_sub(used);
        SystemMemoryInfo {
            total_bytes: total,
            wired_bytes: 0,
            app_bytes: used,
            free_bytes: free,
        }
    }
}

/// Reads macOS vm_statistics64 via `host_statistics64` for accurate memory breakdown.
#[cfg(target_os = "macos")]
fn macos_memory_info() -> SystemMemoryInfo {
    use std::mem;

    let total_bytes = {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        sys.total_memory()
    };

    // Safety: calling Mach kernel API with proper struct size.
    let page_size: u64;
    let (wired_pages, compressor_pages, internal_pages, purgeable_pages);

    unsafe {
        page_size = libc::sysconf(libc::_SC_PAGESIZE) as u64;

        #[allow(deprecated, reason = "libc says use mach2, but not worth a new dep for one call")]
        let host = libc::mach_host_self();
        let mut vm_info: libc::vm_statistics64 = mem::zeroed();
        let mut count = (size_of::<libc::vm_statistics64>() / size_of::<libc::integer_t>()) as u32;

        let ret = libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            &mut vm_info as *mut _ as *mut libc::integer_t,
            &mut count,
        );

        if ret != libc::KERN_SUCCESS {
            log::warn!("host_statistics64 returned {ret}, falling back to sysinfo");
            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            let used = sys.used_memory();
            return SystemMemoryInfo {
                total_bytes,
                wired_bytes: 0,
                app_bytes: used,
                free_bytes: total_bytes.saturating_sub(used),
            };
        }

        wired_pages = vm_info.wire_count as u64;
        compressor_pages = vm_info.compressor_page_count as u64;
        // internal_page_count = anonymous pages owned by processes (what Activity Monitor calls "App Memory").
        // Unlike active+inactive, this excludes file-backed cache that macOS freely reclaims.
        internal_pages = vm_info.internal_page_count as u64;
        purgeable_pages = vm_info.purgeable_count as u64;
    }

    let wired_bytes = (wired_pages + compressor_pages) * page_size;
    let app_bytes = internal_pages.saturating_sub(purgeable_pages) * page_size;
    // Free = everything not wired or app (includes file cache, inactive, purgeable, speculative)
    let free_bytes = total_bytes.saturating_sub(wired_bytes + app_bytes);

    SystemMemoryInfo {
        total_bytes,
        wired_bytes,
        app_bytes,
        free_bytes,
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
