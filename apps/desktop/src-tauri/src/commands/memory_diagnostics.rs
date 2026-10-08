//! One IPC command answering "what is Cmdr holding right now, and what shape is it in?".
//!
//! Three memory investigations reached for `vmmap`, `footprint`, and a
//! `MallocStackLogging=1` relaunch, and two of them still attributed a large block to
//! the wrong subsystem (`docs/notes/performance/idle-memory-profile-2026-07-28.md`,
//! `docs/notes/performance/idle-cpu-attribution-2026-08-03.md`). This command exists so the next one
//! starts from a reading instead of a hypothesis: it works against a RUNNING app,
//! including a shipped release build under a real workload, which is the only condition
//! the interesting numbers appear under.
//!
//! **It names the allocator it read.** macOS builds run on the system allocator and
//! Linux ones (or macOS with the `mimalloc` feature) on mimalloc, and the two account for
//! the Rust heap so differently that a number from one means nothing in the other's terms.
//! So `rustHeap` is tagged by `allocator`, and each variant carries only what that
//! allocator can honestly say (`cmdr_fs::process_memory::GLOBAL_ALLOCATOR`).
//!
//! **It is the only reading that spans every allocator in the process.** Under mimalloc
//! the Rust heap is outside every registered malloc zone, so `malloc_zone_statistics` is
//! structurally blind to it; under the system allocator it shares the default zone with
//! Objective-C and C code. The kernel's VM map sees all of it, because everything takes
//! its pages from there. `cmdr_fs::process_memory` owns that argument and the FFI.
//!
//! **It also names the one big block neither allocator explains.** SQLite serves every
//! store's cached database pages from a single process-wide slab, which is a leaked Rust
//! allocation: 64 MiB sitting inside the Rust heap total with nothing pointing at it.
//! `sqlitePageCache` says how big it is, how much of it is really held, and how many read
//! connections are pushing on it, so the answer to "what is Cmdr holding?" doesn't
//! require knowing to go ask SQLite separately (`cmdr_fs::sqlite_util`).
//!
//! **And it splits the heap into data and slack.** Under mimalloc, the committed total
//! can't say how much of it the program is using, and mimalloc's own stats can't either
//! (they merge per-thread counts late, so they read hundreds of MiB "live" after a free):
//! `rustHeap.census` walks every mimalloc page for the live bytes and reads the heap's
//! resident size off the VM map (`cmdr_fs::process_memory`'s `heap_census.rs`). The
//! system allocator counts live bytes exactly, but per zone, and the Rust heap's zone is
//! shared: there's no Rust-only census, so its variant weighs every zone's live bytes
//! against every malloc VM tag's resident bytes instead.
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
//! - In a mimalloc build, anything under `IOAccelerator` is the Rust heap, ❌ never
//!   graphics. mimalloc tags its arenas with `VM_MEMORY_IOACCELERATOR`, and that single
//!   mislabel cost two days across three agents. In a system-allocator build the Rust heap
//!   is in the `MALLOC_*` tags instead.
//! - `IOSurface` (tag 88) is ❌ NOT in `physFootprintBytes`. Those regions are WebKit's
//!   layer backing stores, mapped here from the GPU process and charged to WebContent, so
//!   their dirty bytes can exceed the whole footprint
//!   (`docs/notes/performance/main-process-iosurface-2026-09-27.md`).
//!
//! Recipes, the traps, and the past investigations: `docs/tooling/memory-debugging.md`.
//!
//! **Privacy**: every field is a byte count, a region count, or a fixed tag name. No
//! paths, no filenames, no user data — nothing here can carry any.

use crate::deadline::blocking_with_timeout;
use cmdr_fs::process_memory::{GlobalAllocator, RustHeapSnapshot};
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
    /// The Rust heap, tagged by the global allocator that holds it (`allocator`). Read its
    /// numbers only in that allocator's terms.
    pub rust_heap: RustHeapDiagnostics,
    /// What the malloc zones BEYOND the Rust heap report as handed out: WebKit,
    /// Objective-C, and C-library allocations. Under mimalloc that's every registered
    /// zone; under the system allocator every zone but the default one, which is
    /// `rustHeap`'s. ❌ Never overlaps `rustHeap`.
    pub system_zones_in_use_bytes: u64,
    /// What those zones hold from the OS, in use or not.
    pub system_zones_reserved_bytes: u64,
    /// How many zones those two fields count.
    pub system_zone_count: u32,
    /// The biggest of those zones by in-use bytes.
    pub largest_system_zone: Option<SystemZone>,
    /// SQLite's process-wide page memory, which no allocator reading names: the slab
    /// is a leaked Rust allocation, so it's a fixed 64 MiB sitting INSIDE `rustHeap`.
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

