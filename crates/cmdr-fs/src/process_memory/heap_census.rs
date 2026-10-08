//! A census of mimalloc's pages: how many of the bytes it holds are live. Built only where
//! mimalloc is the global allocator (`cmdr_mimalloc`).
//!
//! `query_rust_heap` says what mimalloc has COMMITTED, and the VM map says what's
//! resident under its tag, but neither can split that into "the program's data" and "the
//! allocator's slack". This can: it walks every page of mimalloc's heap and sums the
//! blocks in use against the block space the page has set up.
//!
//! **Why a census and not mimalloc's own stats.** `mi_stats_*` with `MI_STAT` built in
//! keeps per-thread counters that merge into the heap only when a thread collects or
//! exits, and a free on another thread decrements THAT thread's counter. After a 400 MiB
//! search arena was freed the stats still read 242–265 MiB "live", so they can't answer
//! this (`docs/tooling/memory-debugging.md` § "Live bytes vs allocator slack").
//!
//! **Process-wide, because mimalloc v3's heap is.** v3 splits the v2 heap in two: a
//! `mi_heap_t` owns pages across every thread, and each thread allocates through its own
//! thread-local `theap` onto it. The Rust global allocator uses the main heap and Cmdr
//! creates no other, so visiting the main heap (a null heap) covers every thread's pages
//! (`libmimalloc-sys` 0.1.49, `c_src/mimalloc/v3/src/arena.c` `_mi_heap_visit_blocks`).
//!
//! **What it can't see:**
//!
//! - mimalloc's own metadata (page maps, arena bitmaps, per-thread `theap`s). Small, and
//!   outside every page.
//! - A page mimalloc took straight from the OS rather than an arena: only an allocation
//!   past `arena_max_object_size` (2 GiB) or with a huge alignment. The walk visits arena
//!   pages only; OS-direct ones would be visited once abandoned, and only with the
//!   `visit_abandoned` option, which our build leaves off.
//! - Blocks another thread has freed that the owning thread hasn't collected yet. A page's
//!   `used` counts them as live until then, so `live_bytes` leans HIGH, never low.
//!
//! **Safe in a live app, and bounded.** It walks without claiming pages from their owning
//! threads (the same way the probe in the 2026-09-23 heap attribution did), so each page's
//! counts are a racy snapshot: a page changing mid-walk is counted either before or after.
//! Arena memory stays mapped for the life of the process, so reading a page header that
//! another thread is recycling can't fault; it only reads a stale count. The visitor
//! allocates nothing (it runs inside the allocator's own walk), and it stops after
//! [`MAX_PAGES`], reporting `complete: false`.

use std::ffi::c_void;

use libmimalloc_sys::{mi_heap_area_t, mi_heap_t, mi_heap_visit_blocks};

use super::{HeapCensus, LARGEST_BLOCKS};

/// The smallest block the census names individually. Below this a block is one of many in
/// a page and says nothing on its own.
const LARGE_BLOCK_MIN: u64 = 1024 * 1024;

/// Where the walk stops. A page is at least 64 KiB, so this is 64 GiB of heap: past any
/// heap a working Cmdr holds, and a few milliseconds of walking.
const MAX_PAGES: u64 = 1_000_000;

/// Count mimalloc's pages: live bytes against block space, and the biggest live blocks.
/// Costs one visit per page (single-digit milliseconds for a few hundred MiB of heap), so
/// it's snapshot-only, like the VM-map walk.
pub(super) fn query_heap_census() -> HeapCensus {
    let mut census = HeapCensus {
        live_bytes: 0,
        block_space_bytes: 0,
        page_count: 0,
        largest_blocks: [0; LARGEST_BLOCKS],
        complete: false,
    };
    // SAFETY: a null heap asks mimalloc for its main heap, which the global allocator is
    // using and which lives for the process. `visit_area` matches `mi_block_visit_fun`,
    // reads only the area it's handed, and writes only through `arg`, which points at
    // `census`, a live local that outlives the call and that nothing else aliases while
    // the walk runs. `visit_blocks = false` means mimalloc hands it areas only, never a
    // block pointer.
    let finished = unsafe {
        mi_heap_visit_blocks(
            std::ptr::null_mut(),
            false,
            Some(visit_area),
            (&mut census as *mut HeapCensus).cast::<c_void>(),
        )
    };
    census.complete = finished;
    census
}

