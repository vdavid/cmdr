//! One IPC command answering "what is Cmdr holding right now, and what shape is it in?".
//!
//! Three memory investigations reached for `vmmap`, `footprint`, and a
//! `MallocStackLogging=1` relaunch, and two of them still attributed a large block to
//! the wrong subsystem (`docs/notes/idle-memory-profile-2026-07-28.md`,
//! `docs/notes/idle-cpu-attribution-2026-08-03.md`). This command exists so the next one
//! starts from a reading instead of a hypothesis: it works against a RUNNING app,
//! including a shipped release build under a real workload, which is the only condition
//! the interesting numbers appear under.
//!
//! **It is the only reading that spans both allocators.** Cmdr's Rust heap is mimalloc,
//! which is not a registered macOS malloc zone, so `malloc_zone_statistics` is
//! structurally blind to it; and the zone APIs know nothing about the C and Objective-C
//! allocations that dominate `MALLOC_LARGE`. The kernel's VM map sees both, because both
//! take their pages from it. `cmdr_fs::process_memory` owns that argument and the FFI.
//!
//! **It also names the one big block neither allocator explains.** SQLite serves every
//! store's cached database pages from a single process-wide slab, which is a leaked Rust
//! allocation: 64 MiB sitting inside the mimalloc total with nothing pointing at it.
//! `sqlitePageCache` says how big it is, how much of it is really held, and how many read
//! connections are pushing on it, so the answer to "what is Cmdr holding?" doesn't
//! require knowing to go ask SQLite separately (`cmdr_fs::sqlite_util`).
//!
//! **How to read the payload.** Sort by `dirtyBytes` and start at the top. Then look at
//! each big tag's `sizes`: a repeated EXACT region size is a fingerprint, because macOS
//! gives every allocation past its 127 KB large-zone threshold a region sized to the
//! request. Two worked examples:
//!
//! - `101,187,584` bytes means the CLIP text tower is loaded — that is its `49,408 × 512`
//!   fp32 token embedding, and nothing else in the process is that size
//!   (`crates/cmdr-index/src/media_index/clip/DETAILS.md` § "What holding the towers
//!   costs").
//! - Anything under `IOAccelerator` is the Rust heap, ❌ never graphics. mimalloc tags its
//!   arenas with `VM_MEMORY_IOACCELERATOR`, and that single mislabel cost two days across
//!   three agents.
//!
//! Recipes, the traps, and the past investigations: `docs/tooling/memory-debugging.md`.
//!
//! **Privacy**: every field is a byte count, a region count, or a fixed tag name. No
//! paths, no filenames, no user data — nothing here can carry any.

use crate::commands::util::blocking_with_timeout;
use std::time::Duration;

/// How long the walk gets before the command gives up and reports what it has. A Mach
/// syscall loop can't hang on a dead mount, so this is a backstop against a pathological
/// region count, not the usual filesystem deadline.
const WALK_TIMEOUT: Duration = Duration::from_secs(5);

/// The most size groups we'll report per tag. Past this the tail is noise, and the
/// payload stops being readable in one screen.
const MAX_SIZES_PER_TAG: u32 = 24;

// ── DTO mirror types ──────────────────────────────────────────────────
//
// `cmdr-fs` can't carry `specta` derives for these (they'd be the only ones in
// `process_memory`, and the crate has no reason to know about IPC), so they're mirrored
// here the way `smb_diagnostics.rs` mirrors `smb2`'s types.

