//! Full-volume recompute tests (synthetic index, no FFI): the ranking that lifts
//! meaningful folders above machine output, the O(dirs)-walk aggregate
//! characterization, and the descendant-floor invariant.

use super::test_support::*;
use super::*;

// ── Full-volume recompute ranking (synthetic index, no FFI) ───────────────

/// Build a synthetic index DB from a `SyntheticHome`, run the recompute over a
/// directly-constructed read pool, and assert `importance.db` holds a ranking
/// that puts the project root and mixed user-content folders above the
/// machine-output folders (node_modules, caches, logs). This is the M2
/// integration test: fake signals via a real index, no FFI, no registry.
#[test]
fn full_recompute_ranks_meaningful_folders_above_machine_output() {
    use crate::importance::fixtures::SyntheticHome;

    let now = 1_000_000_000;
    let home = SyntheticHome::canonical(now);

    // Materialize a real index DB over the synthetic tree and a read pool over it.
    let dir = tempfile::tempdir().expect("temp dir");
    let index_path = dir.path().join("index-root.db");
    build_index_from_home(&index_path, &home);
    let pool = crate::indexing::ReadPool::new(index_path).expect("read pool");

    // Walk once (the single-pass walk), then recompute over that walk through a
    // shared writer (bypassing the registry + async driver). Availability is
    // listing-only here so the ranking doesn't depend on Spotlight (unavailable in
    // the test env) or visits (none) — the redistribution keeps the listing
    // signals summing to the full weight.
    let folders = pool
        .with_conn(|conn| walk_index_folders(conn, &home.home))
        .expect("pool")
        .expect("walk");
    let writer = ImportanceWriter::spawn(&importance_db_path(dir.path(), ROOT_VOLUME_ID)).expect("writer");
    let weights = Weights::default();
    let outcome = recompute_folders(
        &RecomputeInputs {
            writer: &writer,
            weights: &weights,
            home: &home.home,
            now_secs: now,
            available: SignalSet::listing_only(),
            visits: &HashMap::new(),
            last_used: &HashMap::new(),
        },
        &folders,
    )
    .expect("recompute");
    writer.flush_blocking().expect("flush");
    assert!(outcome.count > 0, "recompute must score folders");

    // Read the resulting weights back.
    let imp_path = importance_db_path(dir.path(), ROOT_VOLUME_ID);
    let store = ImportanceStore::open(&imp_path).expect("open importance store");
    // A scored folder's stored score, or `-1.0` when it has NO row. Floored folders
    // deliberately get no row (storage compaction), so their lookup is absent.
    let w = |rel: &str| -> f64 {
        store
            .weight_for(&format!("{}/{rel}", home.home))
            .expect("read")
            .map(|w| w.score)
            .unwrap_or(-1.0)
    };
    let has_row = |rel: &str| -> bool {
        store
            .weight_for(&format!("{}/{rel}", home.home))
            .expect("read")
            .is_some()
    };

    let webapp = w("projects/webapp");
    let downloads = w("Downloads");
    let invoices = w("Documents/invoices");

    assert!(webapp > 0.0 && downloads > 0.0, "the meaningful folders were scored");
    // Floored folders get NO row at all — their floored-ness is re-derivable from
    // the path. A denylisted node_modules, a system/hidden Library/Caches, and the
    // denylisted `logs` monoculture all floor.
    assert!(
        !has_row("projects/webapp/node_modules"),
        "a node_modules folder is floored, so it has no stored row"
    );
    assert!(
        !has_row("Library/Caches"),
        "a Library/Caches folder is floored (system/hidden), so it has no stored row"
    );
    assert!(
        !has_row("logs"),
        "the denylisted `logs` folder is floored, so it has no stored row"
    );
    // The project root outranks a plain user-content subtree (both scored, both rows).
    assert!(
        webapp > invoices,
        "the project root ({webapp}) must outrank a plain user-content folder ({invoices})"
    );

    // Every written weight carries the pass generation (as-of marker), and the
    // store's current generation matches — the staleness contract, end to end.
    let webapp_row = store
        .weight_for(&format!("{}/projects/webapp", home.home))
        .expect("read")
        .expect("scored");
    assert_eq!(
        webapp_row.as_of_generation,
        store.recompute_generation().expect("gen"),
        "a freshly-written weight is stamped at the current generation"
    );
}

