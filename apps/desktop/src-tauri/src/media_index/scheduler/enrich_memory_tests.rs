//! Memory-shape guards on the whole-index walk.
//!
//! The walk runs over multi-million-row NAS indexes, where "one small allocation per row"
//! is the difference between a flat process and a gigabyte one; two production runaways
//! were made of exactly that (`docs/notes/memory-runaway-rust-heap-2026-07-25.md`). These
//! tests pin the SHAPE (allocations amortised over the whole walk, never proportional to
//! folder count) with bounds generous enough to survive allocator and buffer-growth
//! changes, and tight enough that a per-row regression blows straight through them.

use super::enrich::for_each_qualifying_image;
use crate::indexing::store::{DirTree, IndexStore, ROOT_ID};
use crate::indexing::test_support::{count_allocations, heap_bytes_held};

/// Branch folders directly under the root, each holding [`LEAVES_PER_BRANCH`] childless
/// leaf folders. The product is what the guard scales against.
const BRANCHES: i64 = 100;
const LEAVES_PER_BRANCH: i64 = 20;
/// Every directory the index ends up holding: the branches and their leaves, the one folder
/// with an image in it, and the root sentinel the store creates on open.
const FOLDERS: i64 = BRANCHES * (1 + LEAVES_PER_BRANCH) + 2;

/// The ceiling on a whole walk's allocations. The compact walk grows a name arena, a row
/// vector, and a per-group buffer, all by doubling, so its cost is logarithmic in folder
/// count plus a constant for query preparation. Materializing a row per folder would land
/// above [`FOLDERS`], several times past this.
const ALLOCATION_CEILING: u64 = 500;

/// A folder-heavy, file-light index: [`FOLDERS`] directories of which exactly ONE holds a
/// file. The walk therefore emits a single group, so an allocation count over it measures
/// the per-FOLDER cost with nothing per-image or per-group mixed in.
fn build_folder_heavy_index(path: &std::path::Path) {
    let store = IndexStore::open(path).expect("open index");
    let conn = store.read_conn();
    let mut next_id = ROOT_ID + 1;
    let add_dir = |parent_id: i64, name: &str, id: i64| {
        IndexStore::insert_entry_v2_with_id(conn, id, parent_id, name, true, false, None, None, None, None)
            .expect("insert dir");
    };

    for branch in 0..BRANCHES {
        let branch_id = next_id;
        next_id += 1;
        add_dir(ROOT_ID, &format!("branch{branch}"), branch_id);
        for leaf in 0..LEAVES_PER_BRANCH {
            let leaf_id = next_id;
            next_id += 1;
            add_dir(branch_id, &format!("leaf{leaf}"), leaf_id);
        }
    }

    let photos_id = next_id;
    next_id += 1;
    add_dir(ROOT_ID, "photos", photos_id);
    IndexStore::insert_entry_v2_with_id(
        conn,
        next_id,
        photos_id,
        "beach.jpg",
        false,
        false,
        Some(10),
        Some(10),
        Some(1),
        None,
    )
    .expect("insert file");
}

#[test]
fn walking_the_index_does_not_allocate_per_folder() {
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_folder_heavy_index(&index_path);
    let store = IndexStore::open(&index_path).expect("reopen");

    // Warm the connection's prepared-statement cache first, so the measured walk counts
    // steady-state work rather than one-off SQL preparation.
    for_each_qualifying_image(store.read_conn(), &mut |_| {}).expect("warm-up walk");
    let (qualifying, allocations) = count_allocations(|| {
        let mut seen = 0usize;
        for_each_qualifying_image(store.read_conn(), &mut |_| seen += 1).expect("walk");
        seen
    });

    assert_eq!(qualifying, 1, "the corpus qualifies exactly one image");
    assert!(
        allocations < ALLOCATION_CEILING,
        "the folder side is allocating per folder again: walking a {FOLDERS}-folder index took \
         {allocations} heap allocations, ceiling {ALLOCATION_CEILING}"
    );
}

#[test]
fn the_folder_tree_costs_a_fraction_of_the_full_row_shape() {
    // The walk's floor is the folder side, so pin it against the obvious alternative:
    // reading the same folders as full `EntryRow`s and indexing them by id, which is what
    // `importance`'s recompute still does. The compact tree must stay several times
    // smaller, or the shape has quietly regressed back.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_folder_heavy_index(&index_path);
    let store = IndexStore::open(&index_path).expect("reopen");

    let (compact, compact_bytes) = heap_bytes_held(|| DirTree::load(store.read_conn()).expect("load tree"));
    let (full_rows, full_row_bytes) = heap_bytes_held(|| {
        let rows = IndexStore::all_directories(store.read_conn()).expect("all directories");
        let by_id: std::collections::HashMap<i64, usize> = rows.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        (rows, by_id)
    });

    assert_eq!(full_rows.0.len() as i64, FOLDERS, "both shapes cover the same folders");
    drop(compact);
    drop(full_rows);
    assert!(
        compact_bytes * 3 < full_row_bytes,
        "across a {FOLDERS}-folder index the compact tree holds {compact_bytes} B against \
         {full_row_bytes} B for the full-row shape, which is no longer a meaningful saving"
    );
}
