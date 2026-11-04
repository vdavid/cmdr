//! Memory-shape guard on the resident score table.
//!
//! The table lives as long as media indexing is on, one per scored volume, and it's big:
//! 179,949 scored folders on David's boot volume held 31 MiB as path `String`s plus their
//! hash table (heap attribution, 2026-09-23), and a NAS scores 368,043. The consumers only
//! LOOK UP a folder, never enumerate paths, so the paths are dead weight once hashed.
//!
//! This pins the SHAPE (a fixed small slot per folder, nothing per folder on the heap) with
//! a bound generous enough to survive allocator and load-factor changes, and tight enough
//! that storing the path again blows straight through it. The search weight map carries
//! the same guard (`crates/cmdr-search/src/ranking/memory_tests.rs`).

use super::table_from;
use crate::indexing::test_support::heap_bytes_held;

/// Scored folders in the corpus, just under a hashbrown table boundary (262,144 slots at
/// 76 % load), so the guard measures a realistic load factor rather than a fresh doubling.
const FOLDERS: usize = 200_000;

/// The most bytes a scored folder may cost: its `(u64, f64)` slot plus control byte, at
/// this corpus's load factor ~22 B. A stored path puts it around 150 B.
const BYTES_PER_FOLDER_CEILING: i64 = 32;

#[test]
fn the_resident_table_holds_only_a_small_slot_per_scored_folder() {
    let paths: Vec<String> = (0..FOLDERS)
        .map(|i| {
            format!(
                "/Users/test/projects-git/organization-{}/repository-name-{}/crates/subsystem-{}/src/feature/module-{}",
                i % 97,
                i % 331,
                i % 17,
                i
            )
        })
        .collect();

    // The source strings are cloned and consumed INSIDE the measurement, so what the
    // number reports is what the built table still holds.
    let (table, bytes) = heap_bytes_held(|| table_from(paths.iter().map(|p| (p.clone(), 0.5))));

    assert_eq!(table.len(), FOLDERS);
    assert!(
        bytes > 0,
        "the counting allocator isn't installed, so the budget measures nothing"
    );
    let budget = BYTES_PER_FOLDER_CEILING * FOLDERS as i64;
    assert!(
        bytes <= budget,
        "the score table holds {bytes} B across the corpus ({} B a folder), over the {BYTES_PER_FOLDER_CEILING} B slot budget",
        bytes / FOLDERS as i64
    );
}