// ── Memory-fix characterization (plan M4: O(dirs) walk) ───────────────────

/// The O(dirs) walk (directories materialized, files streamed into per-parent
/// accumulators) must produce EXACTLY the aggregate a whole-tree walk would: for
/// every folder, the distinct-extension count, file count, and direct-marker flag
/// its direct children collapse to. This pins the memory restructure's output —
/// the aggregate is the only thing the fix changed, so any drift in what a child
/// contributes (a `.git` directory marker, a `Cargo.toml` file marker, an
/// extension folded wrong) fails here. The independent oracle is the fixture's own
/// listing, re-derived directly (not through the walk under test).
#[test]
fn odirs_walk_aggregates_children_like_a_whole_tree_walk() {
    use crate::importance::fixtures::SyntheticHome;
    use crate::importance::signals::ChildAggregate;

    let now = 1_000_000_000;
    let home = SyntheticHome::canonical(now);
    let dir = tempfile::tempdir().expect("temp dir");
    let index_path = dir.path().join("index-root.db");
    build_index_from_home(&index_path, &home);
    let pool = crate::indexing::ReadPool::new(index_path).expect("read pool");

    let folders = pool
        .with_conn(|conn| walk_index_folders(conn, &home.home))
        .expect("pool")
        .expect("walk");
    assert!(!folders.is_empty(), "the walk found folders");

    for f in &folders {
        // Re-derive the expected aggregate straight from the fixture's OWN listing
        // — an oracle independent of the walk under test.
        let children: Vec<_> = home.direct_children(&f.path).collect();
        let files: Vec<&str> = children
            .iter()
            .filter(|c| !c.is_directory)
            .map(|c| c.name.as_str())
            .collect();
        let expected_ext = crate::importance::scorer::extension_count(files.iter().copied());
        // A marker child is any child (file OR directory) whose folded name is a
        // project marker — `.git` is a directory, `Cargo.toml` a file.
        let has_marker = children
            .iter()
            .any(|c| crate::importance::classify::is_project_marker(&c.name.to_lowercase()));
        assert_eq!(
            f.children,
            ChildAggregate {
                distinct_extension_count: expected_ext,
                file_count: files.len() as u32,
                has_direct_marker: has_marker,
            },
            "the O(dirs) walk's aggregate for {} must match the fixture's own listing",
            f.path
        );
    }
}

// ── Descendant-floor: a floored ancestor floors its whole subtree ─────────

