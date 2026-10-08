//! The Rust heap, read from whichever allocator is global ([`super::GLOBAL_ALLOCATOR`]).
//!
//! The two allocators answer different questions, so the readings are enums whose variant
//! names the allocator, and a caller can't read one allocator's numbers as the other's:
//!
//! - **mimalloc** knows what it has COMMITTED, but has no cheap live-bytes total, so the
//!   snapshot adds a page census (`heap_census.rs`) read against its arenas' VM tag.
//! - **The system allocator** keeps exact live bytes per zone. The Rust heap is the
//!   default zone, which it shares with Objective-C and C code: `malloc` serves them all,
//!   and nothing in the zone tells a Rust block from theirs. There's no census to take.

use super::GlobalAllocator;

/// The cheap reading: what the global allocator holds for the Rust heap. One call, no walk,
/// so the memory watchdog can take it on a threshold trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustHeap {
    /// mimalloc's own accounting.
    Mimalloc {
        /// Bytes committed from the OS: live allocations plus mimalloc's free lists and
        /// arena slack. mimalloc exposes no cheap live total.
        committed: u64,
        /// The high-water mark of `committed`.
        peak_committed: u64,
    },
    /// The default malloc zone: the Rust heap together with Objective-C and C code.
    System {
        /// Bytes in live blocks.
        in_use: u64,
        /// Bytes the zone holds from the OS, in use or free. The zone keeps no high-water
        /// mark (macOS reports `max_size_in_use` as 0).
        reserved: u64,
    },
}

impl RustHeap {
    /// The allocator this reading came from.
    pub fn allocator(&self) -> GlobalAllocator {
        match self {
            RustHeap::Mimalloc { .. } => GlobalAllocator::Mimalloc,
            RustHeap::System { .. } => GlobalAllocator::System,
        }
    }

    /// What the allocator holds from the OS for the heap: the figure to weigh against the
    /// footprint. mimalloc's committed bytes, or the default zone's reserved bytes.
    pub fn held_bytes(&self) -> u64 {
        match *self {
            RustHeap::Mimalloc { committed, .. } => committed,
            RustHeap::System { reserved, .. } => reserved,
        }
    }
}

/// Read the Rust heap from the global allocator. The only way to see it under mimalloc: the
/// macOS zone APIs are blind to mimalloc (the module docs of `process_memory`).
pub fn query_rust_heap() -> RustHeap {
    #[cfg(cmdr_mimalloc)]
    {
        let (committed, peak_committed) = mimalloc_commit();
        RustHeap::Mimalloc {
            committed,
            peak_committed,
        }
    }
    #[cfg(not(cmdr_mimalloc))]
    {
        let stats = super::default_zone_statistics();
        RustHeap::System {
            in_use: stats.size_in_use as u64,
            reserved: stats.size_allocated as u64,
        }
    }
}

/// The snapshot-grade reading: the cheap one plus how much of it is live data and how much
/// the allocator keeps beyond that. Costs a heap walk under mimalloc, so snapshot-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustHeapSnapshot {
    /// mimalloc, with a page census.
    Mimalloc {
        /// Bytes committed from the OS.
        committed: u64,
        /// The high-water mark of `committed`.
        peak_committed: u64,
        /// What the heap's pages hold.
        census: HeapCensus,
        /// The heap's resident size: dirty plus swapped bytes under mimalloc's VM tag
        /// ([`super::MIMALLOC_ARENA_TAG`]). `0` without a VM map.
        resident: u64,
    },
    /// The system allocator, which counts its live bytes exactly.
    System {
        /// The default zone's live bytes: the Rust heap plus Objective-C and C code.
        in_use: u64,
        /// What the default zone holds from the OS, in use or free.
        reserved: u64,
        /// Live bytes across EVERY registered zone, the default one included. The VM tags
        /// can't say which zone a page belongs to, so this is what `resident` is
        /// weighed against.
        all_zones_in_use: u64,
        /// Dirty plus swapped bytes under every malloc VM tag
        /// ([`super::MALLOC_TAGS`]), across every zone. `0` without a VM map.
        resident: u64,
    },
}

/// Take the snapshot-grade reading. `regions` supplies the resident size; pass the map the
/// caller already walked, so one snapshot never walks it twice.
pub fn query_rust_heap_snapshot(regions: Option<&super::VmRegionMap>) -> RustHeapSnapshot {
    let resident_under = |wanted: &[u32]| -> u64 {
        regions.map_or(0, |map| {
            map.tags
                .iter()
                .filter(|t| wanted.contains(&t.tag))
                .map(|t| t.dirty_bytes + t.swapped_bytes)
                .sum()
        })
    };
    #[cfg(cmdr_mimalloc)]
    {
        let (committed, peak_committed) = mimalloc_commit();
        RustHeapSnapshot::Mimalloc {
            committed,
            peak_committed,
            census: super::heap_census::query_heap_census(),
            resident: resident_under(&[super::MIMALLOC_ARENA_TAG]),
        }
    }
    #[cfg(not(cmdr_mimalloc))]
    {
        let stats = super::default_zone_statistics();
        let in_use = stats.size_in_use as u64;
        RustHeapSnapshot::System {
            in_use,
            reserved: stats.size_allocated as u64,
            all_zones_in_use: in_use + super::query_system_malloc_zones().in_use,
            resident: resident_under(super::MALLOC_TAGS),
        }
    }
}

