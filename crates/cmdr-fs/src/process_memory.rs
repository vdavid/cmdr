//! The canonical "how much memory is this process using" reader.
//!
//! One place owns the Mach and allocator FFI so both the indexing memory
//! watchdog (machine-protection thresholds) and the log RAM gauge
//! (the app's `logging::ram_gauge`) read the SAME metrics the SAME cheap way.
//! Policy (thresholds, what to do about a number) lives with the caller — the
//! app's `indexing::resources::memory_watchdog`; this module only reads.
//!
//! Four readers, three different accountants:
//!
//! (The last four are macOS-only, so they're named here rather than linked:
//! an intra-doc link to a `cfg`-gated item is an unresolved link on every other
//! platform, and `cargo doc` is deny-warnings in CI, which builds on Linux.)
//!
//! - [`current_phys_footprint`] / `query_task_vm_info`: the kernel's view.
//! - `query_basic_info`: RSS and its high-water mark.
//! - `query_mimalloc_heap`: what OUR allocator has committed.
//! - `query_system_malloc_zones`: what the SYSTEM allocator holds.
//!
//! **The last two do not overlap, and neither alone is "the heap".** Cmdr sets
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
//! The zone walk is heavier (it iterates every zone), so it's snapshot-only.
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

// ── The kernel's VM map: the ONE reader that spans both allocators ────

/// One distinct region size under a tag: how many regions are exactly this big,
/// and what they hold between them.
///
/// This is the field that names an anonymous block. A tag total says "643 MB of
/// `MALLOC_LARGE`"; the size groups say "71 regions of 9,437,184 bytes and 143
/// of 2,359,296", and a repeated exact size is a fingerprint of whatever sizes
/// it — a fixed buffer, a matrix, an arena step.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionSizeGroup {
    /// The size every region in this group has, in bytes.
    pub region_bytes: u64,
    /// How many regions are exactly this size.
    pub count: u32,
    /// Dirty bytes across the group.
    pub dirty_bytes: u64,
}

/// One VM tag's share of the process's address space.
///
/// The tag is what `vmmap` prints in its left column, so these rows compare
/// directly against a `vmmap -summary` of the same process — and, unlike the
/// malloc-zone APIs, this view sees mimalloc's arenas too (under the
/// `IOAccelerator` name; see the module docs).
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagUsage {
    /// The raw `user_tag` from the map entry.
    pub tag: u32,
    /// The tag's `vmmap`-style name, or `tag-<n>` when we don't carry one.
    pub name: String,
    /// Dirty bytes: pages this process wrote, so pages it pays for. The column
    /// to read.
    pub dirty_bytes: u64,
    /// Dirty pages since compressed or swapped out.
    pub swapped_bytes: u64,
    /// Resident bytes, clean pages (mapped files, shared text) included.
    pub resident_bytes: u64,
    /// Address space reserved, most of which is typically untouched.
    pub virtual_bytes: u64,
    /// How many map entries carry this tag.
    pub region_count: u32,
    /// The tag's distinct region sizes, biggest dirty total first, capped by the
    /// caller's `sizes_per_tag`.
    pub sizes: Vec<RegionSizeGroup>,
}

/// The process's whole VM map, folded by tag.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmRegionMap {
    /// Every tag present, biggest dirty total first.
    pub tags: Vec<TagUsage>,
    /// Dirty bytes across every region.
    pub total_dirty_bytes: u64,
    /// How many map entries the walk saw.
    pub total_region_count: u32,
    /// Set when the walk hit [`MAX_REGIONS`] and stopped early, so the totals are
    /// a floor rather than a total. A runaway is exactly when a diagnostic must
    /// not quietly under-report.
    pub truncated: bool,
}