/// A snapshot of the whole process's memory, from every accountant at once.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDiagnostics {
    /// The honest total: what Activity Monitor's "Memory" column shows and what jetsam
    /// keys on. `0` if the kernel query failed.
    pub phys_footprint_bytes: u64,
    /// The high-water mark of `physFootprintBytes` over the process's life, when the
    /// kernel reports it.
    pub phys_footprint_peak_bytes: Option<u64>,
    /// Resident set size. Counts graphics and shared mappings that aren't real memory
    /// pressure, so prefer the footprint.
    pub resident_bytes: u64,
    /// What mimalloc — our global allocator, so essentially every Rust allocation — has
    /// committed from the OS.
    pub rust_heap_committed_bytes: u64,
    /// The high-water mark of `rustHeapCommittedBytes`.
    pub rust_heap_peak_committed_bytes: u64,
    /// What the registered macOS malloc zones report as handed out: WebKit,
    /// Objective-C, and C-library allocations. ❌ Never the Rust heap.
    pub system_zones_in_use_bytes: u64,
    /// What those zones hold from the OS, in use or not.
    pub system_zones_reserved_bytes: u64,
    /// How many zones were registered at snapshot time.
    pub system_zone_count: u32,
    /// The biggest registered zone by in-use bytes, as `[name, bytes]`.
    pub largest_system_zone: Option<SystemZone>,
    /// SQLite's process-wide page memory, which belongs to no allocator above:
    /// the slab is a leaked Rust allocation, so it's a fixed 64 MiB sitting
    /// INSIDE `rustHeapCommittedBytes` that nothing else here names.
    pub sqlite_page_cache: SqlitePageCache,
    /// The kernel's VM map folded by tag, biggest dirty total first. Empty if the walk
    /// failed or timed out.
    pub tags: Vec<MemoryTag>,
    /// Dirty bytes across every region the walk saw.
    pub total_dirty_bytes: u64,
    /// How many map entries the walk saw.
    pub total_region_count: u32,
    /// True when the walk stopped at its ceiling, so `tags` is a floor rather than a
    /// total. A runaway is exactly when a diagnostic must not quietly under-report.
    pub truncated: bool,
}

/// The biggest registered malloc zone.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SystemZone {
    /// The zone's own name, for example `DefaultMallocZone` or `WebKit Malloc`.
    pub name: String,
    /// Bytes it reports as handed out.
    pub in_use_bytes: u64,
}

/// SQLite's page memory: the one process-wide slab every store's cached database
/// pages come out of, plus the read-connection count that decides whether it can
/// stay a cap.
///
/// `usedBytes` pegged at `slabBytes` with `liveReadConnections` past the budget
/// it was sized for is the treadmill `cmdr_fs::sqlite_util` describes, not a
/// healthy cache.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SqlitePageCache {
    /// The slab's size, or `0` if it failed to install (which the `sqlite` log
    /// target would have warned about at startup).
    pub slab_bytes: u64,
    /// Slab bytes currently holding database pages. The slab is handed to SQLite
    /// zeroed, so this is roughly the part of it that's dirty.
    pub used_bytes: u64,
    /// The high-water mark of `usedBytes`. At `slabBytes` it means the slab ran
    /// full at least once, even if it isn't now.
    pub peak_used_bytes: u64,
    /// Page-cache bytes SQLite took from the heap because the slab couldn't
    /// serve them. Expected to be `0`.
    pub overflow_bytes: u64,
    /// The high-water mark of `overflowBytes`.
    pub peak_overflow_bytes: u64,
    /// Read connections open across every thread. They're thread-local and live
    /// as long as their thread, so this tracks tokio's blocking pool; each one
    /// adds its `cache_size` to SQLite's global ceiling on retained pages.
    pub live_read_connections: u32,
}

/// One VM tag's share of the address space: the rows `vmmap -summary` prints.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryTag {
    /// The raw `user_tag` from the map entry.
    pub tag: u32,
    /// Its `vmmap`-style name, or `tag-<n>` for one we don't carry a name for.
    pub name: String,
    /// Pages this process wrote, so pages it pays for. The column to read.
    pub dirty_bytes: u64,
    /// Dirty pages since compressed or swapped out.
    pub swapped_bytes: u64,
    /// Resident bytes, clean pages (mapped files, shared text) included.
    pub resident_bytes: u64,
    /// Address space reserved, most of which is typically untouched.
    pub virtual_bytes: u64,
    /// How many map entries carry this tag.
    pub region_count: u32,
    /// The tag's distinct region sizes, biggest dirty total first. The fingerprint
    /// field: see the module docs.
    pub sizes: Vec<MemoryRegionSize>,
}