/// What mimalloc's pages hold: the census's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapCensus {
    /// Bytes in allocated blocks: the Rust program's live data (leaning high; see
    /// `heap_census.rs`).
    pub live_bytes: u64,
    /// Bytes of block space the pages have set up, live or free. `block_space_bytes -
    /// live_bytes` is free blocks inside pages that are in use; heap resident minus
    /// `block_space_bytes` is slack outside any page's blocks.
    pub block_space_bytes: u64,
    /// How many pages the census visited.
    pub page_count: u64,
    /// The biggest single live allocations of at least 1 MiB, biggest first, zero-padded.
    /// A repeated exact size is a fingerprint, like a VM region size.
    pub largest_blocks: [u64; LARGEST_BLOCKS],
    /// True when the walk saw every page; false when it stopped at its page ceiling.
    pub complete: bool,
}

/// How many of the largest live blocks the census keeps.
pub const LARGEST_BLOCKS: usize = 8;

/// Ask mimalloc how much it has committed: `(current, peak)`.
#[cfg(cmdr_mimalloc)]
fn mimalloc_commit() -> (u64, u64) {
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

    (current_commit as u64, peak_commit as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reading_comes_from_the_global_allocator() {
        assert_eq!(query_rust_heap().allocator(), super::super::GLOBAL_ALLOCATOR);
        let snapshot = query_rust_heap_snapshot(None);
        let snapshot_allocator = match snapshot {
            RustHeapSnapshot::Mimalloc { .. } => GlobalAllocator::Mimalloc,
            RustHeapSnapshot::System { .. } => GlobalAllocator::System,
        };
        assert_eq!(snapshot_allocator, super::super::GLOBAL_ALLOCATOR);
    }

    /// Under the system allocator the Rust heap IS the default zone, so a block `malloc`
    /// hands out lands in this reading and nowhere in the other zones' total: the two stay
    /// disjoint, which the watchdog's `untracked` remainder relies on.
    #[cfg(not(cmdr_mimalloc))]
    #[test]
    fn a_malloc_block_lands_in_the_rust_heap_and_not_in_the_other_zones() {
        const CHUNK: usize = 64 * 1024 * 1024;

        let heap_before = query_rust_heap();
        let others_before = super::super::query_system_malloc_zones().in_use;
        // SAFETY: `malloc` returns an owned block of at least `CHUNK` bytes or null; we check
        // for null, write only inside it, and `free` the same pointer exactly once.
        let (heap_during, others_during) = unsafe {
            let block = libc::malloc(CHUNK).cast::<u8>();
            assert!(!block.is_null(), "malloc should hand back a {CHUNK}-byte block");
            std::ptr::write_bytes(block, 1u8, CHUNK);
            let readings = (query_rust_heap(), super::super::query_system_malloc_zones().in_use);
            libc::free(block.cast());
            readings
        };

        let (
            RustHeap::System { in_use: before, .. },
            RustHeap::System {
                in_use: during,
                reserved,
            },
        ) = (heap_before, heap_during)
        else {
            panic!("a system-allocator build reads the default zone");
        };
        assert!(
            during.saturating_sub(before) >= CHUNK as u64,
            "the default zone counts the block as live: {before} -> {during}"
        );
        assert!(reserved >= during, "and holds at least what's live");
        assert!(
            others_during.saturating_sub(others_before) < (CHUNK as u64) / 4,
            "the other zones stay clear of it: {others_before} -> {others_during}"
        );
    }

    #[cfg(cmdr_mimalloc)]
    #[test]
    fn mimalloc_reports_a_heap_the_malloc_zones_cannot_see() {
        let RustHeap::Mimalloc {
            committed,
            peak_committed,
        } = query_rust_heap()
        else {
            panic!("a mimalloc build reads mimalloc");
        };
        assert!(committed > 0, "mimalloc should report committed bytes");
        assert!(peak_committed >= committed, "peak should be >= current committed");
    }

    #[cfg(cmdr_mimalloc)]
    #[test]
    fn a_mimalloc_allocation_is_counted_by_mimalloc_and_invisible_to_the_malloc_zones() {
        // The whole reason the mimalloc reader exists: the macOS zone APIs cannot see
        // mimalloc, so a watchdog reading only zones under-reports the heap it polices by
        // orders of magnitude.
        //
        // The allocation goes through `mi_malloc` directly rather than through Rust's `Vec`:
        // the shipped binary installs mimalloc in `main.rs`, but this unit-test harness runs
        // on `System`.
        const CHUNK: usize = 512 * 1024 * 1024;

        let zones_before = super::super::query_system_malloc_zones().in_use;
        let mimalloc_before = query_rust_heap().held_bytes();

        // SAFETY: `mi_malloc` returns an owned block of at least `CHUNK` bytes or null; we
        // check for null, write only within `CHUNK` bytes of it, and hand the same pointer
        // back to `mi_free` exactly once.
        let (zones_after, mimalloc_after) = unsafe {
            let block = libmimalloc_sys::mi_malloc(CHUNK) as *mut u8;
            assert!(!block.is_null(), "mi_malloc should hand back a {CHUNK}-byte block");
            // Touch every page so the bytes are really committed, not just reserved.
            std::ptr::write_bytes(block, 1u8, CHUNK);
            let readings = (
                super::super::query_system_malloc_zones().in_use,
                query_rust_heap().held_bytes(),
            );
            libmimalloc_sys::mi_free(block.cast());
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
