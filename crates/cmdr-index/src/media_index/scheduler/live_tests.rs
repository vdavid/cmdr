//! Live-tick tests: the scoped follow-the-index tick (`live.rs`) end to end, over a
//! registered root read pool.
//!
//! Two layers, in the order a tick runs them:
//!
//! - **The coverage filter**, pure: the property that a dir the filter drops holds no
//!   image the per-image gate would have enriched. Everything below rests on it — the
//!   filtered set is what the walk, the scoped GC, and the counts patch all agree on.
//! - **The tick itself**: re-enriching a modified image without a completed scan,
//!   deferring what coverage doesn't reach, the privacy veto, the index-confirmed GC, and
//!   the two data-safety anchors for the filter (rows and cached counts in a dropped dir
//!   both survive).
//!
//! These drive the process-global read pool + master-toggle gate, so each holds
//! `crate::test_read_pool_lock()` to serialize against parallel tests and resets the gate
//! itself. Fixtures are shared with `kick_tests` (the same shapes).

use std::collections::{HashMap, HashSet};

use super::kick_tests::{
    build_index, config_with, fake_backend, reset_gate, seed_importance_full_pass, seed_media_row, use_automatic_scope,
};
use super::*;
use crate::media_index::network::config::NetworkEnrichConfig;
use crate::media_index::predicate::MediaKind;
use crate::media_index::store::{MediaStore, media_db_path};

const ROOT: &str = "root";

// ── Pure: the directory filter never loses to the per-image gate ────────────

