//! Scheduler tests: the coalescing coordinator (pure), the full-volume recompute
//! ranking (over a synthetic index, no FFI), and the startup-sweep path.

use super::*;

// ── Coalescing coordinator (plan M2 TDD target) ──────────────────────────

/// The core contract: a request while a pass runs does NOT start a second pass —
/// it sets the re-run flag, so the sweep + a concurrent `ScanCompleted` collapse
/// to one pass, then one re-run. This is the "sweep + concurrent completion ⇒ one
/// pass" guarantee (plan Decision 4), unit-tested without an app or a runtime.
#[test]
fn concurrent_requests_coalesce_into_one_pass_plus_one_rerun() {
    let coord = PassCoordinator::new();

    // First request starts a pass.
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    // A second request while it runs coalesces (no second pass).
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    // A third also coalesces onto the SAME pending re-run (not two re-runs).
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);

    // The pass finishes: because a request arrived mid-pass, run once more.
    assert_eq!(coord.finish("root"), FinishOutcome::RunAgain);
    // That re-run finishes with nothing further pending ⇒ done.
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
}

/// After a pass fully finishes (Done), the next request starts a fresh pass — the
/// slot resets, so a later scan completion isn't wrongly coalesced.
#[test]
fn a_new_request_after_done_starts_a_fresh_pass() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
    // A later completion starts a new pass, not a coalesce.
    assert_eq!(coord.request("root"), BeginOutcome::Start);
}

/// Two volumes are independent: a pass running for one never coalesces the other.
#[test]
fn coalescing_is_per_volume() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    // A different volume starts its own pass, not coalesced onto root's.
    assert_eq!(coord.request("smb-nas"), BeginOutcome::Start);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
    assert_eq!(coord.finish("smb-nas"), FinishOutcome::Done);
}

/// Only ONE re-run is buffered no matter how many requests pile up mid-pass: the
/// re-run reruns once and then, with nothing new, is done. (A pathological event
/// storm can't queue N re-runs.)
#[test]
fn many_midpass_requests_buffer_exactly_one_rerun() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    for _ in 0..100 {
        assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    }
    assert_eq!(coord.finish("root"), FinishOutcome::RunAgain);
    assert_eq!(coord.finish("root"), FinishOutcome::Done, "exactly one re-run, not 100");
}

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

    // Recompute directly over the pool (bypassing the registry + async driver).
    // Availability is listing-only here so the ranking doesn't depend on Spotlight
    // (unavailable in the test env) or visits (none) — the redistribution keeps
    // the listing signals summing to the full weight.
    let weights = Weights::default();
    let count = recompute_from_pool(&RecomputeInputs {
        volume_id: ROOT_VOLUME_ID,
        pool: &pool,
        data_dir: dir.path(),
        weights: &weights,
        home: &home.home,
        now_secs: now,
        available: SignalSet::listing_only(),
        visits: &HashMap::new(),
        last_used: &HashMap::new(),
    })
    .expect("recompute");
    assert!(count > 0, "recompute must score folders");

    // Read the resulting weights back.
    let imp_path = importance_db_path(dir.path(), ROOT_VOLUME_ID);
    let store = ImportanceStore::open(&imp_path).expect("open importance store");
    let w = |rel: &str| -> f64 {
        store
            .weight_for(&format!("{}/{rel}", home.home))
            .expect("read")
            .map(|w| w.score)
            .unwrap_or(-1.0)
    };

    let webapp = w("projects/webapp");
    let downloads = w("Downloads");
    let node_modules = w("projects/webapp/node_modules");
    let logs = w("logs");
    let caches = w("Library/Caches");

    assert!(webapp >= 0.0 && downloads >= 0.0, "the meaningful folders were scored");
    assert_eq!(
        node_modules, 0.0,
        "a node_modules folder is floored to 0.0 (denylisted), got {node_modules}"
    );
    assert_eq!(
        caches, 0.0,
        "a Library/Caches folder is floored to 0.0 (system/hidden), got {caches}"
    );
    assert!(
        webapp > logs,
        "the project root ({webapp}) must outrank the monoculture logs folder ({logs})"
    );
    assert!(
        downloads > logs,
        "the mixed Downloads ({downloads}) must outrank the monoculture logs ({logs})"
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

/// Build an index DB over a `SyntheticHome` using the real `IndexStore` +
/// `IndexWriter`, so the recompute reads exactly the schema production reads. We
/// insert each entry with a parent pointer derived from its path.
fn build_index_from_home(index_path: &std::path::Path, home: &crate::importance::fixtures::SyntheticHome) {
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
