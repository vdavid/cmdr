//! Memory-shape guard on the search arena's row.
//!
//! The arena holds one [`SearchEntry`] per file on the volume — ~6 M rows on David's
//! boot disk — and it's resident for as long as someone is searching, so every byte
//! the struct grows costs ~6 MB of peak footprint. That makes the row's SIZE a design
//! constraint rather than an implementation detail, and this file is where it's pinned.
//!
//! Two different failures are covered, because `size_of` alone can't see the second:
//!
//! - The struct growing (an `Option<u64>` re-introduced, a field added).
//! - A field that's small inline but allocates per row (a `String` name instead of an
//!   arena offset costs 24 B of struct and a heap block on top).
//!
//! Sibling guard on the importance weight map: `search/ranking/memory_tests.rs`.

use super::{OptU64, SearchEntry};
use crate::test_support::heap_bytes_held;

/// What one arena row may cost.
///
/// `id` + `parent_id` + `size` + `modified_at` are 8 each; `name_offset` (4),
/// `name_len` (2), and `is_directory` (1) pack into the last 8 with a byte to spare.
/// Both optional values are sentinel-encoded ([`OptU64`]) precisely to hit this
/// number — as plain `Option<u64>` they'd cost 16 apiece and the row would be 56.
const ENTRY_BYTES: usize = 40;

/// Rows in the synthetic arena. Big enough that a per-row heap block would dwarf the
/// fixed overhead, small enough to build in milliseconds.
const ROWS: usize = 100_000;

#[test]
fn a_search_entry_stays_forty_bytes() {
    assert_eq!(
        std::mem::size_of::<SearchEntry>(),
        ENTRY_BYTES,
        "the search arena holds one of these per file (~6 M rows), so every byte here is ~6 MB of \
         peak memory. An `Option<u64>` field costs 16 B for a value that needs 8 — encode it with \
         `OptU64` instead (see its doc comment for why the sentinel can't collide)."
    );
}

/// A row must cost its struct and nothing else: no per-entry heap allocation.
///
/// Filenames live in `SearchIndex.names`, one arena `String` for the whole volume, so
/// the entries `Vec` is the only thing measured here.
#[test]
fn an_arena_row_costs_the_struct_and_nothing_more() {
    let (entries, bytes) = heap_bytes_held(|| {
        let mut entries: Vec<SearchEntry> = Vec::with_capacity(ROWS);
        for i in 0..ROWS {
            entries.push(SearchEntry {
                id: i as i64 + 1,
                parent_id: 1,
                name_offset: 0,
                name_len: 0,
                is_directory: false,
                size: OptU64::new(Some(i as u64)),
                modified_at: OptU64::new(Some(1_700_000_000 + i as u64)),
            });
        }
        entries
    });

    assert_eq!(entries.len(), ROWS);
    // The counting allocator lives in `crate::test_support` and is per test BINARY. If it
    // ever stops being installed here, the budget below reads 0 and passes while checking
    // nothing.
    assert!(
        bytes > 0,
        "the counting allocator isn't installed in this test binary, so the budget below measures nothing"
    );

    // `with_capacity(ROWS)` allocates exactly once, so the only slack is rounding.
    let budget = (ENTRY_BYTES + 1) * ROWS;
    assert!(
        bytes as usize <= budget,
        "the arena holds {bytes} B across {ROWS} rows ({:.1} B each), past the {budget} B budget. \
         A per-row heap allocation (an owned filename, a boxed field) is what this catches.",
        bytes as f64 / ROWS as f64,
    );
}
