//! Memory-shape guard on the importance weight map.
//!
//! Root's map is permanently resident (the recompute subscriber keeps it fresh), so
//! what it keeps PER SCORED FOLDER is pure steady-state cost on every user with an
//! indexed home. Real volumes are big: David's home stores 158,457 scored folders and
//! his NAS 368,043 (measured 2026-07-27), with absolute paths averaging 113 bytes. A
//! map that stores those paths costs ~160 B a folder; the hashed-key map costs the
//! table slot alone.
//!
//! This pins the SHAPE (a fixed small slot per folder, nothing per-folder on the heap)
//! with a bound generous enough to survive allocator and load-factor changes, and tight
//! enough that storing the path again blows straight through it.
//!
//! Sibling guards on the importance walk: `importance/scheduler/walk_memory_tests.rs`.

use std::collections::HashMap;

use super::ImportanceWeights;
use crate::test_support::heap_bytes_held;

/// Scored folders in the corpus. Sized just under a hashbrown table boundary (a
/// 262,144-slot table at 76 % load), so the guard measures a realistic load factor
/// rather than a table that just doubled.
const FOLDERS: usize = 200_000;

/// The most bytes a scored folder may cost the resident map: its hash-table slot and
/// nothing else. At this corpus's load factor a `(u64, f64)` entry plus its control byte
/// measures 22 B a folder (2026-07-27); storing the folder's path instead puts it around
/// 150 B, and one more `u64` per entry lands over at 33 B.
const BYTES_PER_FOLDER_CEILING: i64 = 32;

/// Absolute folder paths shaped like a real home: a few fixed ancestors, then nested
/// project/leaf segments, averaging ~113 bytes to match the measured real-world mean.
fn corpus_paths() -> Vec<String> {
    (0..FOLDERS)
        .map(|i| {
            format!(
                "/Users/test/projects-git/organization-{}/repository-name-{}/crates/subsystem-{}/src/feature/module-{}",
                i % 97,
                i % 331,
                i % 17,
                i
            )
        })
        .collect()
}

/// The resident weight map must cost a fixed small slot per scored folder — no
/// per-folder heap allocation, and above all not the folder's path.
///
/// The measured closure builds the intermediate `path → weight` map too, so the source
/// strings it allocates are freed inside the measurement and net out: what the number
/// reports is what the built [`ImportanceWeights`] STILL HOLDS.
#[test]
fn the_weight_map_holds_only_a_small_slot_per_scored_folder() {
    let paths = corpus_paths();
    let mean_path_len = paths.iter().map(String::len).sum::<usize>() / paths.len();
    assert!(
        (100..=130).contains(&mean_path_len),
        "corpus should model real absolute paths (~113 B mean), got {mean_path_len} B"
    );

    let (weights, bytes) = heap_bytes_held(|| {
        let map: HashMap<String, f64> = paths.iter().map(|p| (p.clone(), 0.5)).collect();
        ImportanceWeights::from_map(map)
    });

    assert!(!weights.is_empty(), "the corpus should produce a non-empty map");
    // The counting allocator lives in `crate::test_support` and is per test BINARY, so an
    // index-crate copy can't serve this one. If it ever stops being installed here, every
    // measurement below reads 0 and the budget passes while checking nothing.
    assert!(
        bytes > 0,
        "the counting allocator isn't installed in this test binary, so the budget below measures nothing"
    );
    // Compared as a total, not a per-folder quotient: integer division rounds a
    // just-over-budget shape back down onto the ceiling and hides it.
    let budget = BYTES_PER_FOLDER_CEILING * FOLDERS as i64;
    assert!(
        bytes <= budget,
        "the weight map holds {bytes} B across the {FOLDERS}-folder corpus ({:.1} B each), past the \
         {BYTES_PER_FOLDER_CEILING} B-a-folder budget of {budget} B. Storing the path per folder \
         is what this catches.",
        bytes as f64 / FOLDERS as f64,
    );
}
