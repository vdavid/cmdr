//! Shared helpers and common re-exports for the scheduler test modules. Each
//! `*_tests.rs` file does `use super::*;` (the scheduler's public items) plus
//! `use super::test_support::*;` (these helpers and common re-exports), so the
//! test bodies read the same as when they all lived in one `tests.rs`.

use super::*;

// Re-exports the split test bodies reference by bare name (the scheduler's
// glob doesn't cover the recompute internals or these crate paths).
pub(super) use super::recompute::{
    ANCESTOR_WALK_CAP, RecomputeInputs, RescoreScope, is_in_changed_subtree, recompute_folders, score_folders,
    touched_folder_set,
};
pub(super) use super::walk::WalkedFolders;
pub(super) use crate::ROOT_VOLUME_ID;
pub(super) use crate::importance::signals::OptionalSignals;
pub(super) use crate::importance::store::{ImportanceStore, importance_db_path};
pub(super) use crate::importance::writer::WeightRow;

/// The stop signal for a test with no volume behind it: one that never fires.
/// ❌ Never cancel it; a test about stopping builds its own token.
pub(super) static NEVER_STOPPED: std::sync::LazyLock<CancellationToken> =
    std::sync::LazyLock::new(CancellationToken::new);

/// Full-pass a hand-built walk, returning the writer + store path for a follow-up
/// incremental. Shared by the two transition tests.
pub(super) fn full_pass_walk(dir: &std::path::Path, home: &str, folders: &mut WalkedFolders) -> ImportanceWriter {
    let writer = ImportanceWriter::spawn(&importance_db_path(dir, ROOT_VOLUME_ID)).expect("writer");
    recompute_folders(
        &RecomputeInputs {
            writer: &writer,
            weights: &Weights::default(),
            home,
            now_secs: 1_000_000_000,
            available: SignalSet::listing_only(),
            visits: &HashMap::new(),
            last_used: &HashMap::new(),
            stop: &NEVER_STOPPED,
        },
        folders,
    )
    .expect("full pass");
    writer.flush_blocking().expect("flush");
    writer
}

/// A folder-heavy index with a few files in each leaf: the shape a real home or NAS
/// has, and the one where per-folder cost dominates. `branches` folders sit directly
/// under the home, each holding `leaves_per_branch` leaf folders of `files_per_leaf`
/// files (at most six). Returns the home path.
pub(super) fn build_folder_heavy_index(
    path: &std::path::Path,
    branches: i64,
    leaves_per_branch: i64,
    files_per_leaf: usize,
) -> String {
    use crate::indexing::store::{IndexStore, ROOT_ID};

    let store = IndexStore::open(path).expect("open index");
    let conn = store.read_conn();
    // One transaction for the whole tree: tens of thousands of autocommits cost seconds in
    // a debug build, which pushed the tests on this fixture past the 8 s cap on CI.
    conn.execute_batch("BEGIN").expect("begin fixture txn");
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

    for branch in 0..branches {
        let branch_id = next_id;
        next_id += 1;
        insert(home_id, &format!("branch{branch}"), branch_id, true);
        for leaf in 0..leaves_per_branch {
            let leaf_id = next_id;
            next_id += 1;
            insert(branch_id, &format!("leaf{leaf}"), leaf_id, true);
            for file in 0..files_per_leaf {
                let file_id = next_id;
                next_id += 1;
                // A handful of extensions, some repeated, so the distinct-extension
                // fold has something to deduplicate.
                let extension = ["txt", "jpg", "TXT", "md", "jpg", "rs"][file];
                insert(leaf_id, &format!("file{file}.{extension}"), file_id, false);
            }
        }
    }
    conn.execute_batch("COMMIT").expect("commit fixture txn");
    "/Users/test".to_string()
}

/// Build an index DB for `volume_id` over the canonical synthetic home and route
/// the volume's read pool at it, so a pass has something real to walk. Without a
/// pool, a pass reads nothing and writes nothing, and a test asserting "no pass
/// ran" would pass for the wrong reason. Pair it with `uninstall_read_pool`.
pub(super) fn install_index_for(data_dir: &std::path::Path, volume_id: &str) {
    let index_path = data_dir.join(format!("index-{volume_id}.db"));
    build_index_from_home(
        &index_path,
        &crate::importance::fixtures::SyntheticHome::canonical(1_000_000_000),
    );
    crate::indexing::read::enrichment::install_read_pool(
        volume_id,
        Arc::new(crate::indexing::read::enrichment::ReadPool::new(index_path).expect("read pool")),
    );
}

/// Build an index DB over a `SyntheticHome` using the real `IndexStore` +
/// `IndexWriter`, so the recompute reads exactly the schema production reads. We
/// insert each entry with a parent pointer derived from its path.
pub(super) fn build_index_from_home(index_path: &std::path::Path, home: &crate::importance::fixtures::SyntheticHome) {
    use crate::indexing::store::{IndexStore, ROOT_ID};

    // Open the store (creates the schema), then insert entries parent-first by
    // walking paths in sorted order so a parent always exists before its child.
    let store = IndexStore::open(index_path).expect("open index");
    let conn = store.read_conn();

    // Map from absolute path to assigned entry id; a top-level entry's parent is
    // the sentinel ROOT_ID (`/`).
    let mut path_to_id: HashMap<String, i64> = HashMap::new();
    let mut next_id: i64 = ROOT_ID + 1;

    // Insert a directory entry for `path`, first inserting any missing ancestors
    // so `reconstruct_path` yields the full absolute path (a real index has every
    // ancestor from `/`; the synthetic tree starts mid-way at the home root).
    fn ensure_dir(
        conn: &rusqlite::Connection,
        path: &str,
        modified_at: Option<u64>,
        path_to_id: &mut HashMap<String, i64>,
        next_id: &mut i64,
    ) -> i64 {
        use crate::indexing::store::{IndexStore, ROOT_ID};
        if let Some(&id) = path_to_id.get(path) {
            return id;
        }
        let parent = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string());
        let parent_id = match parent.as_deref() {
            Some("") | Some("/") | None => ROOT_ID,
            Some(pp) => ensure_dir(conn, pp, None, path_to_id, next_id),
        };
        let name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let id = *next_id;
        *next_id += 1;
        IndexStore::insert_entry_v2_with_id(conn, id, parent_id, &name, true, false, None, None, modified_at, None)
            .expect("insert dir");
        path_to_id.insert(path.to_string(), id);
        id
    }

    let mut entries: Vec<_> = home.all_entries().to_vec();
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    for e in &entries {
        if e.is_directory {
            ensure_dir(conn, &e.path, e.modified_at, &mut path_to_id, &mut next_id);
        } else {
            let parent_path = std::path::Path::new(&e.path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let parent_id = ensure_dir(conn, &parent_path, None, &mut path_to_id, &mut next_id);
            let id = next_id;
            next_id += 1;
            IndexStore::insert_entry_v2_with_id(
                conn,
                id,
                parent_id,
                &e.name,
                false,
                false,
                e.size,
                e.size,
                e.modified_at,
                None,
            )
            .expect("insert file");
        }
    }
}
