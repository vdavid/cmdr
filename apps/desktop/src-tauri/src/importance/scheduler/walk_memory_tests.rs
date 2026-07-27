//! Memory-shape guards on the recompute walk.
//!
//! The walk runs over multi-million-row NAS indexes, where what it keeps PER FOLDER is
//! the whole cost: a full entry row plus a materialized path plus a per-folder
//! extension set is what made a real 391,563-folder pass cost 244 MB. These tests pin
//! the SHAPE (a small fixed record per folder, allocations amortised over the walk
//! rather than one per folder or per file) with bounds generous enough to survive
//! allocator and buffer-growth changes, and tight enough that a per-item regression
//! blows straight through them.
//!
//! Sibling guards on the media walk: `media_index/scheduler/enrich_memory_tests.rs`.

use super::walk::walk_index_folders;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::test_support::{count_allocations, heap_bytes_held};

/// Branch folders directly under the home, each holding [`LEAVES_PER_BRANCH`] leaf
/// folders with a few files in them. The product is what the guards scale against.
const BRANCHES: i64 = 100;
const LEAVES_PER_BRANCH: i64 = 50;
/// Files per leaf folder, over a handful of extensions, so the extension fold is
/// exercised without the file count driving the guard.
const FILES_PER_LEAF: usize = 6;
/// Every folder the walk reports: the branches and their leaves, plus the `/Users` and
/// `/Users/test` ancestors. (The root sentinel is a directory but not a folder.)
const FOLDERS: i64 = BRANCHES * (1 + LEAVES_PER_BRANCH) + 2;

/// The most bytes a walked folder may hold: a 24-byte tree record plus its name plus one
/// small per-folder record, with whatever growth slack the two vectors carry (they double,
/// so up to 2× the live bytes). This corpus measures 109 B a folder (2026-07-27); a
/// materialized path per folder or a per-folder extension set puts it straight over.
const BYTES_PER_FOLDER_CEILING: i64 = 150;

/// The ceiling on a WHOLE walk's allocations. The walk grows a name arena, a folder
/// vector, and a few scratch buffers, all by doubling, so its cost is logarithmic in
/// folder count plus a constant for query preparation and the propagation seed. This
/// corpus measures 52 allocations for 5,102 folders and 30,600 files (2026-07-27); one
/// allocation per folder would land above [`FOLDERS`], and one per file an order beyond.
const ALLOCATION_CEILING: u64 = 300;

/// A folder-heavy index with a few files in each leaf: the shape a real home or NAS
/// has, and the one where per-folder cost dominates.
fn build_folder_heavy_index(path: &std::path::Path) -> String {
    let store = IndexStore::open(path).expect("open index");
    let conn = store.read_conn();
    let mut next_id = ROOT_ID + 1;
    let insert = |parent_id: i64, name: &str, id: i64, is_directory: bool| {
        IndexStore::insert_entry_v2_with_id(
            conn,
            id,
            parent_id,
            name,
            is_directory,
            false,
            None,
            None,
            Some(1_000_000_000),
            None,
        )
        .expect("insert entry");
    };

    let users_id = next_id;
    next_id += 1;
    insert(ROOT_ID, "Users", users_id, true);
    let home_id = next_id;
    next_id += 1;
    insert(users_id, "test", home_id, true);

    for branch in 0..BRANCHES {
        let branch_id = next_id;
        next_id += 1;
        insert(home_id, &format!("branch{branch}"), branch_id, true);
        for leaf in 0..LEAVES_PER_BRANCH {
            let leaf_id = next_id;
            next_id += 1;
            insert(branch_id, &format!("leaf{leaf}"), leaf_id, true);
            for file in 0..FILES_PER_LEAF {
                let file_id = next_id;
                next_id += 1;
                // A handful of extensions, some repeated, so the distinct-extension
                // fold has something to deduplicate.
                let extension = ["txt", "jpg", "TXT", "md", "jpg", "rs"][file];
                insert(leaf_id, &format!("file{file}.{extension}"), file_id, false);
            }
        }
    }
    "/Users/test".to_string()
}

#[test]
fn the_walk_holds_a_small_fixed_record_per_folder() {
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    let home = build_folder_heavy_index(&index_path);
    let store = IndexStore::open(&index_path).expect("reopen");

    // Warm the connection's prepared-statement cache first, so the measured walk holds
    // steady-state structures rather than one-off SQL preparation.
    drop(walk_index_folders(store.read_conn(), &home).expect("warm-up walk"));
    let (folders, bytes) = heap_bytes_held(|| walk_index_folders(store.read_conn(), &home).expect("walk"));

    assert_eq!(folders.len() as i64, FOLDERS, "the walk found every folder");
    let per_folder = bytes / FOLDERS;
    drop(folders);
    assert!(
        per_folder < BYTES_PER_FOLDER_CEILING,
        // allowed-pluralize-noun: `FOLDERS` is a compile-time constant in the thousands.
        "the walk is holding {per_folder} B per folder ({bytes} B over {FOLDERS} folders), \
         ceiling {BYTES_PER_FOLDER_CEILING} B — something is resident per folder again"
    );
}

#[test]
fn the_walk_does_not_allocate_per_folder_or_per_file() {
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    let home = build_folder_heavy_index(&index_path);
    let store = IndexStore::open(&index_path).expect("reopen");

    drop(walk_index_folders(store.read_conn(), &home).expect("warm-up walk"));
    let (folders, allocations) = count_allocations(|| walk_index_folders(store.read_conn(), &home).expect("walk"));

    assert_eq!(folders.len() as i64, FOLDERS, "the walk found every folder");
    assert!(
        allocations < ALLOCATION_CEILING,
        "walking a {FOLDERS}-folder index took {allocations} heap allocations, ceiling \
         {ALLOCATION_CEILING} — the walk is allocating per folder or per file again"
    );
}