/// The Rust heap, as its global allocator accounts for it. `allocator` says which one.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(tag = "allocator", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RustHeapDiagnostics {
    /// mimalloc: its own committed total, plus a census of its pages.
    Mimalloc {
        /// What mimalloc has committed from the OS: live data plus its slack.
        committed_bytes: u64,
        /// The high-water mark of `committedBytes`.
        peak_committed_bytes: u64,
        /// How much of the heap is live data, and how much is allocator slack. The one
        /// field that can tell "the program holds this" from "mimalloc holds this".
        census: RustHeapCensus,
    },
    /// The system allocator: the default malloc zone, which the Rust heap shares with
    /// Objective-C and C code. No page census exists for it: nothing in the zone tells a
    /// Rust block from theirs, so the live/slack split below spans every zone.
    System {
        /// Bytes in live blocks in the default zone: the Rust heap plus Objective-C and C.
        in_use_bytes: u64,
        /// What the default zone holds from the OS, in use or free. The zone keeps no
        /// high-water mark.
        reserved_bytes: u64,
        /// Dirty plus swapped bytes under every `MALLOC_*` VM tag, across every zone: what
        /// malloc costs resident. `0` when the VM walk failed.
        malloc_resident_bytes: u64,
        /// `mallocResidentBytes` minus every zone's live bytes, floored at zero: what malloc
        /// holds beyond live data, in every zone together.
        malloc_slack_bytes: u64,
    },
}

/// The mimalloc heap split into live data and allocator slack.
///
/// `liveBytes` is what the program holds; `residentBytes` is what the heap costs (its VM
/// tag's dirty plus swapped bytes). The gap, `slackBytes`, is memory mimalloc keeps that
/// no live allocation uses: free blocks inside pages (`blockSpaceBytes - liveBytes`) plus
/// retained memory outside any page's blocks (`residentBytes - blockSpaceBytes`). What the
/// census can't see, and why `liveBytes` leans high: `cmdr_fs::process_memory` §
/// `heap_census`.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RustHeapCensus {
    /// Bytes in allocated blocks across every mimalloc page.
    pub live_bytes: u64,
    /// Bytes of block space those pages have set up, live or free.
    pub block_space_bytes: u64,
    /// The heap's resident size: dirty plus swapped bytes under mimalloc's VM tag
    /// (`IOAccelerator`, tag 100). `0` when the VM walk failed.
    pub resident_bytes: u64,
    /// `residentBytes - liveBytes`, floored at zero: what the allocator holds beyond the
    /// program's data.
    pub slack_bytes: u64,
    /// How many pages the census visited.
    pub page_count: u64,
    /// The biggest live allocations of 1 MiB or more, biggest first. A repeated exact
    /// size is a fingerprint (the SQLite page slab is one 64 MiB block).
    pub largest_live_blocks: Vec<u64>,
    /// False when the census stopped at its page ceiling, so the totals are a floor.
    pub complete: bool,
}

/// One VM tag's share of the address space: the rows `vmmap -summary` prints.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryTag {
    /// The raw `user_tag` from the map entry.
    pub tag: u32,
    /// Its `vmmap`-style name, or `tag-<n>` for one we don't carry a name for.
    pub name: String,
    /// Dirty pages in this process's mappings, and the column to read. Private memory is
    /// what the process pays for; a shared object's pages (`IOSurface`) count in every
    /// process that maps them and are paid by the one that owns them.
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

/// Snapshot this process's memory: the footprint, the allocators' own accounting,
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
        rust_heap: empty_rust_heap(),
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

/// The empty snapshot's heap: zeros, still tagged with the allocator this build runs on,
/// so even a timed-out reading can't be read in the wrong allocator's terms.
fn empty_rust_heap() -> RustHeapDiagnostics {
    match cmdr_fs::process_memory::GLOBAL_ALLOCATOR {
        GlobalAllocator::Mimalloc => RustHeapDiagnostics::Mimalloc {
            committed_bytes: 0,
            peak_committed_bytes: 0,
            census: RustHeapCensus {
                live_bytes: 0,
                block_space_bytes: 0,
                resident_bytes: 0,
                slack_bytes: 0,
                page_count: 0,
                largest_live_blocks: Vec::new(),
                complete: false,
            },
        },
        GlobalAllocator::System => RustHeapDiagnostics::System {
            in_use_bytes: 0,
            reserved_bytes: 0,
            malloc_resident_bytes: 0,
            malloc_slack_bytes: 0,
        },
    }
}

/// Mirror `cmdr-fs`'s snapshot-grade heap reading into the payload.
fn rust_heap_diagnostics(snapshot: RustHeapSnapshot) -> RustHeapDiagnostics {
    match snapshot {
        RustHeapSnapshot::Mimalloc {
            committed,
            peak_committed,
            census,
            resident,
        } => RustHeapDiagnostics::Mimalloc {
            committed_bytes: committed,
            peak_committed_bytes: peak_committed,
            census: RustHeapCensus {
                live_bytes: census.live_bytes,
                block_space_bytes: census.block_space_bytes,
                resident_bytes: resident,
                slack_bytes: resident.saturating_sub(census.live_bytes),
                page_count: census.page_count,
                largest_live_blocks: census.largest_blocks.into_iter().filter(|&size| size > 0).collect(),
                complete: census.complete,
            },
        },
        RustHeapSnapshot::System {
            in_use,
            reserved,
            all_zones_in_use,
            resident,
        } => RustHeapDiagnostics::System {
            in_use_bytes: in_use,
            reserved_bytes: reserved,
            malloc_resident_bytes: resident,
            malloc_slack_bytes: resident.saturating_sub(all_zones_in_use),
        },
    }
}