/// The walk marks every folder UNDER a self-flooring ancestor (a denylisted /
/// hidden / system folder) as `under_floored_ancestor`, and the recompute floors
/// it to 0 — so a `node_modules`'s whole subtree floors, not just the folder named
/// `node_modules`. Pre-fix, a deep `node_modules/pkg/dist` inherited a project-root
/// prior and scored near the top; this pins that it now floors.
#[test]
fn descendants_of_a_floored_folder_floor_too() {
    use crate::indexing::store::{IndexStore, ROOT_ID};

    let dir = tempfile::tempdir().expect("temp dir");
    let index_path = dir.path().join("index-root.db");
    let home = "/Users/test";

    // Build a tiny index: a project with a .git (so the project root is marked),
    // and a node_modules holding a nested package with a `dist` and a vendored repo
    // (its own .git) — the folders that must all floor as node_modules descendants.
    {
        let store = IndexStore::open(&index_path).expect("open index");
        let conn = store.read_conn();
        let mut path_to_id: HashMap<String, i64> = HashMap::new();
        let mut next_id: i64 = ROOT_ID + 1;
        let mkdir =
            |conn: &rusqlite::Connection, path: &str, path_to_id: &mut HashMap<String, i64>, next_id: &mut i64| {
                let parent = std::path::Path::new(path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string());
                let parent_id = match parent.as_deref() {
                    Some("") | Some("/") | None => ROOT_ID,
                    Some(pp) => *path_to_id.get(pp).unwrap_or(&ROOT_ID),
                };
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let id = *next_id;
                *next_id += 1;
                IndexStore::insert_entry_v2_with_id(
                    conn,
                    id,
                    parent_id,
                    &name,
                    true,
                    false,
                    None,
                    None,
                    Some(1_000_000_000),
                    None,
                )
                .expect("insert dir");
                path_to_id.insert(path.to_string(), id);
                id
            };
        // Directories, parent-first.
        for d in [
            "/Users",
            "/Users/test",
            "/Users/test/proj",
            "/Users/test/proj/.git",
            "/Users/test/proj/node_modules",
            "/Users/test/proj/node_modules/react",
            "/Users/test/proj/node_modules/react/dist",
            "/Users/test/proj/node_modules/vendored",
            "/Users/test/proj/node_modules/vendored/.git",
        ] {
            mkdir(conn, d, &mut path_to_id, &mut next_id);
        }
        // A package.json under proj so it reads as a project root, and a mixed file
        // under the deep dist so it'd score high absent the fix.
        let proj_id = *path_to_id.get("/Users/test/proj").expect("proj");
        IndexStore::insert_entry_v2_with_id(
            conn,
            next_id,
            proj_id,
            "package.json",
            false,
            false,
            Some(1),
            Some(1),
            Some(1_000_000_000),
            None,
        )
        .expect("insert file");
        next_id += 1;
        let dist_id = *path_to_id
            .get("/Users/test/proj/node_modules/react/dist")
            .expect("dist");
        for fname in ["a.js", "b.ts", "c.css"] {
            IndexStore::insert_entry_v2_with_id(
                conn,
                next_id,
                dist_id,
                fname,
                false,
                false,
                Some(1),
                Some(1),
                Some(1_000_000_000),
                None,
            )
            .expect("insert file");
            next_id += 1;
        }
    }

    let pool = crate::indexing::ReadPool::new(index_path).expect("read pool");
    let folders = pool
        .with_conn(|conn| walk_index_folders(conn, home))
        .expect("pool")
        .expect("walk");

    let by_path = |p: &str| {
        folders
            .iter()
            .find(|f| f.path == p)
            .unwrap_or_else(|| panic!("missing {p}"))
    };

    // The project root itself is NOT under-floored.
    assert!(
        !by_path("/Users/test/proj").under_floored_ancestor,
        "the project root itself is not floored"
    );
    // Every folder under node_modules is under-floored, including the vendored repo.
    for p in [
        "/Users/test/proj/node_modules/react",
        "/Users/test/proj/node_modules/react/dist",
        "/Users/test/proj/node_modules/vendored",
        "/Users/test/proj/node_modules/vendored/.git",
    ] {
        assert!(
            by_path(p).under_floored_ancestor,
            // allowed-pluralize-noun: `{p}` is a path, not a count.
            "expected under-floored (lives under node_modules): {p}"
        );
    }

    // Score the walk and assert the deep dist floors to 0 despite mixed, recent files.
    let writer = ImportanceWriter::spawn(&importance_db_path(dir.path(), ROOT_VOLUME_ID)).expect("writer");
    let weights = Weights::default();
    recompute_folders(
        &RecomputeInputs {
            writer: &writer,
            weights: &weights,
            home,
            now_secs: 1_000_000_000,
            available: SignalSet::listing_only(),
            visits: &HashMap::new(),
            last_used: &HashMap::new(),
        },
        &folders,
    )
    .expect("recompute");
    writer.flush_blocking().expect("flush");

    let store = ImportanceStore::open(&importance_db_path(dir.path(), ROOT_VOLUME_ID)).expect("open");
    let score_of = |p: &str| store.weight_for(p).expect("read").map(|w| w.score).unwrap_or(-1.0);
    let has_row = |p: &str| store.weight_for(p).expect("read").is_some();
    // Floored folders get no row at all (storage compaction), so their subtree
    // never carries a stored weight — the strongest form of "floored".
    assert!(
        !has_row("/Users/test/proj/node_modules/react/dist"),
        "a deep node_modules dir floors, so it has no stored row"
    );
    assert!(
        !has_row("/Users/test/proj/node_modules/vendored"),
        "a vendored repo under node_modules stays floored, so it has no stored row"
    );
    assert!(score_of("/Users/test/proj") > 0.0, "the real project root still scores");
    writer.shutdown();
}
