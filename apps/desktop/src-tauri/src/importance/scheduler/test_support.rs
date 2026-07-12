//! Shared helpers and common re-exports for the scheduler test modules. Each
//! `*_tests.rs` file does `use super::*;` (the scheduler's public items) plus
//! `use super::test_support::*;` (these helpers and common re-exports), so the
//! test bodies read the same as when they all lived in one `tests.rs`.

use super::*;

// Re-exports the split test bodies reference by bare name (the scheduler's
// glob doesn't cover the recompute internals or these crate paths).
pub(super) use super::recompute::{
    ANCESTOR_WALK_CAP, RecomputeInputs, recompute_folders, score_folders, touched_folder_set,
};
pub(super) use crate::importance::signals::OptionalSignals;
pub(super) use crate::importance::store::{ImportanceStore, importance_db_path};
pub(super) use crate::importance::writer::WeightRow;
pub(super) use crate::indexing::ROOT_VOLUME_ID;

/// Build a synthetic `IndexFolder` for a directory path with a couple of mixed
/// files (so an un-floored one scores above zero), computing `under_floored_ancestor`
/// from the shared classifier over the whole path set. Lets a transition test drive
/// `incremental_rescore` directly with a hand-built walk (no index needed).
pub(super) fn folder_at(id: i64, path: &str, home: &str, all_paths: &[&str]) -> recompute::IndexFolder {
    use crate::importance::classify::under_floored_paths;
    use crate::importance::signals::ChildAggregate;
    use crate::indexing::store::EntryRow;
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let under = under_floored_paths(all_paths.iter().copied(), home).contains(path);
    recompute::IndexFolder {
        entry: EntryRow {
            id,
            parent_id: 0,
            name,
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: Some(1_000_000_000),
            inode: None,
        },
        path: path.to_string(),
        children: ChildAggregate {
            distinct_extension_count: 3,
            file_count: 4,
            has_direct_marker: false,
        },
        has_marker_below: false,
        under_floored_ancestor: under,
    }
}

/// Full-pass a hand-built walk, returning the writer + store path for a follow-up
/// incremental. Shared by the two transition tests.
pub(super) fn full_pass_walk(
    dir: &std::path::Path,
    home: &str,
    folders: &[recompute::IndexFolder],
) -> ImportanceWriter {
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
        },
        folders,
    )
    .expect("full pass");
    writer.flush_blocking().expect("flush");
    writer
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
