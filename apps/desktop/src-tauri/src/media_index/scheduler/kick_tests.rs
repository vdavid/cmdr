//! Kick + defer-until-scored + read-side "has scored" fallback tests (TDD targets
//! for plan M1/M2). Two layers:
//!
//! - **Pure** (no globals): the per-image `local_should_enrich` gate and the
//!   per-volume "was deferred" flag — the defer-until-scored contract at its core.
//! - **Scheduler-level** (`run_pass_blocking` over a registered root read pool): the
//!   dead-start regression (a pass enriches only when the master toggle is on), the
//!   defer → score → enrich bridge end to end, the Fresh-sweep GC the kick makes
//!   live, and the incremental-only / empty importance read-side fallback.
//!
//! The scheduler-level tests drive the process-global root read pool + master-toggle
//! gate, so each holds `crate::indexing::test_read_pool_lock()` to serialize against
//! parallel tests and resets the gate itself.

use std::collections::HashMap;

use super::*;
use crate::importance::store::{ImportanceStore, importance_db_path};
use crate::importance::writer::{ImportanceWriter, WeightRow};
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::media_index::backend::fake::FakeVisionBackend;
use crate::media_index::network::config::NetworkEnrichConfig;
use crate::media_index::predicate::MediaKind;
use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};
use crate::media_index::writer::MediaWriter;

const ROOT: &str = "root";

/// An `Arc<dyn VisionBackend>` over the deterministic fake, for `MediaScheduler::new`.
fn fake_backend() -> Arc<dyn VisionBackend> {
    Arc::new(FakeVisionBackend::new())
}

// ── Pure: the per-image local enrich gate (defer-until-scored) ──────────────

