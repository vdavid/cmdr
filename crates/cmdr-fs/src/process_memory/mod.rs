//! The canonical "how much memory is this process using" reader.
//!
//! One place owns the Mach and allocator FFI so both the indexing memory
//! watchdog (machine-protection thresholds) and the log RAM gauge
//! (the app's `logging::ram_gauge`) read the SAME metrics the SAME cheap way.
//! Policy (thresholds, what to do about a number) lives with the caller — the
//! app's `indexing::resources::memory_watchdog`; this module only reads.
//!
//! Five readers, four different accountants:
//!
//! (All but the first are macOS-only, so they're named here rather than linked:
//! an intra-doc link to a `cfg`-gated item is an unresolved link on every other
//! platform, and `cargo doc` is deny-warnings in CI, which builds on Linux.)
//!
//! - [`current_phys_footprint`] / `query_task_vm_info`: the kernel's view.
//! - `query_basic_info`: RSS and its high-water mark.
//! - `query_mimalloc_heap`: what OUR allocator has committed.
//! - `query_system_malloc_zones`: what the SYSTEM allocator holds.
//! - `query_vm_regions` (`vm_regions.rs`): the kernel's VM map folded by tag,
//!   plus a per-tag histogram of distinct region SIZES.
//!
//! **The middle two do not overlap, and neither alone is "the heap".** Cmdr sets
//! mimalloc as the global allocator (`main.rs`), and mimalloc is not a
//! registered macOS malloc zone: `malloc_zone_statistics` and
//! `malloc_get_all_zones` cannot see a single byte of the Rust heap. They see
//! WebKit, Objective-C, and C-library allocations only. Reading zone totals as
//! "the app's heap" is how a 16.5 GB `phys_footprint` got reported as a 1.6 GB
//! heap during the 2026-07 runaway.
//!
//! **`vmmap` gotcha:** mimalloc tags its arena `mmap`s with `os_tag` 100, which
//! macOS defines as `VM_MEMORY_IOACCELERATOR`. So in `vmmap` / `footprint`
//! output the `IOAccelerator` rows ARE the Rust heap, not GPU memory (verified
//! on macOS 15 with `MallocStackLogging=1` + `vmmap -fullStacks`: every 128 MB
//! `IOAccelerator` region backtraces to `mmap` ← `_mi_prim_alloc` ←
//! `mi_arena_reserve`, 2026-07). Don't read those rows as graphics.
//!
//! **We report `phys_footprint`, not `resident_size` (RSS).** RSS counts
//! graphics and shared mappings that are NOT real memory pressure.
//! `phys_footprint` is the metric macOS itself keys memory pressure and jetsam
//! on, and it's what Activity Monitor's "Memory" column shows.
//!
//! The per-read cost is one `task_info` syscall (single-digit microseconds, no
//! allocation), so callers can read it per watchdog tick or per log line freely.
//! The zone walk is heavier (it iterates every zone) and the region walk heavier
//! still (one syscall per map entry), so both are snapshot-only.
//!
//! On non-macOS platforms [`current_phys_footprint`] returns `None` (the Mach
//! queries don't exist); callers degrade gracefully.