/// Fold one page into the census. Returns `false` to stop the walk.
///
/// # Safety
///
/// `area` points at a valid `mi_heap_area_t` for the duration of the call, and `arg` is the
/// `*mut HeapCensus` [`query_heap_census`] passed in.
unsafe extern "C" fn visit_area(
    _heap: *const mi_heap_t,
    area: *const mi_heap_area_t,
    _block: *mut c_void,
    _block_size: usize,
    arg: *mut c_void,
) -> bool {
    // SAFETY: the caller's contract above: `arg` is our exclusive `HeapCensus`, and `area`
    // is valid for this call (mimalloc fills a stack local and passes its address).
    let (census, area) = unsafe { (&mut *arg.cast::<HeapCensus>(), &*area) };
    if census.page_count >= MAX_PAGES {
        return false;
    }
    census.page_count += 1;
    // v3 fills `used` with a BLOCK COUNT (`_mi_heap_area_init`), whatever the binding's
    // doc comment says, and `committed` with the page's block capacity in bytes.
    let block_size = area.block_size as u64;
    let used = area.used as u64;
    census.live_bytes += used * block_size;
    census.block_space_bytes += area.committed as u64;
    if block_size >= LARGE_BLOCK_MIN {
        for _ in 0..used.min(LARGEST_BLOCKS as u64) {
            keep_if_largest(&mut census.largest_blocks, block_size);
        }
    }
    true
}

/// Insert `size` into a descending, zero-padded top list if it beats the smallest entry.
/// Allocation-free on purpose: it runs inside mimalloc's own walk.
fn keep_if_largest(top: &mut [u64; LARGEST_BLOCKS], size: u64) {
    let Some(slot) = top.iter().position(|&kept| size > kept) else {
        return;
    };
    top.copy_within(slot..LARGEST_BLOCKS - 1, slot + 1);
    top[slot] = size;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_list_stays_sorted_and_drops_the_smallest() {
        let mut top = [0u64; LARGEST_BLOCKS];
        for size in [5, 9, 1, 9, 7, 3, 2, 8, 6, 4] {
            keep_if_largest(&mut top, size);
        }
        assert_eq!(top, [9, 9, 8, 7, 6, 5, 4, 3]);
    }

    /// The census sees a block the moment it's allocated and stops seeing it once it's
    /// freed. Allocated through `mi_malloc` directly because this unit-test harness doesn't
    /// run on mimalloc as its global allocator (the shipped binary does).
    #[test]
    fn a_live_block_is_counted_and_a_freed_one_is_not() {
        const BLOCK: usize = 48 * 1024 * 1024;

        let before = query_heap_census();
        // SAFETY: `mi_malloc` returns an owned block of at least `BLOCK` bytes or null; we
        // check for null, write only inside it, and free the same pointer exactly once.
        let (during, after) = unsafe {
            let block = libmimalloc_sys::mi_malloc(BLOCK) as *mut u8;
            assert!(!block.is_null(), "mi_malloc should hand back a {BLOCK}-byte block");
            std::ptr::write_bytes(block, 1u8, BLOCK);
            let during = query_heap_census();
            libmimalloc_sys::mi_free(block as *mut c_void);
            (during, query_heap_census())
        };

        assert!(during.complete, "a census of a small test heap sees every page");
        assert!(
            during.live_bytes >= before.live_bytes + BLOCK as u64,
            "the live total grows by the block: {} -> {}",
            before.live_bytes,
            during.live_bytes
        );
        assert!(
            during.block_space_bytes >= during.live_bytes,
            "live bytes are a share of the block space, never more"
        );
        assert!(
            during.largest_blocks.contains(&(BLOCK as u64)),
            "a block this big is named by its exact size: {:?}",
            during.largest_blocks
        );
        assert!(
            !after.largest_blocks.contains(&(BLOCK as u64)),
            "and it's gone once freed: {:?}",
            after.largest_blocks
        );
    }
}