/// `vmmap`'s names for the tags a Cmdr process actually maps. Anything outside
/// this table renders as `tag-<n>`, which is still a usable fingerprint.
///
/// `100` is the one to know: mimalloc tags its arenas with it and macOS calls it
/// `IOAccelerator`, so those rows are the Rust heap (module docs). The name here
/// carries that inline, because whoever meets it in a diagnostic payload has no
/// module docs in front of them.
#[cfg(target_os = "macos")]
const TAG_NAMES: &[(u32, &str)] = &[
    (0, "unnamed"),
    (1, "MALLOC"),
    (2, "MALLOC_SMALL"),
    (3, "MALLOC_LARGE"),
    (4, "MALLOC_HUGE"),
    (6, "REALLOC"),
    (7, "MALLOC_TINY"),
    (8, "MALLOC_LARGE_REUSABLE"),
    (9, "MALLOC_LARGE_REUSED"),
    (11, "MALLOC_NANO"),
    (12, "MALLOC_MEDIUM"),
    (13, "MALLOC_PROB_GUARD"),
    (20, "MACH_MSG"),
    (21, "IOKit"),
    (22, "VM_RECLAIM"),
    (30, "Stack"),
    (31, "Guard"),
    (32, "Shared pmap"),
    (33, "__DATA/dylib"),
    (34, "ObjC dispatchers"),
    (35, "Unshared pmap"),
    (36, "libchannel"),
    (40, "AppKit"),
    (41, "Foundation"),
    (42, "CoreGraphics"),
    (43, "CoreServices"),
    (45, "CoreData"),
    (50, "ATS (font support)"),
    (51, "CoreAnimation"),
    (52, "CG image"),
    (54, "CoreGraphics data"),
    (55, "CoreGraphics shared"),
    (56, "CoreGraphics framebuffers"),
    (57, "CoreGraphics backing stores"),
    (60, "dyld"),
    (61, "dyld malloc"),
    (62, "SQLite page cache"),
    (63, "JavaScriptCore heap"),
    (64, "JS JIT executable"),
    (65, "JS JIT register file"),
    (68, "CoreImage"),
    (70, "ImageIO"),
    (73, "OS Alloc Once"),
    (74, "libdispatch"),
    (75, "Accelerate framework"),
    (76, "CoreUI"),
    (77, "CoreUI image file"),
    (82, "Swift runtime"),
    (83, "Swift metadata"),
    (88, "IOSurface"),
    (89, "libnetwork"),
    (90, "Audio"),
    (97, "QuickLook thumbnails"),
    (100, "IOAccelerator (= our Rust heap: mimalloc arenas)"),
    (103, "CoreUI cached image data"),
    (104, "ColorSync"),
    (107, "Compositor Services"),
];

/// The walk's iteration ceiling. A busy Cmdr maps a few thousand regions, so this
/// only trips on something pathological, and then it sets
/// [`VmRegionMap::truncated`] rather than spinning.
#[cfg(target_os = "macos")]
const MAX_REGIONS: u32 = 200_000;

/// `vm_region_submap_info_64` from `<mach/vm_region.h>`, through its v2 fields.
///
/// The `count` we hand `mach_vm_region_recurse` is this struct's size in
/// `natural_t` (u32) words, which is how that ABI measures it; the kernel fills
/// the newest version the count covers and reports back how much it wrote. We
/// read only v0 fields, which every version has.
///
/// ⚠️ **`packed(4)` is load-bearing, not decoration.** The whole of
/// `<mach/vm_region.h>` sits inside `#pragma pack(push, 4)`, so `offset`'s `u64`
/// starts at byte 12 and every field after it is 4 bytes earlier than natural
/// alignment would put it. A plain `#[repr(C)]` reads `user_tag` from
/// `pages_resident`'s bytes and the walk comes back as plausible-looking
/// nonsense: tags above 255, region counts an order of magnitude short, and a
/// fresh 9 MiB allocation nowhere in the map (verified on macOS 26.5, 2026-08-21).
#[cfg(target_os = "macos")]
#[repr(C, packed(4))]
struct VmRegionSubmapInfo64 {
    protection: i32,
    max_protection: i32,
    inheritance: u32,
    offset: u64,
    user_tag: u32,
    pages_resident: u32,
    pages_shared_now_private: u32,
    pages_swapped_out: u32,
    pages_dirtied: u32,
    ref_count: u32,
    shadow_depth: u16,
    external_pager: u8,
    share_mode: u8,
    is_submap: i32,
    behavior: i32,
    object_id: u32,
    user_wired_count: u16,
    flags: u16,
    pages_reusable: u32,
    object_id_full: u64,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    /// Walk one entry of the task's VM map at or below `nesting_depth`, starting
    /// at `address`. Returns `KERN_INVALID_ADDRESS` past the last entry, which is
    /// what ends the walk.
    fn mach_vm_region_recurse(
        target_task: libc::mach_port_t,
        address: *mut u64,
        size: *mut u64,
        nesting_depth: *mut u32,
        info: *mut VmRegionSubmapInfo64,
        info_count: *mut u32,
    ) -> libc::kern_return_t;
}