/// A config with the given always-index folders and excluded folders.
fn config_with(always_folders: &[&str], excluded: &[&str]) -> NetworkEnrichConfig {
    NetworkEnrichConfig {
        opted_in_volumes: Default::default(),
        always_index_volumes: Default::default(),
        always_index_folders: always_folders.iter().map(|s| s.to_string()).collect(),
        excluded_folders: excluded.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn unscored_local_defers_the_gated_remainder_but_honors_overrides() {
    // The M1 slider-integrity contract: importance hasn't scored the volume
    // (`scores` None), so a normal folder DEFERS (never "enrich all"), while an
    // explicit always-index override still enriches. Pre-fix this returned `true`
    // for everything (enrich-all), which over-indexed the whole volume permanently.
    let config = config_with(&["/archive"], &[]);
    assert!(
        !local_should_enrich("/photos/a.jpg", None, &config, ROOT),
        "an unscored, non-overridden folder must DEFER, not enrich-all"
    );
    assert!(
        local_should_enrich("/archive/b.jpg", None, &config, ROOT),
        "an always-index override still enriches even while importance is unscored"
    );
}

#[test]
fn excluded_folder_is_a_hard_veto_even_over_an_override() {
    let config = config_with(&["/archive"], &["/archive/secret"]);
    // Excluded beats the override (privacy veto), whether scored or not.
    assert!(!local_should_enrich("/archive/secret/x.jpg", None, &config, ROOT));
    let scores: HashMap<String, f64> = [("/archive/secret".to_string(), 0.9)].into_iter().collect();
    assert!(!local_should_enrich(
        "/archive/secret/x.jpg",
        Some(&scores),
        &config,
        ROOT
    ));
}

#[test]
fn scored_local_enriches_folders_in_the_map_and_defers_the_rest() {
    let config = config_with(&[], &[]);
    // `scores` holds only the above-threshold folders (already filtered by the caller).
    let scores: HashMap<String, f64> = [("/keep".to_string(), 0.8)].into_iter().collect();
    assert!(
        local_should_enrich("/keep/a.jpg", Some(&scores), &config, ROOT),
        "in-map folder enriches"
    );
    assert!(
        !local_should_enrich("/skip/b.jpg", Some(&scores), &config, ROOT),
        "a folder absent from the threshold-filtered map defers"
    );
}

// ── Pure: the per-volume "was deferred" flag (the unscored → scored bridge) ──

#[test]
fn deferred_flag_is_read_once_per_deferral() {
    let sched = MediaScheduler::new(std::env::temp_dir(), fake_backend());
    // Not deferred yet.
    assert!(!sched.take_deferred_for_importance(ROOT));
    // A pass deferred ⇒ the bridge consumes it exactly once (so a later incremental
    // bump doesn't re-kick).
    sched.mark_deferred_for_importance(ROOT);
    assert!(
        sched.take_deferred_for_importance(ROOT),
        "the deferral is observed once"
    );
    assert!(
        !sched.take_deferred_for_importance(ROOT),
        "and cleared, so a later bump won't re-kick"
    );
}

// ── Test helpers for the scheduler-level tests ──────────────────────────────

/// Build a tiny index DB at `path` with `(dir, file)` images (mtime/size fixed).
fn build_index(path: &std::path::Path, files: &[(&str, &str)]) {
    let store = IndexStore::open(path).expect("open index");
    let conn = store.read_conn();
    let mut path_to_id: HashMap<String, i64> = HashMap::new();
    let mut next_id: i64 = ROOT_ID + 1;

    fn ensure_dir(
        conn: &rusqlite::Connection,
        dir: &str,
        path_to_id: &mut HashMap<String, i64>,
        next_id: &mut i64,
    ) -> i64 {
        if dir.is_empty() || dir == "/" {
            return ROOT_ID;
        }
        if let Some(&id) = path_to_id.get(dir) {
            return id;
        }
        let parent = std::path::Path::new(dir)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let parent_id = ensure_dir(conn, &parent, path_to_id, next_id);
        let name = std::path::Path::new(dir)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let id = *next_id;
        *next_id += 1;
        IndexStore::insert_entry_v2_with_id(conn, id, parent_id, &name, true, false, None, None, None, None)
            .expect("insert dir");
        path_to_id.insert(dir.to_string(), id);
        id
    }

    for (dir, name) in files {
        let parent_id = ensure_dir(conn, dir, &mut path_to_id, &mut next_id);
        let id = next_id;
        next_id += 1;
        IndexStore::insert_entry_v2_with_id(
            conn,
            id,
            parent_id,
            name,
            false,
            false,
            Some(10),
            Some(10),
            Some(1),
            None,
        )
        .expect("insert file");
    }
}

/// Seed a FULL-pass importance store (generation stamped, so `folder_scores` reads it
/// as scored the normal way).
fn seed_importance_full_pass(data_dir: &std::path::Path, rows: &[(&str, f64)]) {
    let path = importance_db_path(data_dir, ROOT);
    ImportanceStore::open(&path).expect("open importance store");
    let writer = ImportanceWriter::spawn(&path).expect("importance writer");
    let rows = rows
        .iter()
        .map(|(p, s)| WeightRow {
            path: p.to_string(),
            score: *s,
            signals_json: "{}".to_string(),
        })
        .collect();
    writer.write_weights(1, rows).expect("write weights");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

/// Seed an INCREMENTAL-only importance store: weight rows present, but NO generation
/// stamped (the incremental path never bumps it) — the exact shape of the dev
/// `importance-root.db` that broke detection (233k rows, generation 0).
fn seed_importance_incremental_only(data_dir: &std::path::Path, rows: &[(&str, f64)]) {
    let path = importance_db_path(data_dir, ROOT);
    ImportanceStore::open(&path).expect("open importance store");
    let writer = ImportanceWriter::spawn(&path).expect("importance writer");
    let rows = rows
        .iter()
        .map(|(p, s)| WeightRow {
            path: p.to_string(),
            score: *s,
            signals_json: "{}".to_string(),
        })
        .collect();
    // No delete_subtrees, and crucially: this does NOT advance the stored generation.
    writer
        .write_weights_incremental(1, rows, Vec::new())
        .expect("write incremental");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

/// A scratch media writer over the scheduler's data dir, to pre-seed `media.db` rows.
fn seed_media_row(data_dir: &std::path::Path, path: &str) {
    let db_path = media_db_path(data_dir, ROOT);
    MediaStore::open(&db_path).expect("open media store");
    let writer = MediaWriter::spawn(&db_path).expect("media writer");
    writer
        .upsert(
            MediaStatusRow {
                path: path.to_string(),
                mtime: Some(10),
                size: Some(10),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "fake-vision-1".to_string(),
            },
            Some(crate::media_index::writer::UpsertAnalysis::ocr_only("seeded")),
        )
        .expect("seed row");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

fn reset_gate() {
    gate::set_enabled(false);
    gate::set_importance_threshold(gate::DEFAULT_IMPORTANCE_THRESHOLD);
    network::config::set_config(NetworkEnrichConfig::default());
}

// ── Scheduler-level: dead-start, defer→score→enrich, Fresh-sweep GC ─────────

#[test]
fn a_pass_no_ops_while_disabled_and_enriches_once_enabled() {
    // The dead-start regression at the pass level: with the master toggle OFF a pass
    // does nothing (returns Ok(0)); flipping it on (what the toggle kick does) makes
    // the same pass enrich. Importance is seeded scored so nothing defers.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/keep", "a.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    seed_importance_full_pass(dir.path(), &[("/keep", 0.9)]);

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());

    // Disabled ⇒ no-op.
    assert_eq!(
        sched.run_pass_blocking(ROOT).expect("pass"),
        0,
        "a disabled pass enriches nothing"
    );
    assert!(
        MediaStore::open(&media_db_path(dir.path(), ROOT))
            .expect("open")
            .status_for("/keep/a.jpg")
            .expect("read")
            .is_none(),
        "nothing enriched while disabled"
    );

    // Enabled ⇒ the pass enriches (the dead-start the toggle kick fixes).
    gate::set_enabled(true);
    assert_eq!(
        sched.run_pass_blocking(ROOT).expect("pass"),
        1,
        "an enabled pass enriches"
    );
    assert!(
        MediaStore::open(&media_db_path(dir.path(), ROOT))
            .expect("open")
            .status_for("/keep/a.jpg")
            .expect("read")
            .is_some(),
        "the image enriched once enabled"
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn defers_until_importance_scores_then_enriches_at_the_threshold() {
    // The slider-integrity regression, end to end. Seed a GENUINELY EMPTY importance
    // store (no weights, no generation) so `folder_scores` is `None` and the volume
    // truly defers — with any weights present the `scored_folder_count > 0` fallback
    // would read it as scored and it would never defer.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);
    gate::set_importance_threshold(0.0);

    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/keep", "a.jpg"), ("/archive", "b.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");

    // An always-index override on /archive: it must enrich even while unscored.
    network::config::set_config(config_with(&["/archive"], &[]));

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());

    // Phase 1: importance unscored ⇒ /keep defers, /archive (override) enriches.
    assert_eq!(
        sched.run_pass_blocking(ROOT).expect("pass 1"),
        1,
        "only the override enriches while unscored"
    );
    let store = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open");
    assert!(
        store.status_for("/archive/b.jpg").expect("read").is_some(),
        "override enriched"
    );
    assert!(
        store.status_for("/keep/a.jpg").expect("read").is_none(),
        "gated folder deferred"
    );
    assert!(
        sched.take_deferred_for_importance(ROOT),
        "the pass recorded that it deferred on importance (the bridge would re-kick)"
    );

    // Phase 2: importance completes a full pass scoring /keep ⇒ the next pass enriches
    // it at the threshold (what the unscored → scored bridge triggers in production).
    seed_importance_full_pass(dir.path(), &[("/keep", 0.9)]);
    assert_eq!(
        sched.run_pass_blocking(ROOT).expect("pass 2"),
        1,
        "the deferred folder now enriches"
    );
    let store = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open");
    assert!(
        store.status_for("/keep/a.jpg").expect("read").is_some(),
        "gated folder enriched after scoring"
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn a_kicked_pass_gcs_a_vanished_row_against_the_walked_set() {
    // The Fresh-sweep GC path the startup kick makes live: a pass over the complete
    // walked set collects a stored row whose file vanished from the index.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);

    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    // The index holds only keep.jpg now (gone.jpg vanished).
    build_index(&index_path, &[("/photos", "keep.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    seed_importance_full_pass(dir.path(), &[("/photos", 0.9)]);
    // media.db still carries both — including the vanished gone.jpg.
    seed_media_row(dir.path(), "/photos/keep.jpg");
    seed_media_row(dir.path(), "/photos/gone.jpg");

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    sched.run_pass_blocking(ROOT).expect("pass");

    let store = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open");
    assert!(
        store.status_for("/photos/keep.jpg").expect("read").is_some(),
        "present file kept"
    );
    assert!(
        store.status_for("/photos/gone.jpg").expect("read").is_none(),
        "vanished file GC'd"
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

// ── M2 read-side fallback: incremental-only / empty importance store ─────────

#[test]
fn folder_scores_reads_an_incremental_only_store_as_scored() {
    // The core M2 detection bug: a store with weight rows but NO generation (only
    // incremental rescores ran) must read as SCORED, not "never scored". Pre-fix
    // `folder_scores` gated on `recompute_generation() == 0` and returned `None`,
    // reporting "0 covered" at every threshold despite usable weights.
    let dir = tempfile::tempdir().expect("temp");
    seed_importance_incremental_only(dir.path(), &[("/photos", 0.8)]);

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    let scores = sched.folder_scores(ROOT, 0.0);
    assert!(
        scores.is_some(),
        "incremental-only weights (generation 0) read as scored"
    );
    assert!(
        scores.expect("some").contains_key("/photos"),
        "the weight row is visible"
    );
}

#[test]
fn folder_scores_is_none_for_a_genuinely_empty_store() {
    // No importance.db at all ⇒ genuinely unscored ⇒ `None` (so the local pass defers).
    let dir = tempfile::tempdir().expect("temp");
    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    assert!(sched.folder_scores(ROOT, 0.0).is_none(), "a missing store is unscored");
}