proptest::proptest! {
    /// The one property a live tick's directory filter has to have: whatever the
    /// overrides, the scores, and the paths turn out to be, an image the per-image gate
    /// would enrich lives in a directory the filter keeps.
    ///
    /// It matters far beyond the enriching: a tick hands the SAME filtered set to its
    /// walk and to `GcScope::TouchedDirs`, so a directory the filter drops is a
    /// directory whose stored rows are never GC candidates. If the filter could ever
    /// drop a directory the gate would have enriched, the two would disagree about
    /// which directories a tick covers, which is where rows get deleted for being
    /// "in scope but not walked".
    #[test]
    fn a_dir_the_filter_drops_holds_no_image_the_gate_would_enrich(
        dir in proptest::sample::select(vec!["/", "/a", "/a/b", "/a/b/c", "/ab"]),
        name in proptest::sample::select(vec!["p.jpg", "b", "c"]),
        always in proptest::collection::vec(
            proptest::sample::select(vec!["/", "/a", "/a/b", "/a/b/c", "/a/b/c.jpg", "/ab"]),
            0..3,
        ),
        scored in proptest::collection::vec(proptest::sample::select(vec!["/", "/a", "/a/b", "/ab"]), 0..3),
        volume_override in proptest::bool::ANY,
        unscored in proptest::bool::ANY,
    ) {
        let config = NetworkEnrichConfig {
            opted_in_volumes: Default::default(),
            always_index_volumes: if volume_override { [ROOT.to_string()].into_iter().collect() } else { Default::default() },
            always_index_folders: always.iter().map(|s| s.to_string()).collect(),
            excluded_folders: Default::default(),
        };
        let scores: HashMap<String, f64> = scored.iter().map(|s| (s.to_string(), 0.9)).collect();
        let scores = if unscored { None } else { Some(&scores) };
        let path = if dir == "/" { format!("/{name}") } else { format!("{dir}/{name}") };
        if local_should_enrich(&path, scores, &config, ROOT) {
            proptest::prop_assert!(
                lifecycle::local_dir_may_be_covered(dir, scores, &config, ROOT),
                "the gate enriches {path} but the filter drops {dir}"
            );
        }
    }
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
    let _guard = crate::test_read_pool_lock();
    reset_gate();
    // Importance-driven coverage, so ask for the automatic scope (the default indexes
    // only the user's chosen folders).
    use_automatic_scope();
    gate::set_enabled(true);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/keep", "a.jpg")]);
    crate::test_install_root_read_pool(index_path).expect("install pool");
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

    crate::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn a_live_tick_defers_a_below_threshold_folder() {
    // A folder below the slider threshold defers on a live tick, exactly like the full pass:
    // /skip has no score at or above 0.5, so it's absent from the threshold-filtered map and
    // never enriches.
    let _guard = crate::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);
    gate::set_importance_threshold(0.5);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/skip", "a.jpg")]);
    crate::test_install_root_read_pool(index_path).expect("install pool");
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

    crate::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn a_live_tick_never_enriches_an_excluded_folder() {
    // The privacy veto holds on a live tick: an excluded folder never enriches, even when
    // importance covers it. The automatic scope is what makes the seeded score cover
    // /secret, so the veto is the ONLY thing stopping the tick here.
    let _guard = crate::test_read_pool_lock();
    reset_gate();
    use_automatic_scope();
    gate::set_enabled(true);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/secret", "a.jpg")]);
    crate::test_install_root_read_pool(index_path).expect("install pool");
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

    crate::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn a_live_tick_gcs_an_index_confirmed_removal() {
    // An index-confirmed removal is a fact about the tree (not a scan-state inference), so a
    // live tick may delete its row: the index now holds only keep.jpg, so gone.jpg's stored
    // row is scoped-GC'd — while keep.jpg (present) survives.
    let _guard = crate::test_read_pool_lock();
    reset_gate();
    // Importance-driven coverage, matching the seeded scores below: a tick only walks —
    // and so only GCs within — the dirs its coverage filter keeps, so /photos has to be
    // a covered dir for this to be about GC at all.
    use_automatic_scope();
    gate::set_enabled(true);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/photos", "keep.jpg")]); // gone.jpg removed from the index
    crate::test_install_root_read_pool(index_path).expect("install pool");
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

    crate::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn a_live_tick_keeps_every_row_in_a_dir_its_coverage_filter_dropped() {
    // ❗ THE scoped-GC data-safety anchor. A tick filters its touched dirs by coverage
    // before walking, and hands the SAME filtered set to the walk, to
    // `GcScope::TouchedDirs`, and to the counts patch. Filter the WALK ALONE and every
    // stored row under a dropped dir becomes "in scope, absent from the walk, therefore
    // deleted" — every OCR text, Vision tag, and CLIP embedding in it, against
    // `media_index/CLAUDE.md`'s "uncovered rows STAY".
    //
    // `/uncovered/kept.jpg` is the row that dies if the two sets ever come apart: its
    // file is still right there in the index, and only a GC scope wider than the walk
    // can reach it. `/covered/gone.jpg` is the control — filtering must not cost the
    // tick the GC it's for.
    let _guard = crate::test_read_pool_lock();
    reset_gate();
    use_automatic_scope();
    gate::set_enabled(true);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    // Both files are PRESENT in the index; `gone.jpg` never was.
    build_index(&index_path, &[("/covered", "a.jpg"), ("/uncovered", "kept.jpg")]);
    crate::test_install_root_read_pool(index_path).expect("install pool");
    // Only /covered is scored, so the filter drops /uncovered.
    seed_importance_full_pass(dir.path(), &[("/covered", 0.9)]);
    seed_media_row(dir.path(), "/uncovered/kept.jpg");
    seed_media_row(dir.path(), "/covered/gone.jpg");

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    sched
        .run_live_tick_blocking(ROOT, &touched(&["/covered", "/uncovered"]))
        .expect("tick");

    let store = MediaStore::open(&media_db_path(dir.path(), ROOT)).expect("open");
    assert!(
        store.status_for("/uncovered/kept.jpg").expect("read").is_some(),
        "a row in a filtered-out dir is NEVER a GC candidate — its dir was never walked"
    );
    assert!(
        store.status_for("/covered/gone.jpg").expect("read").is_none(),
        "and the dirs that DID survive the filter still get their vanished rows collected"
    );

    crate::test_uninstall_root_read_pool();
    reset_gate();
}

#[test]
fn a_live_tick_leaves_the_cached_counts_of_a_dir_it_filtered_out_alone() {
    // The third consumer of the filtered set, and the third way to get it wrong: the
    // eligible-counts patch replaces each dir it's handed with a fresh count taken from
    // the walk. Hand it the UNFILTERED dirs against a filtered walk and every dropped
    // dir's count is replaced by zero — the coverage badge silently loses images that
    // are still on disk.
    let _guard = crate::test_read_pool_lock();
    reset_gate();
    use_automatic_scope();
    gate::set_enabled(true);
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/covered", "a.jpg"), ("/uncovered", "b.jpg")]);
    crate::test_install_root_read_pool(index_path).expect("install pool");
    seed_importance_full_pass(dir.path(), &[("/covered", 0.9)]);
    // A warm counts cache, as a completed pass would leave it.
    crate::media_index::coverage::replace_from_entries(
        ROOT,
        &[image_entry("/covered/a.jpg"), image_entry("/uncovered/b.jpg")],
    );

    let sched = MediaScheduler::new(dir.path().to_path_buf(), fake_backend());
    // /covered/a.jpg enriches, so the patch runs (it only fires on a tick that changed
    // something).
    assert_eq!(
        sched
            .run_live_tick_blocking(ROOT, &touched(&["/covered", "/uncovered"]))
            .expect("tick"),
        1
    );

    let counts = crate::media_index::coverage::cached(ROOT).expect("counts stayed cached");
    assert_eq!(
        counts.per_folder.get("/uncovered"),
        Some(&1),
        "a filtered-out dir keeps the count the last full walk gave it"
    );
    assert_eq!(
        counts.per_folder.get("/covered"),
        Some(&1),
        "and the walked dir is fresh"
    );
    assert_eq!(counts.total, 2, "so the volume total doesn't lose the dropped dir");

    crate::media_index::coverage::invalidate(ROOT);
    crate::test_uninstall_root_read_pool();
    reset_gate();
}

/// A present, unenriched image entry at `path`, for seeding the counts cache the way a
/// completed pass's walk would.
fn image_entry(path: &str) -> enrich::ImageEntry {
    enrich::ImageEntry {
        path: path.to_string(),
        mtime: Some(1),
        size: Some(1),
        kind: MediaKind::Image,
    }
}

#[test]
fn an_unmounted_volume_live_tick_deletes_nothing() {
    // Unmount safety: with no read pool (the volume is gone), a live tick no-ops entirely —
    // it never GCs, so a disconnect can't wipe a volume's coverage.
    let _guard = crate::test_read_pool_lock();
    reset_gate();
    gate::set_enabled(true);
    crate::test_uninstall_root_read_pool(); // ensure no pool is installed
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