/// Walk this process's VM map and fold it by tag: an in-process `vmmap -summary`,
/// plus a per-tag histogram of distinct region sizes.
///
/// **This is the reader that sees BOTH allocators.** [`query_mimalloc_heap`] and
/// [`query_system_malloc_zones`] each see exactly one, and neither can say what
/// SHAPE the bytes are in. The kernel's map can, because every allocator
/// ultimately takes its pages from it, which is what makes a repeated exact
/// region size the cheapest available fingerprint of an unattributed block.
///
/// `sizes_per_tag` caps the histogram per tag (biggest dirty totals win); 0 asks
/// for tag totals only.
///
/// **Snapshot-only, never per-tick.** It costs one `mach_vm_region_recurse`
/// syscall per map entry: low single-digit milliseconds for a few thousand
/// regions, against the single `task_info` call the cheap readers make.
///
/// `None` if the very first query fails.
#[cfg(target_os = "macos")]
pub fn query_vm_regions(sizes_per_tag: usize) -> Option<VmRegionMap> {
    // SAFETY: `_SC_PAGESIZE` is a valid `sysconf` name and the call has no side effects.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page_size = if page_size > 0 { page_size as u64 } else { 4096 };

    let mut by_tag: std::collections::HashMap<u32, TagAccumulator> = std::collections::HashMap::new();
    let mut address: u64 = 0;
    let mut depth: u32 = 0;
    let mut seen: u32 = 0;

    while seen < MAX_REGIONS {
        let mut size: u64 = 0;
        let mut count = (size_of::<VmRegionSubmapInfo64>() / size_of::<u32>()) as u32;
        #[allow(deprecated, reason = "mach_task_self is deprecated in libc but works fine")]
        // SAFETY: every pointer is to an initialized local. `info` is a `#[repr(C)]` match of
        // `vm_region_submap_info_64` and `count` is its size in `natural_t` words, so the kernel
        // writes only within it (it fills the newest version that count covers). `address`,
        // `size`, and `depth` are the in/out cursor the recurse ABI defines. We read `info` only
        // after the call returned `KERN_SUCCESS`.
        let (kr, info) = unsafe {
            let mut info: VmRegionSubmapInfo64 = std::mem::zeroed();
            let kr = mach_vm_region_recurse(
                libc::mach_task_self(),
                &mut address,
                &mut size,
                &mut depth,
                &mut info,
                &mut count,
            );
            (kr, info)
        };
        // Anything but success means the walk ran off the end of the map.
        if kr != 0 {
            break;
        }
        if info.is_submap != 0 {
            // A submap (the shared region): descend into it rather than past it,
            // WITHOUT advancing the address.
            depth += 1;
            continue;
        }

        seen += 1;
        let dirty = u64::from(info.pages_dirtied) * page_size;
        let entry = by_tag.entry(info.user_tag).or_default();
        entry.dirty += dirty;
        entry.swapped += u64::from(info.pages_swapped_out) * page_size;
        entry.resident += u64::from(info.pages_resident) * page_size;
        entry.virtual_bytes += size;
        entry.region_count += 1;
        if sizes_per_tag > 0 {
            let group = entry.sizes.entry(size).or_insert((0, 0));
            group.0 += 1;
            group.1 += dirty;
        }
        // Advance past this region. A zero-sized entry would otherwise stall the walk.
        address = address.saturating_add(size.max(page_size));
    }
    if seen == 0 {
        return None;
    }

    let mut tags: Vec<TagUsage> = by_tag
        .into_iter()
        .map(|(tag, acc)| {
            let mut sizes: Vec<RegionSizeGroup> = acc
                .sizes
                .into_iter()
                .map(|(region_bytes, (count, dirty_bytes))| RegionSizeGroup {
                    region_bytes,
                    count,
                    dirty_bytes,
                })
                .collect();
            sizes.sort_by(|a, b| {
                b.dirty_bytes
                    .cmp(&a.dirty_bytes)
                    .then(b.region_bytes.cmp(&a.region_bytes))
            });
            sizes.truncate(sizes_per_tag);
            TagUsage {
                tag,
                name: tag_name(tag),
                dirty_bytes: acc.dirty,
                swapped_bytes: acc.swapped,
                resident_bytes: acc.resident,
                virtual_bytes: acc.virtual_bytes,
                region_count: acc.region_count,
                sizes,
            }
        })
        .collect();
    tags.sort_by(|a, b| b.dirty_bytes.cmp(&a.dirty_bytes).then(a.tag.cmp(&b.tag)));

    Some(VmRegionMap {
        total_dirty_bytes: tags.iter().map(|t| t.dirty_bytes).sum(),
        total_region_count: seen,
        truncated: seen >= MAX_REGIONS,
        tags,
    })
}