/// Read every accountant and fold them into one payload.
fn collect(sizes_per_tag: usize) -> MemoryDiagnostics {
    let vm = cmdr_fs::process_memory::query_task_vm_info();
    let zones = cmdr_fs::process_memory::query_system_malloc_zones();
    let regions = cmdr_fs::process_memory::query_vm_regions(sizes_per_tag);
    let page_cache = cmdr_fs::sqlite_util::query_page_cache_usage();
    let rust_heap = cmdr_fs::process_memory::query_rust_heap_snapshot(regions.as_ref());

    MemoryDiagnostics {
        phys_footprint_bytes: vm.as_ref().map_or(0, |v| v.phys_footprint),
        phys_footprint_peak_bytes: vm.as_ref().and_then(|v| v.phys_footprint_peak),
        resident_bytes: vm.as_ref().map_or(0, |v| v.resident_size),
        rust_heap: rust_heap_diagnostics(rust_heap),
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

    unsafe extern "C" {
        fn malloc_create_zone(start_size: libc::size_t, flags: libc::c_uint) -> *mut libc::c_void;
    }

    #[tokio::test]
    async fn the_snapshot_reads_every_accountant_at_once() {
        // Which zones exist beyond the default one is up to the OS: on macOS 27 the Objective-C
        // runtime registers `objc-class_rw_t`, while the macOS 26 CI runner had none (verified
        // with `malloc_get_all_zones` locally and a CI run, 2026-10-06). Under the system
        // allocator the default zone is the Rust heap's and isn't counted here, so a bare
        // process can legitimately report zero. Registering our own makes "at least one"
        // hold on every macOS. Leaked on purpose: destroying a zone while a parallel test
        // walks the registry would hand that walk a dangling pointer.
        // SAFETY: `malloc_create_zone` takes plain integers and returns a new zone (or NULL),
        // which we never use or free.
        let zone = unsafe { malloc_create_zone(0, 0) };
        assert!(!zone.is_null(), "libmalloc registers the test's own zone");

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

    /// The two allocators' numbers mean different things, so a reading that doesn't say
    /// which allocator it came from gets misread. The tag is the first thing in `rustHeap`.
    #[tokio::test]
    async fn the_snapshot_names_the_allocator_the_app_runs_on() {
        let expected = serde_json::to_value(cmdr_fs::process_memory::GLOBAL_ALLOCATOR).expect("serializable");

        let snapshot = serde_json::to_value(get_memory_diagnostics(0).await).expect("serializable");
        assert_eq!(snapshot["rustHeap"]["allocator"], expected, "a real reading names it");

        let empty = serde_json::to_value(empty_snapshot()).expect("serializable");
        assert_eq!(
            empty["rustHeap"]["allocator"], expected,
            "and so does the timed-out one, so its zeros can't be read in the wrong terms"
        );
    }

    /// The question a committed or reserved total can't answer: of what the Rust heap holds,
    /// how much is live data? Each allocator answers it in its own terms.
    #[tokio::test]
    async fn the_snapshot_splits_the_rust_heap_into_live_data_and_slack() {
        // Through the global allocator, which in this test binary is `System` whatever the
        // build's choice: it lands in the default zone, and only the system variant sees it.
        const BLOCK: usize = 24 * 1024 * 1024;
        let block = vec![1u8; BLOCK];

        let heap = get_memory_diagnostics(0).await.rust_heap;
        drop(block);

        match heap {
            RustHeapDiagnostics::System {
                in_use_bytes,
                reserved_bytes,
                malloc_resident_bytes,
                malloc_slack_bytes,
            } => {
                assert!(in_use_bytes >= BLOCK as u64, "the block is live in the default zone");
                assert!(reserved_bytes >= in_use_bytes, "which holds at least what's live");
                assert!(
                    malloc_resident_bytes >= BLOCK as u64,
                    "and it's resident under the malloc tags"
                );
                assert!(
                    malloc_slack_bytes <= malloc_resident_bytes,
                    "slack is a share of resident"
                );
            }
            RustHeapDiagnostics::Mimalloc { census, .. } => {
                assert!(census.complete, "a test heap is nowhere near the page ceiling");
                assert!(
                    census.block_space_bytes >= census.live_bytes,
                    "live is inside block space"
                );
                assert_eq!(
                    census.slack_bytes,
                    census.resident_bytes.saturating_sub(census.live_bytes)
                );
            }
        }
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
