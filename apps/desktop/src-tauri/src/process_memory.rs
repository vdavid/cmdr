//! The canonical "how much memory is this process using" reader.
//!
//! One place owns the Mach `task_info` FFI so both the indexing memory watchdog
//! (machine-protection thresholds) and the log RAM gauge
//! ([`crate::logging::ram_gauge`]) read the SAME metric the SAME cheap way.
//!
//! **We report `phys_footprint`, not `resident_size` (RSS).** On macOS, RSS
//! counts GPU/WebView graphics mappings (the WebKit Metal compositor's
//! `IOAccelerator` region can be multiple GB) that are NOT real memory pressure.
//! `phys_footprint` is the metric macOS itself keys memory pressure and jetsam
//! on, and it's what Activity Monitor's "Memory" column shows. See
//! [`crate::indexing::memory_watchdog`] for the incident that established this.
//!
//! The per-read cost is one `task_info` syscall (single-digit microseconds, no
//! allocation), so callers can read it per watchdog tick or per log line freely.
//!
//! On non-macOS platforms [`current_phys_footprint`] returns `None` (the Mach
//! queries don't exist); callers degrade gracefully.

/// The cheap read: the current process's `phys_footprint` in bytes, or `None`
/// if the query failed or the platform has no Mach `task_info`.
pub(crate) fn current_phys_footprint() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        query_task_vm_info().map(|vm| vm.phys_footprint)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// What we extract from a `task_vm_info` query. The watchdog's snapshot path
/// wants the peak and resident size too, so this carries all three.
#[cfg(target_os = "macos")]
pub(crate) struct TaskVmInfoResult {
    pub(crate) phys_footprint: u64,
    pub(crate) phys_footprint_peak: Option<u64>,
    pub(crate) resident_size: u64,
}

/// The prefix of `task_vm_info` we read: everything up to and including
/// `ledger_phys_footprint_peak`.
///
/// `task_vm_info`'s layout differs from `mach_task_basic_info`, and its `count`
/// is measured in `natural_t` (u32) words. We request only this prefix's worth
/// of words; the kernel writes `min(requested, supported)` and reports the
/// actual back, so we gate each field on the returned count covering its byte
/// range (a very old kernel might predate the rev that added `phys_footprint` /
/// the ledger peak).
#[cfg(target_os = "macos")]
#[repr(C)]
struct TaskVmInfo {
    virtual_size: u64,
    region_count: i32,
    page_size: i32,
    resident_size: u64,
    resident_size_peak: u64,
    device: u64,
    device_peak: u64,
    internal: u64,
    internal_peak: u64,
    external: u64,
    external_peak: u64,
    reusable: u64,
    reusable_peak: u64,
    purgeable_volatile_pmap: u64,
    purgeable_volatile_resident: u64,
    purgeable_volatile_virtual: u64,
    compressed: u64,
    compressed_peak: u64,
    compressed_lifetime: u64,
    phys_footprint: u64,
    min_address: u64,
    max_address: u64,
    ledger_phys_footprint_peak: i64,
}

/// Query `task_vm_info` (`TASK_VM_INFO`, flavor 22) for `phys_footprint` (plus
/// the ledger peak and resident size the watchdog snapshot uses).
///
/// Uses raw FFI because the `libc` crate doesn't expose `TASK_VM_INFO`.
#[cfg(target_os = "macos")]
pub(crate) fn query_task_vm_info() -> Option<TaskVmInfoResult> {
    // Mach task info flavor (from <mach/task_info.h>).
    const TASK_VM_INFO: u32 = 22;

    // `count` is in `natural_t` (u32) words, per the `task_info` ABI.
    let requested_count = (size_of::<TaskVmInfo>() / size_of::<u32>()) as u32;

    #[allow(deprecated, reason = "mach_task_self is deprecated in libc but works fine")]
    // SAFETY: `info` is zeroed before use; `count` is the prefix's size in `natural_t` (u32) words,
    // which is how `task_info` with `TASK_VM_INFO` reports its length. `TaskVmInfo` is `#[repr(C)]`
    // and matches the leading fields of `task_vm_info`, so the kernel writes only within `info`
    // (it writes `min(requested, supported)` words). We read fields only after `result == 0` AND
    // the returned `count` covers each field's byte range.
    let (info, returned_count, result) = unsafe {
        let mut info: TaskVmInfo = std::mem::zeroed();
        let mut count = requested_count;
        let result = libc::task_info(
            libc::mach_task_self(),
            TASK_VM_INFO,
            &mut info as *mut TaskVmInfo as *mut i32,
            &mut count,
        );
        (info, count, result)
    };

    if result != 0 {
        log::debug!("process_memory: task_info(VM_INFO) failed with code {result}");
        return None;
    }

    // Only trust a field if the kernel actually wrote through its byte range.
    let covered = |byte_offset: usize, field_size: usize| -> bool {
        (returned_count as usize) * size_of::<u32>() >= byte_offset + field_size
    };

    if !covered(std::mem::offset_of!(TaskVmInfo, phys_footprint), size_of::<u64>()) {
        log::debug!("process_memory: task_info(VM_INFO) returned too few words for phys_footprint");
        return None;
    }

    let phys_footprint_peak = if covered(
        std::mem::offset_of!(TaskVmInfo, ledger_phys_footprint_peak),
        size_of::<i64>(),
    ) && info.ledger_phys_footprint_peak > 0
    {
        Some(info.ledger_phys_footprint_peak as u64)
    } else {
        None
    };

    Some(TaskVmInfoResult {
        phys_footprint: info.phys_footprint,
        phys_footprint_peak,
        resident_size: info.resident_size,
    })
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn current_phys_footprint_returns_positive_value() {
        let phys = current_phys_footprint();
        assert!(phys.is_some(), "should be able to query phys_footprint");
        assert!(phys.unwrap() > 0, "phys_footprint should be positive");
    }
}
