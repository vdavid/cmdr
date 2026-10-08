//! The canonical "how much memory is this process using" reader.
//!
//! One place owns the Mach and allocator FFI so both the indexing memory
//! watchdog (machine-protection thresholds) and the log RAM gauge
//! (the app's `logging::ram_gauge`) read the SAME metrics the SAME cheap way.
//! Policy (thresholds, what to do about a number) lives with the caller — the
//! app's `indexing::resources::memory_watchdog`; this module only reads. It also
//! owns WHICH global allocator the app runs on (`allocator.rs`), because every
//! reader below has to know.
//!
//! Six readers, four different accountants:
//!
//! (All but the first are macOS-only, so they're named here rather than linked:
//! an intra-doc link to a `cfg`-gated item is an unresolved link on every other
//! platform, and `cargo doc` is deny-warnings in CI, which builds on Linux.)
//!
//! - [`current_phys_footprint`] / `query_task_vm_info`: the kernel's view.
//! - `query_basic_info`: RSS and its high-water mark.
//! - `query_rust_heap` / `query_rust_heap_snapshot` (`rust_heap.rs`): what the
//!   global allocator holds for the Rust heap, and (snapshot only) how much of
//!   it is live.
//! - `query_system_malloc_zones`: what the malloc zones hold BEYOND the Rust heap.
//! - `query_vm_regions` (`vm_regions.rs`): the kernel's VM map folded by tag,
//!   plus a per-tag histogram of distinct region SIZES.
//!
//! **The middle two never overlap, and which zones they split depends on the
//! allocator.**
//!
//! - **Under mimalloc**, the Rust heap is mimalloc's arenas, and mimalloc is not a
//!   registered macOS malloc zone: `malloc_zone_statistics` and
//!   `malloc_get_all_zones` cannot see a single byte of it, so every zone belongs
//!   to WebKit, Objective-C, and C code. Reading zone totals as "the app's heap"
//!   is how a 16.5 GB `phys_footprint` got reported as a 1.6 GB heap during the
//!   2026-07 runaway.
//! - **Under the system allocator**, the Rust heap is the default zone, shared
//!   with Objective-C and C code, and `query_system_malloc_zones` leaves that
//!   zone out.
//!
//! **`vmmap` gotcha, mimalloc builds only:** mimalloc tags its arena `mmap`s with
//! `os_tag` 100, which macOS defines as `VM_MEMORY_IOACCELERATOR`. So in `vmmap` /
//! `footprint` output the `IOAccelerator` rows ARE the Rust heap, not GPU memory
//! (verified on macOS 15 with `MallocStackLogging=1` + `vmmap -fullStacks`: every
//! 128 MB `IOAccelerator` region backtraces to `mmap` ← `_mi_prim_alloc` ←
//! `mi_arena_reserve`, 2026-07). Don't read those rows as graphics. Under the
//! system allocator the Rust heap is in the `Malloc *` rows instead.
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

mod allocator;
pub use allocator::{GLOBAL_ALLOC, GLOBAL_ALLOCATOR, GlobalAlloc, GlobalAllocator};

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

// ── The Rust heap, from whichever allocator is global ───────────────

#[cfg(target_os = "macos")]
mod rust_heap;
#[cfg(target_os = "macos")]
pub use rust_heap::{
    HeapCensus, LARGEST_BLOCKS, RustHeap, RustHeapSnapshot, query_rust_heap, query_rust_heap_snapshot,
};

#[cfg(all(target_os = "macos", cmdr_mimalloc))]
mod heap_census;

// ── System malloc zones: everything BEYOND the Rust heap ─────────────

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
    /// the process. mimalloc doesn't register one, so under mimalloc this never
    /// sees the Rust heap.
    fn malloc_zone_statistics(zone: *mut libc::c_void, stats: *mut MallocStatistics);
    /// The zone `malloc` serves from. A forwarding wrapper: its address isn't
    /// the registered default zone's, but its statistics are.
    #[cfg(not(cmdr_mimalloc))]
    fn malloc_default_zone() -> *mut libc::c_void;
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

/// What the malloc zones hold BEYOND the Rust heap: WebKit, Objective-C, and
/// C-library allocations. Under mimalloc that's every registered zone; under the
/// system allocator, every zone but the default one, which [`query_rust_heap`]
/// reads. Either way it never overlaps the Rust heap reading (see the module docs).
#[cfg(target_os = "macos")]
pub struct SystemMallocZones {
    /// Bytes the zones report as handed out.
    pub in_use: u64,
    /// Bytes the zones hold from the OS, in use or not.
    pub reserved: u64,
    /// How many zones were counted.
    pub zone_count: u32,
    /// The largest counted zone by in-use bytes: `(name, in_use)`.
    pub largest_zone: Option<(String, u64)>,
}

/// Sum the malloc zones beyond the Rust heap, plus their count and the largest.
#[cfg(target_os = "macos")]
pub fn query_system_malloc_zones() -> SystemMallocZones {
    let mut zones_total = SystemMallocZones {
        in_use: 0,
        reserved: 0,
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

    // SAFETY: on success `malloc_get_all_zones` set `addresses` to a valid array
    // of `count` zone addresses in this process; we read exactly `count` of them
    // and never mutate or free the buffer.
    let zones = unsafe { std::slice::from_raw_parts(addresses, count as usize) };

    // libmalloc keeps the default zone first in its registry. Under the system
    // allocator that zone is the Rust heap, so it's [`query_rust_heap`]'s to
    // report, not ours (a test pins the premise: a `malloc` block never shows up
    // here).
    let skip = usize::from(GLOBAL_ALLOCATOR == GlobalAllocator::System);

    let mut largest = 0u64;
    let mut largest_name: Option<String> = None;
    for &addr in zones.iter().skip(skip) {
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
        zones_total.in_use += in_use;
        zones_total.reserved += stats.size_allocated as u64;
        zones_total.zone_count += 1;
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

/// The default zone's statistics: every `malloc` in the process, so under the
/// system allocator the Rust heap plus Objective-C and C code.
#[cfg(all(target_os = "macos", not(cmdr_mimalloc)))]
fn default_zone_statistics() -> MallocStatistics {
    // SAFETY: `malloc_default_zone` takes nothing and returns the process's
    // default zone, which lives as long as the process. `stats` is a
    // `#[repr(C)]` match of `malloc_statistics_t` and is fully written.
    unsafe {
        let mut stats = MallocStatistics::default();
        malloc_zone_statistics(malloc_default_zone(), &mut stats);
        stats
    }
}

// ── The kernel's VM map ──────────────────────────────────────────────

/// The fifth reader, and the only one that spans both allocators: see
/// [`query_vm_regions`].
#[cfg(target_os = "macos")]
mod vm_regions;
#[cfg(target_os = "macos")]
pub use vm_regions::{MALLOC_TAGS, MIMALLOC_ARENA_TAG, RegionSizeGroup, TagUsage, VmRegionMap, query_vm_regions};

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
    fn the_zones_beyond_the_rust_heap_hold_at_least_what_they_hand_out() {
        let zones = query_system_malloc_zones();
        assert!(zones.reserved >= zones.in_use, "reserved should be >= in-use");
    }
}