/// Per-tag fold state while the walk runs. `sizes` maps an exact region size to
/// `(count, dirty bytes)`.
// DEFAULT-OK: every field starts at zero, which is what a freshly-seen tag means.
#[cfg(target_os = "macos")]
#[derive(Default)]
struct TagAccumulator {
    dirty: u64,
    swapped: u64,
    resident: u64,
    virtual_bytes: u64,
    region_count: u32,
    sizes: std::collections::HashMap<u64, (u32, u64)>,
}

/// A tag's `vmmap`-style name, or `tag-<n>` for one we don't carry.
#[cfg(target_os = "macos")]
fn tag_name(tag: u32) -> String {
    TAG_NAMES
        .iter()
        .find(|(t, _)| *t == tag)
        .map_or_else(|| format!("tag-{tag}"), |(_, name)| (*name).to_string())
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

    /// The `MALLOC_LARGE` user tag, from `<mach/vm_statistics.h>`.
    const TAG_MALLOC_LARGE: u32 = 3;
    /// The `IOAccelerator` user tag, which mimalloc claims for its arenas.
    const TAG_IOACCELERATOR: u32 = 100;

    fn tag(map: &VmRegionMap, tag: u32) -> Option<&TagUsage> {
        map.tags.iter().find(|t| t.tag == tag)
    }

    #[test]
    fn the_region_walk_covers_the_whole_address_space() {
        let map = query_vm_regions(8).expect("the VM map should be walkable in-process");
        assert!(map.total_region_count > 10, "a live process maps more than 10 regions");
        assert!(!map.truncated, "a test process is nowhere near the region ceiling");
        assert!(map.total_dirty_bytes > 0, "a live process has dirty pages");
        assert!(
            map.tags.windows(2).all(|w| w[0].dirty_bytes >= w[1].dirty_bytes),
            "tags come back biggest dirty total first"
        );
    }

    #[test]
    fn sizes_per_tag_zero_asks_for_tag_totals_only() {
        let map = query_vm_regions(0).expect("the VM map should be walkable in-process");
        assert!(
            map.tags.iter().all(|t| t.sizes.is_empty()),
            "a zero histogram cap yields no size groups"
        );
        assert!(map.total_dirty_bytes > 0, "the totals are still collected");
    }

    #[test]
    fn a_big_system_zone_block_becomes_a_malloc_large_region_of_exactly_its_size() {
        // The mechanism the 2026-08 memory attribution leans on: macOS routes any
        // allocation past its 127 KB large-zone threshold to its own VM region, sized
        // to the request. So a repeated exact region size in `MALLOC_LARGE` is the
        // fingerprint of whatever asked for that many bytes — which is how a block
        // no allocator API can name still gets named. ❌ Break this and the region
        // histogram stops being evidence.
        //
        // 9 MiB is not arbitrary: it's the size the CLIP image tower's Core ML weight
        // buffers land on (`docs/notes/idle-malloc-large-clip-towers-2026-08-21.md`).
        const BLOCK: usize = 9 * 1024 * 1024;

        let before = query_vm_regions(64).expect("walkable");
        let before_dirty = tag(&before, TAG_MALLOC_LARGE).map_or(0, |t| t.dirty_bytes);

        // SAFETY: `malloc` returns an owned block of at least `BLOCK` bytes or null; we
        // null-check, write only within `BLOCK` bytes of it, and `free` it exactly once.
        // It goes through the SYSTEM allocator on purpose: mimalloc is the global
        // allocator in the shipped binary but not in this test harness, and this test is
        // about the system zone either way.
        let (after, block) = unsafe {
            let block = libc::malloc(BLOCK).cast::<u8>();
            assert!(!block.is_null(), "malloc should hand back a {BLOCK}-byte block");
            std::ptr::write_bytes(block, 1u8, BLOCK);
            (query_vm_regions(64).expect("walkable"), block)
        };
        // SAFETY: `block` came from `malloc` above and is freed exactly once, after the
        // measurement that needed it alive.
        unsafe { libc::free(block.cast()) };

        let large = tag(&after, TAG_MALLOC_LARGE).expect("a 9 MiB block puts something in MALLOC_LARGE");
        assert!(
            large.dirty_bytes >= before_dirty + (BLOCK as u64) / 2,
            "MALLOC_LARGE should grow by most of the block: {before_dirty} -> {}",
            large.dirty_bytes
        );
        assert!(
            large.sizes.iter().any(|s| s.region_bytes == BLOCK as u64),
            "the block should show as a region of exactly {BLOCK} bytes, saw {:?}",
            large.sizes.iter().map(|s| s.region_bytes).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_rust_heap_shows_up_under_the_ioaccelerator_tag() {
        // The trap the module docs open with, asserted rather than only written down:
        // mimalloc tags its arenas with `VM_MEMORY_IOACCELERATOR`, so a Rust-heap
        // runaway reads as graphics memory in `vmmap` and in this walk alike.
        const CHUNK: usize = 256 * 1024 * 1024;

        let before = query_vm_regions(0).expect("walkable");
        let before_dirty = tag(&before, TAG_IOACCELERATOR).map_or(0, |t| t.dirty_bytes);

        // SAFETY: `mi_malloc` returns an owned block of at least `CHUNK` bytes or null; we
        // null-check, write only within `CHUNK` bytes of it, and `mi_free` it exactly once.
        let (after, block) = unsafe {
            let block = libmimalloc_sys::mi_malloc(CHUNK).cast::<u8>();
            assert!(!block.is_null(), "mi_malloc should hand back a {CHUNK}-byte block");
            std::ptr::write_bytes(block, 1u8, CHUNK);
            (query_vm_regions(0).expect("walkable"), block)
        };
        // SAFETY: `block` came from `mi_malloc` above and is freed exactly once.
        unsafe { libmimalloc_sys::mi_free(block.cast()) };

        let accel = tag(&after, TAG_IOACCELERATOR).expect("mimalloc arenas carry the IOAccelerator tag");
        assert!(
            accel.dirty_bytes >= before_dirty + (CHUNK as u64) / 2,
            "the mimalloc block should land under IOAccelerator: {before_dirty} -> {}",
            accel.dirty_bytes
        );
        assert!(
            accel.name.contains("Rust heap"),
            "the tag name has to carry the trap: {}",
            accel.name
        );
    }
}