/// One distinct region size under a tag.
#[derive(Debug, Clone, Copy, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRegionSize {
    /// The size every region in this group has, in bytes.
    pub region_bytes: u64,
    /// How many regions are exactly this size.
    pub count: u32,
    /// Dirty bytes across the group.
    pub dirty_bytes: u64,
}

// ── The command ───────────────────────────────────────────────────────

/// Snapshot this process's memory: the footprint, both allocators' own accounting,
/// SQLite's page-cache slab, and the kernel's VM map folded by tag with a per-tag
/// region-size histogram.
///
/// `sizesPerTag` caps the histogram (0 asks for tag totals only); it's clamped to
/// [`MAX_SIZES_PER_TAG`]. Runs off the IPC thread because the walk costs one syscall per
/// map entry — single-digit milliseconds for a few thousand regions, but not free.
///
/// Reading the result: module docs. macOS only; the Mach queries behind it don't exist
/// elsewhere.
#[tauri::command]
#[specta::specta]
pub async fn get_memory_diagnostics(sizes_per_tag: u32) -> MemoryDiagnostics {
    let cap = sizes_per_tag.min(MAX_SIZES_PER_TAG) as usize;
    blocking_with_timeout(WALK_TIMEOUT, empty_snapshot(), move || collect(cap)).await
}

/// The zero snapshot the timeout path returns. Callers can tell it apart from a real one:
/// a live process never reports a zero footprint.
fn empty_snapshot() -> MemoryDiagnostics {
    MemoryDiagnostics {
        phys_footprint_bytes: 0,
        phys_footprint_peak_bytes: None,
        resident_bytes: 0,
        rust_heap_committed_bytes: 0,
        rust_heap_peak_committed_bytes: 0,
        system_zones_in_use_bytes: 0,
        system_zones_reserved_bytes: 0,
        system_zone_count: 0,
        largest_system_zone: None,
        sqlite_page_cache: SqlitePageCache::default(),
        tags: Vec::new(),
        total_dirty_bytes: 0,
        total_region_count: 0,
        truncated: false,
    }
}

