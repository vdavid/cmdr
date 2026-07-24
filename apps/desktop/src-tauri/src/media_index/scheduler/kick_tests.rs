//! Kick + defer-until-scored + read-side "has scored" fallback tests. Two layers:
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

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::*;
use crate::importance::store::{ImportanceStore, importance_db_path};
use crate::importance::writer::{ImportanceWriter, WeightRow};
use crate::indexing::IndexVolumeKind;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::media_index::backend::fake::FakeVisionBackend;
use crate::media_index::network::config::NetworkEnrichConfig;
use crate::media_index::predicate::MediaKind;
use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};
use crate::media_index::writer::MediaWriter;
use crate::test_support::wait_until;

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
    // The slider-integrity contract: importance hasn't scored the volume
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

// The privacy exclusion is no longer part of `local_should_enrich` (which is now the
// pure COVERAGE gate); it's a live hard veto applied in `enrich::enrich_and_gc`, tested
// there (`exclusion_vetoes_even_an_override_covered_image`,
// `exclusion_landing_during_analyze_writes_no_row`).

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

/// Build a tiny index DB at `path` with `(dir, file)` images (mtime/size fixed). Shared
/// with `reclaim_tests` (same shape).
pub(super) fn build_index(path: &std::path::Path, files: &[(&str, &str)]) {
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

/// A scratch media writer over the scheduler's data dir, to pre-seed a `media.db` row
/// for `volume_id` at `path`.
fn seed_media_row_for(data_dir: &std::path::Path, volume_id: &str, path: &str) {
    let db_path = media_db_path(data_dir, volume_id);
    MediaStore::open(&db_path).expect("open media store");
    let writer = MediaWriter::spawn(&db_path, volume_id).expect("media writer");
    writer
        .upsert(
            MediaStatusRow {
                path: path.to_string(),
                mtime: Some(10),
                size: Some(10),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "fake-vision-1".to_string(),
                clip_stamp: String::new(),
            },
            Some(crate::media_index::writer::UpsertAnalysis::ocr_only("seeded")),
        )
        .expect("seed row");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

/// A scratch media writer over the scheduler's data dir, to pre-seed `media.db` rows.
fn seed_media_row(data_dir: &std::path::Path, path: &str) {
    let db_path = media_db_path(data_dir, ROOT);
    MediaStore::open(&db_path).expect("open media store");
    let writer = MediaWriter::spawn(&db_path, ROOT).expect("media writer");
    writer
        .upsert(
            MediaStatusRow {
                path: path.to_string(),
                mtime: Some(10),
                size: Some(10),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "fake-vision-1".to_string(),
                clip_stamp: String::new(),
            },
            Some(crate::media_index::writer::UpsertAnalysis::ocr_only("seeded")),
        )
        .expect("seed row");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}

pub(super) fn reset_gate() {
    gate::set_enabled(false);
    gate::set_scope(gate::DEFAULT_SCOPE);
    gate::set_importance_threshold(gate::DEFAULT_IMPORTANCE_THRESHOLD);
    network::config::set_config(NetworkEnrichConfig::default());
}

/// Put the process-global gate in the AUTOMATIC scope, for the tests that exercise
/// importance-driven coverage. The default scope indexes only the user's chosen
/// folders, so a test about the importance threshold has to say so — it's asking for
/// the non-default mode. The narrow scope has its own tests (`lifecycle`, `coverage`,
/// `reclaim_tests`).
pub(super) fn use_automatic_scope() {
    gate::set_scope(gate::IndexScope::ByImportance);
}

/// The budget for a POSITIVE wait on a kicked pass. The kick paths spawn their pass on the
/// tauri runtime (off this test thread), so an assertion on the pass's effect has to wait
/// for it.
///
/// The real work is sub-millisecond, but the kick crosses several scheduling hops before
/// the row is observable (async spawn → blocking-pool thread → SQLite writes → the media
/// writer thread → the poll's own read), and each hop can wait hundreds of ms when the
/// host is saturated (measured 2026-07-24: at load ~150 — full slow-check suite plus a
/// Docker workspace build — a 5 s budget expired with the pass landing fine on the next
/// run; solo the same test lands in ~80 ms). The wait returns on the first poll that
/// sees the row, so this ceiling costs nothing on the happy path and only ever elapses
/// on a genuine dead-start regression. The matching nextest override in
/// `.config/nextest.toml` (`kick_tests` block) keeps the process cap above this budget;
/// keep the two in sync.
const PASS_LANDS_WITHIN: Duration = Duration::from_secs(20);

/// Whether `cond` stays false for a full second — for a NEGATIVE assertion. A spurious
/// enrichment would land in tens of milliseconds, so 1 s is ample without dragging the
/// suite; the authoritative disabled-pass no-op proof is the synchronous
/// `a_pass_no_ops_while_disabled_and_enriches_once_enabled`.
fn never_within_a_second(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if cond() {
            return false;
        }
        // allowed-test-sleep: a negative assertion has no condition to wait on; the window IS the test
        std::thread::sleep(Duration::from_millis(20));
    }
    !cond()
}

/// Whether `media.db` for `volume_id` under `data_dir` carries an enriched row for
/// `path` — the "did a spawned pass enrich it?" probe the async kick tests poll on.
fn has_enriched_row(data_dir: &std::path::Path, volume_id: &str, path: &str) -> bool {
    MediaStore::open(&media_db_path(data_dir, volume_id))
        .ok()
        .and_then(|s| s.status_for(path).ok().flatten())
        .is_some()
}

// ── Async: the dead-start kicks that actually start a pass ───────────────────

#[test]
fn wire_volume_kicks_an_initial_pass_for_a_fresh_at_launch_volume() {
    // The restart race (item 1): `start()`'s sweep kick can run before a volume is
    // ready; the volume then registers and `wire_volume` runs from the registration
    // bus. Its lifecycle-bus subscription never kicks a Fresh-at-launch volume (the bus
    // stays `Pending`, never `Completed`), and the importance bridge only re-kicks
    // volumes that DEFERRED — so without an initial kick in `wire_volume` a persisted-on
    // toggle enriches nothing until the user re-toggles. Importance is seeded SCORED so
    // nothing defers: the ONLY path that can enrich here is the `wire_volume` kick.
    // Pre-fix this stays un-enriched (the regression).
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    // Importance-driven coverage, so ask for the automatic scope (the default indexes
    // only the user's chosen folders).
    use_automatic_scope();
    gate::set_enabled(true);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/keep", "a.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    seed_importance_full_pass(dir.path(), &[("/keep", 0.9)]);

    let sched = Arc::new(MediaScheduler::new(dir.path().to_path_buf(), fake_backend()));
    // Wire the volume exactly as the registration bus would: its bus is Pending (we
    // never publish a ScanCompleted) — the Fresh-at-launch shape the race hits.
    wire_volume(Arc::clone(&sched), ROOT.to_string(), IndexVolumeKind::Local);

    wait_until(
        PASS_LANDS_WITHIN,
        "wire_volume to kick an initial pass for a Fresh-at-launch volume (the restart race)",
        || has_enriched_row(dir.path(), ROOT, "/keep/a.jpg"),
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn kick_runs_a_pass_for_a_ready_volume_only_when_enabled() {
    // The dead-start end to end through the kick core (item 2a): a ready Local volume
    // with the master toggle ON gets a real enrichment pass; with it OFF the spawned
    // pass self-gates and enriches nothing; and an MTP entry in the ready list is never
    // kicked (on-demand only). Driving the extracted `kick_ready_passes_from` with a
    // controlled list keeps this hermetic — no process-global index registry.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    // Importance-driven coverage, so ask for the automatic scope (the default indexes
    // only the user's chosen folders).
    use_automatic_scope();
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/keep", "a.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    seed_importance_full_pass(dir.path(), &[("/keep", 0.9)]);

    let sched = Arc::new(MediaScheduler::new(dir.path().to_path_buf(), fake_backend()));
    // A ready Local root plus an MTP volume that must never be kicked.
    let ready = || {
        vec![
            (ROOT.to_string(), IndexVolumeKind::Local),
            ("phone-mtp".to_string(), IndexVolumeKind::Mtp),
        ]
    };

    // Gate OFF: kicking still spawns the passes, but each self-gates ⇒ nothing enriches.
    kick_ready_passes_from(&sched, ready());
    assert!(
        never_within_a_second(|| has_enriched_row(dir.path(), ROOT, "/keep/a.jpg")),
        "a disabled kick enriches nothing (the pass self-gates on the master toggle)"
    );

    // Gate ON: the ready Local volume enriches; the MTP entry never does.
    gate::set_enabled(true);
    kick_ready_passes_from(&sched, ready());
    wait_until(
        PASS_LANDS_WITHIN,
        "an enabled kick to run the ready volume's pass (the dead-start fix)",
        || has_enriched_row(dir.path(), ROOT, "/keep/a.jpg"),
    );
    assert!(
        !media_db_path(dir.path(), "phone-mtp").exists(),
        "MTP is never kicked, so no media.db is created for it"
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

// ── Scheduler-level: dead-start, defer→score→enrich, Fresh-sweep GC ─────────

#[test]
fn a_pass_no_ops_while_disabled_and_enriches_once_enabled() {
    // The dead-start regression at the pass level: with the master toggle OFF a pass
    // does nothing (returns Ok(0)); flipping it on (what the toggle kick does) makes
    // the same pass enrich. Importance is seeded scored so nothing defers.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    // Importance-driven coverage, so ask for the automatic scope (the default indexes
    // only the user's chosen folders).
    use_automatic_scope();
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
fn the_narrow_scope_enriches_only_the_chosen_folder_end_to_end() {
    // The feature's whole promise, through a real pass: with the scope at its default,
    // a high-importance folder nobody chose stays unindexed while the chosen one
    // enriches — at the BROADEST slider position (0.0), so the threshold is provably not
    // what's holding the other folder back.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);
    gate::set_importance_threshold(0.0);

    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/chosen", "a.jpg"), ("/important", "b.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    // Importance scores /important highly — in the automatic scope it would be covered.
    seed_importance_full_pass(dir.path(), &[("/important", 1.0)]);
    network::config::set_config(config_with(&["/chosen"], &[]));

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    assert_eq!(sched.run_pass_blocking(ROOT).expect("pass"), 1, "one image enriched");
    let store = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open");
    assert!(
        store.status_for("/chosen/a.jpg").expect("read").is_some(),
        "the folder the user chose is indexed"
    );
    assert!(
        store.status_for("/important/b.jpg").expect("read").is_none(),
        "a folder nobody chose stays unindexed, however important it scores"
    );
    assert!(
        !sched.take_deferred_for_importance(ROOT),
        "nothing is deferred on importance: this scope isn't waiting for it"
    );

    // Same setup, the other scope: now importance broadens coverage to both.
    use_automatic_scope();
    assert_eq!(
        sched.run_pass_blocking(ROOT).expect("pass"),
        1,
        "the importance-covered folder enriches once the scope allows it"
    );
    let store = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open");
    assert!(
        store.status_for("/important/b.jpg").expect("read").is_some(),
        "the automatic scope picks it up"
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
    // Importance-driven coverage, so ask for the automatic scope (the default indexes
    // only the user's chosen folders).
    use_automatic_scope();
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

// ── Read-side "has scored" fallback: incremental-only / empty importance store ─────────

#[test]
fn folder_scores_reads_an_incremental_only_store_as_scored() {
    // The core has-scored detection bug: a store with weight rows but NO generation (only
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

// ── Privacy retro-delete: across path spaces, and only reachable volumes ──

#[test]
fn retro_delete_prunes_a_local_folder_and_skips_volumes_it_isnt_under() {
    // Excluding an OS folder deletes its rows on the local volume (index path == OS
    // path) and does NOT touch a NAS the folder isn't under (its index space is
    // different).
    let dir = tempfile::tempdir().expect("temp");
    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    seed_media_row_for(dir.path(), ROOT, "/secret/id.jpg");
    seed_media_row_for(dir.path(), ROOT, "/keep/a.jpg");
    seed_media_row_for(dir.path(), "smb-vol", "/Photos/p.jpg");

    let deleted = sched.retro_delete_excluded_folder(
        "/secret",
        &[
            (ROOT.to_string(), "/".to_string()),
            ("smb-vol".to_string(), "/Volumes/naspi".to_string()),
        ],
    );
    assert_eq!(
        deleted, 1,
        "only the local /secret row goes; /secret isn't under the NAS mount"
    );

    let root = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open root");
    assert!(
        root.status_for("/secret/id.jpg").expect("read").is_none(),
        "excluded gone"
    );
    assert!(root.status_for("/keep/a.jpg").expect("read").is_some(), "sibling kept");
    let smb = MediaStore::open(&media_db_path(dir.path(), "smb-vol")).expect("open smb");
    assert!(
        smb.status_for("/Photos/p.jpg").expect("read").is_some(),
        "the NAS row is untouched (the folder isn't under its mount)"
    );
}

#[test]
fn retro_delete_maps_a_network_folder_into_the_volumes_index_space() {
    // Excluding an OS-mount folder on a NAS strips the mount root to reach the stored
    // (index-relative) rows.
    let dir = tempfile::tempdir().expect("temp");
    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    seed_media_row_for(dir.path(), "smb-vol", "/Photos/p.jpg");
    seed_media_row_for(dir.path(), "smb-vol", "/Docs/d.jpg");

    let deleted = sched.retro_delete_excluded_folder(
        "/Volumes/naspi/Photos",
        &[("smb-vol".to_string(), "/Volumes/naspi".to_string())],
    );
    assert_eq!(deleted, 1);

    let smb = MediaStore::open(&media_db_path(dir.path(), "smb-vol")).expect("open smb");
    assert!(
        smb.status_for("/Photos/p.jpg").expect("read").is_none(),
        "mapped + pruned"
    );
    assert!(
        smb.status_for("/Docs/d.jpg").expect("read").is_some(),
        "other folder kept"
    );
}

// ── Live enrichment: the scoped tick end to end (over a registered read pool) ──

fn touched(dirs: &[&str]) -> HashSet<String> {
    dirs.iter().map(|d| d.to_string()).collect()
}

#[test]
fn a_live_tick_re_enriches_a_modified_covered_image() {
    // A modified covered image re-enriches on a live tick, no completed scan needed. The
    // stored row's `(mtime, size)` is stale vs the index (10 vs the index's 1), so the
    // staleness predicate marks it dirty and the tick re-analyzes it.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    // Importance-driven coverage, so ask for the automatic scope (the default indexes
    // only the user's chosen folders).
    use_automatic_scope();
    gate::set_enabled(true);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/keep", "a.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    seed_importance_full_pass(dir.path(), &[("/keep", 0.9)]);
    seed_media_row(dir.path(), "/keep/a.jpg"); // stored mtime 10 ≠ index mtime 1 ⇒ stale

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    let n = sched.run_live_tick_blocking(ROOT, &touched(&["/keep"])).expect("tick");
    assert_eq!(n, 1, "the modified covered image re-enriches on a live tick");

    let store = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open");
    let row = store.status_for("/keep/a.jpg").expect("read").expect("row present");
    assert_eq!(
        row.mtime,
        Some(1),
        "the row now carries the index's current mtime (re-enriched)"
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn a_live_tick_defers_a_below_threshold_folder() {
    // A folder below the slider threshold defers on a live tick, exactly like the full pass:
    // /skip has no score at or above 0.5, so it's absent from the threshold-filtered map and
    // never enriches.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);
    gate::set_importance_threshold(0.5);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/skip", "a.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    // Only /keep scores ≥ threshold; /skip has no row ⇒ not covered.
    seed_importance_full_pass(dir.path(), &[("/keep", 0.9)]);

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    let n = sched.run_live_tick_blocking(ROOT, &touched(&["/skip"])).expect("tick");
    assert_eq!(n, 0, "a below-threshold folder defers on a live tick");
    assert!(
        MediaStore::open(&media_db_path(dir.path(), ROOT))
            .expect("open")
            .status_for("/skip/a.jpg")
            .expect("read")
            .is_none(),
        "no row for the deferred folder"
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn a_live_tick_never_enriches_an_excluded_folder() {
    // The privacy veto holds on a live tick: an excluded folder never enriches, even when
    // importance covers it.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/secret", "a.jpg")]);
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    seed_importance_full_pass(dir.path(), &[("/secret", 0.9)]);
    network::config::set_config(config_with(&[], &["/secret"]));

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    let n = sched
        .run_live_tick_blocking(ROOT, &touched(&["/secret"]))
        .expect("tick");
    assert_eq!(n, 0, "an excluded folder never enriches on a live tick");
    assert!(
        MediaStore::open(&media_db_path(dir.path(), ROOT))
            .expect("open")
            .status_for("/secret/a.jpg")
            .expect("read")
            .is_none(),
        "no row for the excluded folder"
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn a_live_tick_gcs_an_index_confirmed_removal() {
    // An index-confirmed removal is a fact about the tree (not a scan-state inference), so a
    // live tick may delete its row: the index now holds only keep.jpg, so gone.jpg's stored
    // row is scoped-GC'd — while keep.jpg (present) survives.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/photos", "keep.jpg")]); // gone.jpg removed from the index
    crate::indexing::test_install_root_read_pool(index_path).expect("install pool");
    seed_importance_full_pass(dir.path(), &[("/photos", 0.9)]);
    seed_media_row(dir.path(), "/photos/keep.jpg");
    seed_media_row(dir.path(), "/photos/gone.jpg");

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    sched
        .run_live_tick_blocking(ROOT, &touched(&["/photos"]))
        .expect("tick");

    let store = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open");
    assert!(
        store.status_for("/photos/keep.jpg").expect("read").is_some(),
        "present kept"
    );
    assert!(
        store.status_for("/photos/gone.jpg").expect("read").is_none(),
        "index-confirmed removal GC'd on the live tick"
    );

    crate::indexing::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn an_unmounted_volume_live_tick_deletes_nothing() {
    // Unmount safety: with no read pool (the volume is gone), a live tick no-ops entirely —
    // it never GCs, so a disconnect can't wipe a volume's coverage.
    let _guard = crate::indexing::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);
    crate::indexing::test_uninstall_root_read_pool(); // ensure no pool is installed
    let dir = tempfile::tempdir().expect("temp");
    seed_media_row(dir.path(), "/photos/keep.jpg");
    seed_media_row(dir.path(), "/photos/gone.jpg");

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    let n = sched
        .run_live_tick_blocking(ROOT, &touched(&["/photos"]))
        .expect("tick");
    assert_eq!(n, 0, "no read pool ⇒ the tick no-ops");

    let store = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open");
    assert!(
        store.status_for("/photos/keep.jpg").expect("read").is_some(),
        "row kept"
    );
    assert!(
        store.status_for("/photos/gone.jpg").expect("read").is_some(),
        "an absent pool never GCs — unmount deletes nothing"
    );

    reset_gate();
}