/// The cheap read: the current process's `phys_footprint` in bytes, or `None`
/// if the query failed or the platform has no Mach `task_info`.
pub fn current_phys_footprint() -> Option<u64> {
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
pub struct TaskVmInfoResult {
    /// The metric macOS keys memory pressure and jetsam on, and what Activity
    /// Monitor's "Memory" column shows.
    pub phys_footprint: u64,
    /// The high-water mark of `phys_footprint`, when the kernel reports it.
    pub phys_footprint_peak: Option<u64>,
    /// Resident set size. Counts graphics and shared mappings that aren't real
    /// memory pressure, so prefer `phys_footprint`.
    pub resident_size: u64,
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
pub fn query_task_vm_info() -> Option<TaskVmInfoResult> {
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

// ── `mach_task_basic_info` (RSS) ─────────────────────────────────────

/// The prefix of `mach_task_basic_info` we read.
#[cfg(target_os = "macos")]
pub struct BasicInfo {
    /// Resident set size right now.
    pub resident_size: u64,
    /// The high-water mark of `resident_size` over the process's life.
    pub resident_size_max: u64,
}

/// Query `mach_task_basic_info` for resident size and its high-water mark.
///
/// Uses raw FFI because the `libc` crate doesn't expose `MACH_TASK_BASIC_INFO`.
#[cfg(target_os = "macos")]
pub fn query_basic_info() -> Option<BasicInfo> {
    // Mach task info flavor (from <mach/task_info.h>).
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
    // kernel writes only within `info`. We read `info` only when `result == 0`.
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
            Some(BasicInfo {
                resident_size: info.resident_size,
                resident_size_max: info.resident_size_max,
            })
        } else {
            log::debug!("process_memory: task_info(BASIC) failed with code {result}");
            None
        }
    }
}

// ── mimalloc: OUR allocator ──────────────────────────────────────────

/// What mimalloc accounts for. mimalloc is Cmdr's global allocator, so this
/// covers essentially every Rust allocation in the process, indexing included.
#[cfg(target_os = "macos")]
pub struct MimallocHeap {
    /// Bytes mimalloc has committed from the OS: live allocations plus its own
    /// free lists and arena slack. mimalloc exposes no cheap process-wide
    /// "bytes in use", so committed is the number that tracks the Rust heap.
    pub committed: u64,
    /// High-water mark of `committed` over the process lifetime.
    pub peak_committed: u64,
}

/// Ask mimalloc how much it has committed. This is the only way to see the Rust
/// heap: the macOS zone APIs are blind to it (see the module docs).
#[cfg(target_os = "macos")]
pub fn query_mimalloc_heap() -> MimallocHeap {
    let mut current_commit: usize = 0;
    let mut peak_commit: usize = 0;

    // SAFETY: every `mi_process_info` parameter is an independent, nullable out-pointer.
    // We pass null for the six fields we don't read and pointers to two initialized locals
    // for the two we do; mimalloc only writes through non-null ones and reads none. It is
    // documented thread-safe and needs no initialization beyond the allocator already being
    // in use as our global allocator.
    unsafe {
        libmimalloc_sys::mi_process_info(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut current_commit,
            &mut peak_commit,
            std::ptr::null_mut(),
        );
    }

    MimallocHeap {
        committed: current_commit as u64,
        peak_committed: peak_commit as u64,
    }
}

// ── System malloc zones: everything EXCEPT our allocator ─────────────

/// `malloc_statistics_t` from `<malloc/malloc.h>`.
// DEFAULT-OK: an all-zero out-param is what `malloc_zone_statistics` expects and fills
// in; nothing reads a field before that call returns.
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct MallocStatistics {
    blocks_in_use: libc::c_uint,
    size_in_use: libc::size_t,
    max_size_in_use: libc::size_t,
    size_allocated: libc::size_t,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    /// With a NULL zone, aggregates statistics across every REGISTERED zone in
    /// the process. mimalloc doesn't register one, so this never sees the Rust heap.
    fn malloc_zone_statistics(zone: *mut libc::c_void, stats: *mut MallocStatistics);
    /// Fills `*addresses` with a pointer to an array of `*count` zone addresses.
    /// A NULL `reader` uses the default in-process reader.
    fn malloc_get_all_zones(
        task: libc::mach_port_t,
        reader: *mut libc::c_void,
        addresses: *mut *mut usize,
        count: *mut libc::c_uint,
    ) -> libc::c_int;
    /// Returns the zone's name (a NUL-terminated string owned by the zone), or NULL.
    fn malloc_get_zone_name(zone: *mut libc::c_void) -> *const libc::c_char;
}

/// What the system malloc zones hold: WebKit, Objective-C, and C-library
/// allocations. Explicitly NOT the Rust heap (see the module docs).
#[cfg(target_os = "macos")]
pub struct SystemMallocZones {
    /// Bytes the zones report as handed out.
    pub in_use: u64,
    /// Bytes the zones hold from the OS, in use or not.
    pub reserved: u64,
    /// How many zones were registered at snapshot time.
    pub zone_count: u32,
    /// The largest zone by in-use bytes: `(name, in_use)`.
    pub largest_zone: Option<(String, u64)>,
}

/// Sum every registered malloc zone, plus the zone count and the largest zone.
#[cfg(target_os = "macos")]
pub fn query_system_malloc_zones() -> SystemMallocZones {
    // SAFETY: a NULL zone pointer asks `malloc_zone_statistics` for the
    // all-zones aggregate (documented behavior); `agg` is a `#[repr(C)]` match
    // of `malloc_statistics_t` and is fully written by the call.
    let agg = unsafe {
        let mut agg = MallocStatistics::default();
        malloc_zone_statistics(std::ptr::null_mut(), &mut agg);
        agg
    };

    let mut zones_total = SystemMallocZones {
        in_use: agg.size_in_use as u64,
        reserved: agg.size_allocated as u64,
        zone_count: 0,
        largest_zone: None,
    };

    let mut addresses: *mut usize = std::ptr::null_mut();
    let mut count: libc::c_uint = 0;
    #[allow(deprecated, reason = "mach_task_self is deprecated in libc but works fine")]
    // SAFETY: `mach_task_self()` is our own task; a NULL `reader` selects the
    // default in-process reader, which sets `addresses` to point at the live
    // zone registry (process-owned; we must NOT free it) and `count` to its
    // length. Both out-pointers are valid locals.
    let kr = unsafe { malloc_get_all_zones(libc::mach_task_self(), std::ptr::null_mut(), &mut addresses, &mut count) };

    if kr != 0 || addresses.is_null() {
        return zones_total;
    }
    zones_total.zone_count = count;

    // SAFETY: on success `malloc_get_all_zones` set `addresses` to a valid array
    // of `count` zone addresses in this process; we read exactly `count` of them
    // and never mutate or free the buffer.
    let zones = unsafe { std::slice::from_raw_parts(addresses, count as usize) };

    let mut largest = 0u64;
    let mut largest_name: Option<String> = None;
    for &addr in zones {
        let zone = addr as *mut libc::c_void;
        if zone.is_null() {
            continue;
        }
        // SAFETY: `zone` is a live zone pointer from `malloc_get_all_zones`;
        // `stats` is a `#[repr(C)]` match of `malloc_statistics_t`, fully written.
        let stats = unsafe {
            let mut stats = MallocStatistics::default();
            malloc_zone_statistics(zone, &mut stats);
            stats
        };
        let in_use = stats.size_in_use as u64;
        if in_use > largest {
            largest = in_use;
            // SAFETY: `zone` is a live zone pointer; `malloc_get_zone_name`
            // returns a NUL-terminated string owned by the zone, or NULL.
            let name_ptr = unsafe { malloc_get_zone_name(zone) };
            largest_name = if name_ptr.is_null() {
                None
            } else {
                // SAFETY: `name_ptr` is non-NULL and points at a NUL-terminated,
                // zone-owned C string that outlives this borrow.
                Some(
                    unsafe { std::ffi::CStr::from_ptr(name_ptr) }
                        .to_string_lossy()
                        .into_owned(),
                )
            };
        }
    }
    if largest > 0 {
        zones_total.largest_zone = Some((largest_name.unwrap_or_else(|| "?".to_string()), largest));
    }

    zones_total
}

// ── The kernel's VM map ──────────────────────────────────────────────

/// The fifth reader, and the only one that spans both allocators: see
/// [`query_vm_regions`].
#[cfg(target_os = "macos")]
mod vm_regions;
#[cfg(target_os = "macos")]
pub use vm_regions::{RegionSizeGroup, TagUsage, VmRegionMap, query_vm_regions};

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn current_phys_footprint_returns_positive_value() {
        let phys = current_phys_footprint();
        assert!(phys.is_some(), "should be able to query phys_footprint");
        assert!(phys.unwrap() > 0, "phys_footprint should be positive");
    }

    #[test]
    fn query_basic_info_returns_positive_resident() {
        let basic = query_basic_info().expect("should be able to query resident memory");
        assert!(basic.resident_size > 0, "resident memory should be positive");
    }

    #[test]
    fn system_malloc_zones_sum_is_positive() {
        let zones = query_system_malloc_zones();
        assert!(zones.in_use > 0, "system malloc zones in-use should be positive");
        assert!(zones.reserved >= zones.in_use, "reserved should be >= in-use");
        assert!(zones.zone_count >= 1, "there should be at least one malloc zone");
    }

    #[test]
    fn mimalloc_reports_a_heap_the_malloc_zones_cannot_see() {
        let heap = query_mimalloc_heap();
        assert!(heap.committed > 0, "mimalloc should report committed bytes");
        assert!(
            heap.peak_committed >= heap.committed,
            "peak should be >= current committed"
        );
    }

    #[test]
    fn a_mimalloc_allocation_is_counted_by_mimalloc_and_invisible_to_the_malloc_zones() {
        // The whole reason `query_mimalloc_heap` exists: the macOS zone APIs
        // cannot see mimalloc, so a watchdog reading only zones under-reports the
        // heap it polices by orders of magnitude.
        //
        // The allocation goes through `mi_malloc` directly rather than through
        // Rust's `Vec`, because `#[global_allocator]` is set in `main.rs` — the
        // shipped binary runs on mimalloc, but this unit-test harness does not.
        const CHUNK: usize = 512 * 1024 * 1024;

        let zones_before = query_system_malloc_zones().in_use;
        let mimalloc_before = query_mimalloc_heap().committed;

        // SAFETY: `mi_malloc` returns an owned block of at least `CHUNK` bytes or
        // null; we check for null, write only within `CHUNK` bytes of it, and hand
        // the same pointer back to `mi_free` exactly once.
        let (zones_after, mimalloc_after) = unsafe {
            let block = libmimalloc_sys::mi_malloc(CHUNK) as *mut u8;
            assert!(!block.is_null(), "mi_malloc should hand back a {CHUNK}-byte block");
            // Touch every page so the bytes are really committed, not just reserved.
            std::ptr::write_bytes(block, 1u8, CHUNK);
            let readings = (query_system_malloc_zones().in_use, query_mimalloc_heap().committed);
            libmimalloc_sys::mi_free(block as *mut core::ffi::c_void);
            readings
        };

        let mimalloc_growth = mimalloc_after.saturating_sub(mimalloc_before);
        assert!(
            mimalloc_growth >= (CHUNK as u64) / 2,
            "mimalloc should account for most of its own {CHUNK}-byte block, saw {mimalloc_growth}"
        );

        let zone_growth = zones_after.saturating_sub(zones_before);
        assert!(
            zone_growth < (CHUNK as u64) / 4,
            "the system malloc zones should stay blind to the mimalloc heap, but grew by {zone_growth}"
        );
    }
}