/// Read every accountant and fold them into one payload.
fn collect(sizes_per_tag: usize) -> MemoryDiagnostics {
    let vm = cmdr_fs::process_memory::query_task_vm_info();
    let heap = cmdr_fs::process_memory::query_mimalloc_heap();
    let zones = cmdr_fs::process_memory::query_system_malloc_zones();
    let regions = cmdr_fs::process_memory::query_vm_regions(sizes_per_tag);
    let page_cache = cmdr_fs::sqlite_util::query_page_cache_usage();

    MemoryDiagnostics {
        phys_footprint_bytes: vm.as_ref().map_or(0, |v| v.phys_footprint),
        phys_footprint_peak_bytes: vm.as_ref().and_then(|v| v.phys_footprint_peak),
        resident_bytes: vm.as_ref().map_or(0, |v| v.resident_size),
        rust_heap_committed_bytes: heap.committed,
        rust_heap_peak_committed_bytes: heap.peak_committed,
        system_zones_in_use_bytes: zones.in_use,
        system_zones_reserved_bytes: zones.reserved,
        system_zone_count: zones.zone_count,
        largest_system_zone: zones
            .largest_zone
            .map(|(name, in_use_bytes)| SystemZone { name, in_use_bytes }),
        sqlite_page_cache: SqlitePageCache {
            slab_bytes: page_cache.slab_bytes,
            used_bytes: page_cache.used_bytes,
            peak_used_bytes: page_cache.peak_used_bytes,
            overflow_bytes: page_cache.overflow_bytes,
            peak_overflow_bytes: page_cache.peak_overflow_bytes,
            live_read_connections: u32::try_from(cmdr_fs::sqlite_util::live_read_connections()).unwrap_or(u32::MAX),
        },
        tags: regions
            .as_ref()
            .map(|map| {
                map.tags
                    .iter()
                    .map(|t| MemoryTag {
                        tag: t.tag,
                        name: t.name.clone(),
                        dirty_bytes: t.dirty_bytes,
                        swapped_bytes: t.swapped_bytes,
                        resident_bytes: t.resident_bytes,
                        virtual_bytes: t.virtual_bytes,
                        region_count: t.region_count,
                        sizes: t
                            .sizes
                            .iter()
                            .map(|s| MemoryRegionSize {
                                region_bytes: s.region_bytes,
                                count: s.count,
                                dirty_bytes: s.dirty_bytes,
                            })
                            .collect(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        total_dirty_bytes: regions.as_ref().map_or(0, |m| m.total_dirty_bytes),
        total_region_count: regions.as_ref().map_or(0, |m| m.total_region_count),
        truncated: regions.as_ref().is_some_and(|m| m.truncated),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_snapshot_reads_every_accountant_at_once() {
        let snapshot = get_memory_diagnostics(8).await;

        assert!(snapshot.phys_footprint_bytes > 0, "a live process has a footprint");
        assert!(snapshot.resident_bytes > 0, "and a resident set");
        assert!(snapshot.system_zone_count >= 1, "and at least one registered zone");
        assert!(!snapshot.tags.is_empty(), "and a walkable VM map");
        assert!(!snapshot.truncated, "nowhere near the region ceiling");
        assert!(
            snapshot.tags.windows(2).all(|w| w[0].dirty_bytes >= w[1].dirty_bytes),
            "tags arrive biggest dirty total first, so a reader starts at the top"
        );
        assert!(
            snapshot.tags.iter().all(|t| t.sizes.len() <= 8),
            "the histogram honors the caller's cap"
        );
    }

    /// The gap this closes: SQLite's page-cache slab is a fixed 64 MiB inside
    /// the mimalloc total that nothing else in the payload names, so a reader
    /// asking "what is Cmdr holding?" used to have to know to check SQLite
    /// separately. One call now answers the whole question.
    #[tokio::test]
    async fn the_snapshot_names_sqlites_page_cache_too() {
        let snapshot = get_memory_diagnostics(4).await;
        let sqlite = &snapshot.sqlite_page_cache;

        let budget = cmdr_fs::sqlite_util::SHARED_PAGE_CACHE_BYTES as u64;
        assert!(
            sqlite.slab_bytes > 0 && budget - sqlite.slab_bytes < 8 * 1024,
            "the slab fills its budget bar the slot remainder, got {}",
            sqlite.slab_bytes
        );
        assert!(
            sqlite.used_bytes <= sqlite.slab_bytes,
            "what the slab holds is a share of it, never more"
        );
        assert_eq!(
            sqlite.overflow_bytes, 0,
            "page memory outside the budget would mean the slab stopped describing it"
        );
    }

    #[tokio::test]
    async fn an_absurd_histogram_cap_is_clamped_rather_than_honored() {
        // A diagnostic surface takes its argument from whoever is debugging, which
        // includes a typo. The payload has to stay readable either way.
        let snapshot = get_memory_diagnostics(u32::MAX).await;
        assert!(
            snapshot
                .tags
                .iter()
                .all(|t| t.sizes.len() <= MAX_SIZES_PER_TAG as usize),
            "sizes per tag are capped at {MAX_SIZES_PER_TAG}"
        );
    }

    #[tokio::test]
    async fn a_zero_cap_asks_for_tag_totals_only() {
        let snapshot = get_memory_diagnostics(0).await;
        assert!(snapshot.total_dirty_bytes > 0, "the totals are still collected");
        assert!(
            snapshot.tags.iter().all(|t| t.sizes.is_empty()),
            "and no histogram is built"
        );
    }
}
