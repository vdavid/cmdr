//! Enrichment-core tests (TDD targets): the fake-backend pass over a synthetic
//! index, path-keyed staleness in action, deletion-driven GC (and why it must NOT
//! fire mid-rescan), the edge-triggered `Completed` consumption, and the
//! throttle/cancel decision. No FFI, no registry, no async driver.

use std::collections::{HashMap, HashSet};

use super::enrich::{
    EnrichGates, GcScope, ImageEntry, PassHooks, enrich_and_gc, enrich_and_gc_scoped, enrichable_totals, gc_targets,
    load_statuses, parent_dir, prioritized, walk_image_entries, walk_image_entries_in_dirs,
};
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::media_index::backend::fake::FakeVisionBackend;
use crate::media_index::predicate::MediaKind;
use crate::media_index::progress::{EnrichProgress, EnrichProgressSink, NoopProgressSink};
use crate::media_index::read::MediaIndex;
use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};
use crate::media_index::writer::MediaWriter;

/// Build a tiny synthetic index DB at `path` with the given `(parent_dir, file,
/// mtime, size)` files, creating each missing directory. Returns nothing; open the
/// store's read connection to walk it.
fn build_index(path: &std::path::Path, files: &[(&str, &str, u64, u64)]) {
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

    for (dir, name, mtime, size) in files {
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
            Some(*size),
            Some(*size),
            Some(*mtime),
            None,
        )
        .expect("insert file");
    }
}

/// Open a fresh media store + writer for a scratch volume.
fn media_writer(dir: &std::path::Path, volume_id: &str) -> MediaWriter {
    let db_path = media_db_path(dir, volume_id);
    MediaStore::open(&db_path).expect("open media store");
    MediaWriter::spawn(&db_path).expect("media writer")
}

/// A pass-hooks bundle that never cancels and drops progress on the floor — the default
/// for tests that don't assert cancellation or progress.
fn never_cancels() -> bool {
    false
}
fn no_op_hooks() -> PassHooks<'static> {
    PassHooks {
        cancel: &never_cancels,
        progress: &NoopProgressSink,
    }
}

#[test]
fn walk_qualifies_images_only() {
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(
        &index_path,
        &[
            ("/photos", "beach.jpg", 10, 100),
            ("/photos", "notes.txt", 11, 50),
            ("/docs", "report.pdf", 12, 200),
            ("/photos", "raw.cr2", 13, 300),
            ("/photos", "raw.jpg", 14, 120),
        ],
    );
    let store = IndexStore::open(&index_path).expect("reopen");
    let mut images = walk_image_entries(store.read_conn()).expect("walk");
    images.sort_by(|a, b| a.path.cmp(&b.path));
    let paths: Vec<&str> = images.iter().map(|i| i.path.as_str()).collect();
    // beach.jpg + raw.jpg (the RAW defers to its JPEG sibling; txt/pdf skip).
    assert_eq!(paths, vec!["/photos/beach.jpg", "/photos/raw.jpg"]);
    // Staleness key rode along.
    let beach = images.iter().find(|i| i.path == "/photos/beach.jpg").unwrap();
    assert_eq!(beach.mtime, Some(10));
    assert_eq!(beach.size, Some(100));
}

#[test]
fn walk_streams_multiple_dirs_grouped_by_parent() {
    // The streaming walk qualifies each dir's COMPLETE file group independently even though
    // rows arrive ordered by parent_id: RAW+JPEG pairs in two separate dirs each defer the
    // RAW to its sibling JPEG, and a lone RAW in a third dir qualifies. Proves the group
    // boundary is per-dir (sibling-aware), never smeared across the streamed parent groups.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(
        &index_path,
        &[
            ("/a", "one.cr2", 1, 10),
            ("/a", "one.jpg", 2, 20),
            ("/b", "two.cr2", 3, 30),
            ("/b", "two.jpg", 4, 40),
            ("/c", "lone.cr2", 5, 50),
            ("/c", "note.txt", 6, 60),
        ],
    );
    let store = IndexStore::open(&index_path).expect("reopen");
    let mut images = walk_image_entries(store.read_conn()).expect("walk");
    images.sort_by(|a, b| a.path.cmp(&b.path));
    let paths: Vec<&str> = images.iter().map(|i| i.path.as_str()).collect();
    // /a + /b each keep only the JPEG (RAW defers to its sibling); /c's lone RAW qualifies;
    // the txt is skipped.
    assert_eq!(paths, vec!["/a/one.jpg", "/b/two.jpg", "/c/lone.cr2"]);
    // The staleness key rode along per file across the streamed groups.
    let lone = images.iter().find(|i| i.path == "/c/lone.cr2").expect("lone");
    assert_eq!((lone.mtime, lone.size), (Some(5), Some(50)));
}

#[test]
fn enrich_over_fake_backend_populates_ocr_for_images_only() {
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(
        &index_path,
        &[("/photos", "beach.jpg", 10, 100), ("/photos", "notes.txt", 11, 50)],
    );
    let store = IndexStore::open(&index_path).expect("reopen");
    let images = walk_image_entries(store.read_conn()).expect("walk");

    let backend = FakeVisionBackend::new().with_text("/photos/beach.jpg", "a sunny beach with waves");
    let writer = media_writer(dir.path(), "root");
    let summary = enrich_and_gc(
        &images,
        &HashMap::new(),
        &backend,
        &writer,
        &|_| true,
        &|_| false,
        &PassHooks {
            cancel: &|| false,
            progress: &NoopProgressSink,
        },
    )
    .expect("pass");
    assert_eq!(summary.enriched, 1);

    let index = MediaIndex::open(dir.path(), "root");
    let hits = index.search_ocr("beach", 10).expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "/photos/beach.jpg");
    // A second pass over the loaded statuses re-enriches nothing (path-keyed
    // staleness: same mtime/size/engine ⇒ fresh).
    let statuses = load_statuses(dir.path(), "root");
    let again = enrich_and_gc(
        &images,
        &statuses,
        &backend,
        &writer,
        &|_| true,
        &|_| false,
        &no_op_hooks(),
    )
    .expect("second pass");
    assert_eq!(again.enriched, 0, "unchanged images aren't re-enriched");
    writer.shutdown();
}

// ── Importance-prioritized enrichment ──────────────────────────────────

fn image(path: &str) -> ImageEntry {
    ImageEntry {
        path: path.to_string(),
        mtime: Some(1),
        size: Some(2),
        kind: MediaKind::Image,
    }
}

#[test]
fn prioritized_orders_high_importance_folders_first() {
    let images = [image("/low/a.jpg"), image("/high/b.jpg"), image("/mid/c.jpg")];
    let score = |dir: &str| match dir {
        "/high" => 0.9,
        "/mid" => 0.5,
        "/low" => 0.1,
        _ => 0.0,
    };
    let ordered = prioritized(&images, &score);
    let paths: Vec<&str> = ordered.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["/high/b.jpg", "/mid/c.jpg", "/low/a.jpg"]);
}

#[test]
fn enrich_defers_below_threshold_folder_but_keeps_it_for_gc() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "root");
    let backend = FakeVisionBackend::new();

    // Two folders: /keep qualifies (should_enrich true), /skip is below threshold
    // (should_enrich false). Neither is stored yet, so /keep enriches, /skip defers.
    let images = [image("/keep/a.jpg"), image("/skip/b.jpg")];
    let should_enrich = |path: &str| parent_dir(path) == "/keep";
    let summary = enrich_and_gc(
        &images,
        &HashMap::new(),
        &backend,
        &writer,
        &should_enrich,
        &|_| false,
        &PassHooks {
            cancel: &|| false,
            progress: &NoopProgressSink,
        },
    )
    .expect("pass");
    assert_eq!(summary.enriched, 1, "only the qualifying folder enriches");
    assert_eq!(summary.gc_count, 0, "the deferred image is present, so it is NOT GC'd");

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen");
    assert!(
        store.status_for("/keep/a.jpg").expect("read").is_some(),
        "qualifying enriched"
    );
    assert!(
        store.status_for("/skip/b.jpg").expect("read").is_none(),
        "below-threshold deferred, not enriched"
    );
    writer.shutdown();
}

#[test]
fn override_enriches_a_folder_the_threshold_would_defer() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "root");
    let backend = FakeVisionBackend::new();

    // The whole tree is below threshold, but /archive is override-covered ⇒ it
    // enriches while the rest defers (the escape hatch for a rarely-browsed archive).
    let images = [image("/archive/a.jpg"), image("/misc/b.jpg")];
    let overridden = |path: &str| parent_dir(path) == "/archive";
    let summary = enrich_and_gc(
        &images,
        &HashMap::new(),
        &backend,
        &writer,
        &overridden,
        &|_| false,
        &PassHooks {
            cancel: &|| false,
            progress: &NoopProgressSink,
        },
    )
    .expect("pass");
    assert_eq!(summary.enriched, 1);

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen");
    assert!(
        store.status_for("/archive/a.jpg").expect("read").is_some(),
        "override enriches"
    );
    assert!(
        store.status_for("/misc/b.jpg").expect("read").is_none(),
        "the rest defers"
    );
    writer.shutdown();
}

// ── Privacy veto: exclusion beats coverage, live, and closes the TOCTOU ─────

#[test]
fn exclusion_vetoes_even_an_override_covered_image() {
    // The hard privacy veto beats coverage: even an override-covered folder
    // (`should_enrich` true) is skipped when `is_excluded` says so, and no row lands.
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "root");
    let backend = FakeVisionBackend::new();

    let images = [image("/secret/a.jpg")];
    let covered = |_: &str| true; // coverage would enrich it
    let excluded = |_: &str| true; // …but the privacy veto forbids it
    let summary = enrich_and_gc(
        &images,
        &HashMap::new(),
        &backend,
        &writer,
        &covered,
        &excluded,
        &PassHooks {
            cancel: &|| false,
            progress: &NoopProgressSink,
        },
    )
    .expect("pass");
    assert_eq!(
        summary.enriched, 0,
        "an excluded image never enriches, even when covered"
    );

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen");
    assert!(
        store.status_for("/secret/a.jpg").expect("read").is_none(),
        "no row is written for an excluded image"
    );
    writer.shutdown();
}

#[test]
fn exclusion_landing_during_analyze_writes_no_row() {
    // The in-flight-analyze TOCTOU: the image passes the filter `is_excluded` (false),
    // then the exclusion lands DURING the slow `analyze`, so the pre-upsert re-check
    // (the SECOND `is_excluded` call) must return true and skip the upsert. Modeled by
    // a stateful veto that flips false → true across its two calls for the image.
    use std::cell::Cell;
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "root");
    let backend = FakeVisionBackend::new();

    let images = [image("/mid/a.jpg")];
    let calls = Cell::new(0u32);
    // 1st call (filter): not excluded ⇒ proceed into analyze. 2nd call (pre-upsert
    // recheck): the exclude landed ⇒ veto, so the just-analyzed row is dropped.
    let excluded = |_: &str| {
        let n = calls.get();
        calls.set(n + 1);
        n >= 1
    };
    let summary = enrich_and_gc(
        &images,
        &HashMap::new(),
        &backend,
        &writer,
        &|_| true,
        &excluded,
        &PassHooks {
            cancel: &|| false,
            progress: &NoopProgressSink,
        },
    )
    .expect("pass");
    assert_eq!(summary.enriched, 0, "an exclude landing mid-analyze drops the row");
    assert!(
        calls.get() >= 2,
        "the veto is re-checked before the upsert, not only up front"
    );

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen");
    assert!(
        store.status_for("/mid/a.jpg").expect("read").is_none(),
        "nothing is persisted for the mid-analyze exclusion"
    );
    writer.shutdown();
}

// ── GC: deletion-driven, and why it must not fire mid-rescan ────────────────

#[test]
fn gc_targets_is_a_pure_set_difference() {
    let stored = ["/a.jpg".to_string(), "/b.jpg".to_string(), "/gone.jpg".to_string()];
    let current: HashSet<String> = ["/a.jpg".to_string(), "/b.jpg".to_string()].into_iter().collect();
    assert_eq!(gc_targets(stored.iter(), &current), vec!["/gone.jpg".to_string()]);
}

#[test]
fn gc_over_an_empty_index_would_delete_everything_which_is_why_it_gates_on_completed() {
    // A rescan's truncate window transiently empties the tree. If GC ran THEN, this
    // is what it would wrongly target — the whole coverage. The safety comes from
    // running GC only on a `Completed` edge (post-flush, tree whole), never
    // mid-`Scanning`. This test pins the hazard the edge-gate defends against.
    let stored = ["/a.jpg".to_string(), "/b.jpg".to_string()];
    let empty: HashSet<String> = HashSet::new();
    assert_eq!(
        gc_targets(stored.iter(), &empty).len(),
        2,
        "an empty walk would target every row"
    );
}

#[test]
fn a_completed_pass_gcs_a_vanished_known_entry() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "root");

    // Pre-seed two enriched images.
    for path in ["/photos/keep.jpg", "/photos/gone.jpg"] {
        writer
            .upsert(
                MediaStatusRow {
                    path: path.to_string(),
                    mtime: Some(1),
                    size: Some(2),
                    media_kind: MediaKind::Image,
                    state: EnrichmentState::Done,
                    engine_version: "fake-vision-1".to_string(),
                    clip_stamp: String::new(),
                },
                Some(crate::media_index::writer::UpsertAnalysis::ocr_only("some text")),
            )
            .expect("seed");
    }
    writer.flush_blocking().expect("flush");

    // A completed pass where the index now holds only keep.jpg ⇒ gone.jpg is GC'd.
    let images = vec![ImageEntry {
        path: "/photos/keep.jpg".to_string(),
        mtime: Some(1),
        size: Some(2),
        kind: MediaKind::Image,
    }];
    let statuses = load_statuses(dir.path(), "root");
    let backend = FakeVisionBackend::new();
    let summary = enrich_and_gc(
        &images,
        &statuses,
        &backend,
        &writer,
        &|_| true,
        &|_| false,
        &no_op_hooks(),
    )
    .expect("pass");
    assert_eq!(summary.gc_count, 1);

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen");
    assert!(store.status_for("/photos/keep.jpg").expect("read").is_some(), "kept");
    assert!(store.status_for("/photos/gone.jpg").expect("read").is_none(), "GC'd");
    writer.shutdown();
}

#[test]
fn gc_fires_on_a_completed_edge_never_a_retained_poll() {
    // The wiring consumes a `Completed` TRANSITION (`borrow_and_update`), so the
    // `watch`'s retained `Completed` across a new scan's truncate window can't
    // re-trigger a pass (and its GC). Assert exactly ONE edge is observed.
    use crate::indexing::lifecycle_bus::{ScanState, publish_scan_completed, subscribe};
    let vid = "media-edge-test";
    let mut rx = subscribe(vid);
    publish_scan_completed(vid);

    // First consumption sees the edge.
    assert!(rx.has_changed().expect("sender alive"), "a new completion is a change");
    assert!(matches!(*rx.borrow_and_update(), ScanState::Completed { .. }));

    // A poll of the RETAINED value without a new publish reports NO change — so a
    // poll-based consumer would keep seeing `Completed`, but the edge-based one does
    // not re-fire. This is the data-safety property.
    assert!(
        !rx.has_changed().expect("sender alive"),
        "no new publish ⇒ no edge ⇒ the retained Completed does not re-trigger a pass/GC"
    );
}

// ── Throttle / cancel decision ──────────────────────────────────────────────

#[test]
fn a_cancelled_pass_enriches_nothing_and_skips_gc() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "root");
    // Seed a row that a naive GC (empty current set) would delete.
    writer
        .upsert(
            MediaStatusRow {
                path: "/survivor.jpg".to_string(),
                mtime: Some(1),
                size: Some(2),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "fake-vision-1".to_string(),
                clip_stamp: String::new(),
            },
            Some(crate::media_index::writer::UpsertAnalysis::ocr_only("keep me")),
        )
        .expect("seed");
    writer.flush_blocking().expect("flush");

    let images = vec![ImageEntry {
        path: "/photos/new.jpg".to_string(),
        mtime: Some(5),
        size: Some(6),
        kind: MediaKind::Image,
    }];
    let statuses = load_statuses(dir.path(), "root");
    let backend = FakeVisionBackend::new();
    // Cancel returns true immediately ⇒ no enrichment, and GC is skipped (yield
    // fully), so the pre-existing row survives even though it's absent from `images`.
    let summary = enrich_and_gc(
        &images,
        &statuses,
        &backend,
        &writer,
        &|_| true,
        &|_| false,
        &PassHooks {
            cancel: &|| true,
            progress: &NoopProgressSink,
        },
    )
    .expect("pass");
    assert_eq!(summary.enriched, 0);
    assert_eq!(summary.gc_count, 0, "a cancelled pass skips GC");
    assert!(summary.cancelled, "the pass reports it was cancelled");

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen");
    assert!(
        store.status_for("/survivor.jpg").expect("read").is_some(),
        "a cancelled pass never GCs"
    );
    writer.shutdown();
}

#[test]
fn disabling_the_master_toggle_stops_a_running_pass_and_keeps_rows() {
    use crate::media_index::gate;

    // Turning "Index image contents" OFF must stop an IN-FLIGHT pass promptly, not only
    // prevent future passes. Every pass's cancel hook is `gate::should_stop`, which yields
    // when the master toggle flips off (as well as on the watchdog's emergency stop). A
    // stopped pass keeps every already-enriched row — disabling is "stop processing", not
    // "erase": no GC, no prune.
    //
    // The gate is process-global, so serialize with the other gate-touching tests via the
    // shared read-pool lock (the guard `kick_tests` / `reclaim_tests` also hold).
    let _guard = crate::indexing::test_read_pool_lock();
    gate::set_enabled(true); // is_enabled = true, and clears any prior emergency-stop

    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "root");
    // A pre-existing enriched row ABSENT from this pass's walk: a full-pass GC would
    // delete it. A stopped pass must keep it (GC skipped), proving disable never deletes.
    writer
        .upsert(
            MediaStatusRow {
                path: "/survivor.jpg".to_string(),
                mtime: Some(1),
                size: Some(2),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "fake-vision-1".to_string(),
                clip_stamp: String::new(),
            },
            Some(crate::media_index::writer::UpsertAnalysis::ocr_only("keep me")),
        )
        .expect("seed survivor");
    writer.flush_blocking().expect("flush");

    // Two stale images: with the toggle ON both would enrich. The user flips it OFF right
    // before the second, so only the first is processed.
    let images = vec![
        ImageEntry {
            path: "/photos/a.jpg".to_string(),
            mtime: Some(5),
            size: Some(6),
            kind: MediaKind::Image,
        },
        ImageEntry {
            path: "/photos/b.jpg".to_string(),
            mtime: Some(7),
            size: Some(8),
            kind: MediaKind::Image,
        },
    ];
    let statuses = load_statuses(dir.path(), "root");
    let backend = FakeVisionBackend::new();

    // The cancel hook is wired to the production predicate `gate::should_stop`. Between the
    // first and second image, simulate the user turning "Index image contents" OFF.
    let calls = std::cell::Cell::new(0u32);
    let cancel = || {
        if calls.get() == 1 {
            gate::set_enabled(false);
        }
        calls.set(calls.get() + 1);
        gate::should_stop()
    };
    let summary = enrich_and_gc(
        &images,
        &statuses,
        &backend,
        &writer,
        &|_| true,
        &|_| false,
        &PassHooks {
            cancel: &cancel,
            progress: &NoopProgressSink,
        },
    )
    .expect("pass");

    assert_eq!(
        summary.enriched, 1,
        "only the first image enriched before the toggle flipped off"
    );
    assert!(summary.cancelled, "flipping the master toggle off stops the pass");
    assert_eq!(
        summary.gc_count, 0,
        "a stopped pass skips GC — disabling is not a deletion"
    );

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen");
    assert!(
        store.status_for("/photos/a.jpg").expect("read").is_some(),
        "the first image was enriched and kept"
    );
    assert!(
        store.status_for("/photos/b.jpg").expect("read").is_none(),
        "the pass stopped before reaching the second image"
    );
    assert!(
        store.status_for("/survivor.jpg").expect("read").is_some(),
        "already-enriched rows are kept — disabling triggers no GC"
    );

    writer.shutdown();
    // Leave the gate disabled (the default) so other tests start clean.
    gate::set_enabled(false);
}

// ── Progress denominator + vanished-file handling ─────────────────

/// A progress sink that records the last reported snapshot, so a test can assert the
/// enrichable-subset denominator and that `done` reaches `total`.
#[derive(Default)]
struct RecordingSink {
    last: std::sync::Mutex<Option<EnrichProgress>>,
}
impl EnrichProgressSink for RecordingSink {
    fn report(&self, progress: EnrichProgress) {
        *self.last.lock().expect("recording sink lock") = Some(progress);
    }
}

#[test]
fn enrichable_totals_excludes_deferred_and_excluded_images() {
    // Pure denominator: only images passing BOTH gates count; a below-threshold
    // (should_enrich false) and an excluded image are left out, and bytes track the
    // subset (a `None` size counts 0).
    let img = |path: &str, size: Option<u64>| ImageEntry {
        path: path.to_string(),
        mtime: Some(1),
        size,
        kind: MediaKind::Image,
    };
    let images = vec![
        img("/keep/a.jpg", Some(100)),
        img("/keep/b.jpg", None),          // covered but size-unknown ⇒ counts, 0 bytes
        img("/skip/c.jpg", Some(999)),     // below threshold ⇒ not in the subset
        img("/keep/secret.jpg", Some(50)), // excluded ⇒ not in the subset
    ];
    let should_enrich = |p: &str| parent_dir(p) == "/keep";
    let is_excluded = |p: &str| p == "/keep/secret.jpg";
    let (total, bytes_total) = enrichable_totals(&images, &should_enrich, &is_excluded);
    assert_eq!(total, 2, "only the two covered, non-excluded images count");
    assert_eq!(bytes_total, 100, "bytes sum the subset; a None size counts 0");
}

#[test]
fn a_vanished_image_still_completes_the_pass_at_done_equals_total() {
    // The enrichable subset is /keep/a.jpg + /keep/gone.jpg (both covered); /skip/b.jpg
    // is below threshold, so it's NOT in `total`. /keep/gone.jpg reads ENOENT (vanished),
    // so it writes NO row but STILL counts as processed — the bar reaches done == total,
    // never the never-finishes bug.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(
        &index_path,
        &[
            ("/keep", "a.jpg", 10, 100),
            ("/skip", "b.jpg", 11, 999),
            ("/keep", "gone.jpg", 12, 50),
        ],
    );
    let store = IndexStore::open(&index_path).expect("reopen");
    let images = walk_image_entries(store.read_conn()).expect("walk");
    let statuses = load_statuses(dir.path(), "root");
    let writer = media_writer(dir.path(), "root");
    // The vanished source: analyze returns `VisionError::Missing`.
    let backend = FakeVisionBackend::new().missing_for("/keep/gone.jpg");

    let should_enrich = |p: &str| parent_dir(p) == "/keep";
    let sink = RecordingSink::default();
    let summary = enrich_and_gc(
        &images,
        &statuses,
        &backend,
        &writer,
        &should_enrich,
        &|_| false,
        &PassHooks {
            cancel: &never_cancels,
            progress: &sink,
        },
    )
    .expect("pass");

    // /keep/a.jpg enriched; /keep/gone.jpg vanished (skipped, no row). Both counted.
    assert_eq!(summary.enriched, 1, "only the readable covered image enriches");
    let last = sink.last.lock().expect("sink").expect("a progress tick");
    assert_eq!(
        last.total, 2,
        "the below-threshold /skip image is NOT in the denominator"
    );
    assert_eq!(
        last.done, 2,
        "the vanished image still completes the pass (done == total)"
    );
    assert_eq!(last.bytes_total, 150, "bytes denominator is the covered subset only");

    let media = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen media");
    assert!(
        media.status_for("/keep/a.jpg").expect("read").is_some(),
        "the readable image has a row"
    );
    assert!(
        media.status_for("/keep/gone.jpg").expect("read").is_none(),
        "a vanished image writes NO row (not Failed); GC collects any stale one"
    );
    assert!(
        media.status_for("/skip/b.jpg").expect("read").is_none(),
        "a deferred image writes no row"
    );
    writer.shutdown();
}

// ── Live enrichment: the scoped walk + scoped GC (data-safety) ──

/// Seed a Done `media_status` row (with OCR text) for `path`.
fn seed_media(writer: &MediaWriter, path: &str) {
    writer
        .upsert(
            MediaStatusRow {
                path: path.to_string(),
                mtime: Some(1),
                size: Some(2),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "fake-vision-1".to_string(),
                clip_stamp: String::new(),
            },
            Some(crate::media_index::writer::UpsertAnalysis::ocr_only("seed")),
        )
        .expect("seed row");
}

fn touched(dirs: &[&str]) -> HashSet<String> {
    dirs.iter().map(|d| d.to_string()).collect()
}

#[test]
fn walk_in_dirs_touches_only_the_given_dirs() {
    // The scoped walk visits ONLY the touched dirs — never the whole index (the per-tick
    // cost bound). Sibling-aware per dir, carrying the staleness key.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(
        &index_path,
        &[("/a", "x.jpg", 1, 10), ("/b", "y.jpg", 2, 20), ("/c", "z.jpg", 3, 30)],
    );
    let store = IndexStore::open(&index_path).expect("reopen");
    let mut images = walk_image_entries_in_dirs(store.read_conn(), &touched(&["/a", "/b"])).expect("walk");
    images.sort_by(|a, b| a.path.cmp(&b.path));
    let paths: Vec<&str> = images.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["/a/x.jpg", "/b/y.jpg"], "only /a + /b walked; /c untouched");
    let x = images.iter().find(|i| i.path == "/a/x.jpg").expect("x");
    assert_eq!((x.mtime, x.size), (Some(1), Some(10)), "the staleness key rode along");
}

#[test]
fn walk_in_dirs_skips_a_dir_absent_from_the_index() {
    // A touched dir that's since vanished from the index resolves to `None` ⇒ skipped
    // quietly (no error). Its stored rows fall to the scoped GC instead.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/a", "x.jpg", 1, 10)]);
    let store = IndexStore::open(&index_path).expect("reopen");
    let images = walk_image_entries_in_dirs(store.read_conn(), &touched(&["/gone"])).expect("walk");
    assert!(images.is_empty(), "a dir absent from the index yields no images");
}

#[test]
fn scoped_gc_spares_rows_in_untouched_dirs() {
    // The scoped-GC data-safety regression: a live tick walks only /a, so its GC deletes a
    // vanished row UNDER /a but must NEVER touch the untouched-dir /b row — even though /b's
    // row is absent from the scoped walk. A whole-store GC over the scoped walk would delete
    // /b/survivor.jpg (see the trap test below).
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    // The index holds /a/keep.jpg + /b/survivor.jpg now; /a/gone.jpg vanished.
    build_index(&index_path, &[("/a", "keep.jpg", 1, 10), ("/b", "survivor.jpg", 2, 20)]);
    let store = IndexStore::open(&index_path).expect("reopen");
    let touched_a = touched(&["/a"]);
    let images = walk_image_entries_in_dirs(store.read_conn(), &touched_a).expect("walk");

    let writer = media_writer(dir.path(), "root");
    for path in ["/a/keep.jpg", "/a/gone.jpg", "/b/survivor.jpg"] {
        seed_media(&writer, path);
    }
    writer.flush_blocking().expect("flush");
    let statuses = load_statuses(dir.path(), "root");
    let backend = FakeVisionBackend::new();

    let summary = enrich_and_gc_scoped(
        &images,
        &statuses,
        &backend,
        &writer,
        &EnrichGates {
            should_enrich: &|_| true,
            is_excluded: &|_| false,
            gc_scope: GcScope::TouchedDirs(&touched_a),
            clip_stamp: None,
        },
        &no_op_hooks(),
    )
    .expect("pass");
    assert_eq!(summary.gc_count, 1, "only the vanished /a/gone.jpg is GC'd");

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen media");
    assert!(
        store.status_for("/a/keep.jpg").expect("read").is_some(),
        "present /a row kept"
    );
    assert!(
        store.status_for("/a/gone.jpg").expect("read").is_none(),
        "vanished /a row GC'd"
    );
    assert!(
        store.status_for("/b/survivor.jpg").expect("read").is_some(),
        "the untouched /b row SURVIVES — the scoped-GC data-safety line"
    );
    writer.shutdown();
}

#[test]
fn whole_store_gc_over_a_scoped_walk_would_wipe_untouched_dirs() {
    // The hazard scoped GC defends against (cf. `gc_over_an_empty_index_would_delete_...`):
    // running the full-pass WholeStore GC against a SCOPED walk targets every stored row
    // outside the touched dirs. This is exactly why a live tick uses `TouchedDirs`, never
    // `WholeStore`. Pins the RED against a naive whole-store GC.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/a", "keep.jpg", 1, 10), ("/b", "survivor.jpg", 2, 20)]);
    let store = IndexStore::open(&index_path).expect("reopen");
    let touched_a = touched(&["/a"]);
    let images = walk_image_entries_in_dirs(store.read_conn(), &touched_a).expect("walk");

    let writer = media_writer(dir.path(), "root");
    for path in ["/a/keep.jpg", "/b/survivor.jpg"] {
        seed_media(&writer, path);
    }
    writer.flush_blocking().expect("flush");
    let statuses = load_statuses(dir.path(), "root");
    let backend = FakeVisionBackend::new();

    // The whole-store entry point (`enrich_and_gc`) over a SCOPED walk — the trap.
    let summary = enrich_and_gc(
        &images,
        &statuses,
        &backend,
        &writer,
        &|_| true,
        &|_| false,
        &no_op_hooks(),
    )
    .expect("pass");
    assert_eq!(
        summary.gc_count, 1,
        "WholeStore over a scoped walk wrongly targets the untouched /b row"
    );
    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen media");
    assert!(
        store.status_for("/b/survivor.jpg").expect("read").is_none(),
        "the trap deletes the untouched row — which scoped GC prevents"
    );
    writer.shutdown();
}

#[test]
fn a_scoped_tick_promotes_a_lone_raw_and_gcs_the_deleted_jpg() {
    // The sibling re-qualify edge: /photos held raw.cr2 + raw.jpg (the RAW deferred to its
    // cheaper JPEG sibling). The JPEG is deleted, so /photos now holds only raw.cr2 — a LONE
    // RAW that NOW qualifies. Because the scoped walk fetches the COMPLETE dir (not just the
    // changed file), the tick enriches the promoted raw.cr2 AND scoped-GCs the vanished
    // raw.jpg row — the whole-dir fetch is what makes this correct.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/photos", "raw.cr2", 1, 10)]); // raw.jpg gone
    let store = IndexStore::open(&index_path).expect("reopen");
    let touched_photos = touched(&["/photos"]);
    let images = walk_image_entries_in_dirs(store.read_conn(), &touched_photos).expect("walk");
    assert_eq!(
        images.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(),
        vec!["/photos/raw.cr2"],
        "the lone RAW now qualifies"
    );

    let writer = media_writer(dir.path(), "root");
    seed_media(&writer, "/photos/raw.jpg"); // the old JPEG's stored row
    writer.flush_blocking().expect("flush");
    let statuses = load_statuses(dir.path(), "root");
    let backend = FakeVisionBackend::new();

    let summary = enrich_and_gc_scoped(
        &images,
        &statuses,
        &backend,
        &writer,
        &EnrichGates {
            should_enrich: &|_| true,
            is_excluded: &|_| false,
            gc_scope: GcScope::TouchedDirs(&touched_photos),
            clip_stamp: None,
        },
        &no_op_hooks(),
    )
    .expect("pass");
    assert_eq!(summary.enriched, 1, "the promoted lone RAW enriches");
    assert_eq!(summary.gc_count, 1, "the deleted JPEG's row GCs");

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen media");
    assert!(
        store.status_for("/photos/raw.cr2").expect("read").is_some(),
        "lone RAW enriched"
    );
    assert!(
        store.status_for("/photos/raw.jpg").expect("read").is_none(),
        "deleted JPEG GC'd"
    );
    writer.shutdown();
}

// ── CLIP two-part staleness (plan M3) ──────────────────────────────────────

/// The number of `media_clip_embedding` rows for a path (0 or 1), and the stored
/// `media_status.clip_stamp`.
fn clip_state(db_path: &std::path::Path, path: &str) -> (i64, String) {
    let conn = crate::media_index::store::open_read_connection(db_path).expect("open read");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM media_clip_embedding WHERE path = ?1",
            rusqlite::params![path],
            |r| r.get(0),
        )
        .expect("count clip");
    let stamp: String = conn
        .query_row(
            "SELECT clip_stamp FROM media_status WHERE path = ?1",
            rusqlite::params![path],
            |r| r.get(0),
        )
        .unwrap_or_default();
    (count, stamp)
}

/// One whole-store pass over `images` with the given CLIP stamp, over `backend`.
fn clip_pass(
    images: &[ImageEntry],
    statuses: &HashMap<String, MediaStatusRow>,
    backend: &FakeVisionBackend,
    writer: &MediaWriter,
    clip_stamp: Option<&str>,
) -> super::enrich::PassSummary {
    enrich_and_gc_scoped(
        images,
        statuses,
        backend,
        writer,
        &EnrichGates {
            should_enrich: &|_| true,
            is_excluded: &|_| false,
            gc_scope: GcScope::WholeStore,
            clip_stamp,
        },
        &no_op_hooks(),
    )
    .expect("clip pass")
}

#[test]
fn model_absent_is_vision_only_no_clip_embedding() {
    // With NO CLIP model installed (clip_stamp None), a pass embeds Vision only — the
    // CLIP table stays empty and the row's clip_stamp stays blank.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/p", "beach.jpg", 1, 10)]);
    let store = IndexStore::open(&index_path).expect("reopen");
    let images = walk_image_entries(store.read_conn()).expect("walk");
    let backend = FakeVisionBackend::new();
    let writer = media_writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");

    let summary = clip_pass(&images, &HashMap::new(), &backend, &writer, None);
    writer.flush_blocking().expect("flush");
    assert_eq!(summary.enriched, 1, "Vision enriched the image");
    let (clip_rows, stamp) = clip_state(&db_path, "/p/beach.jpg");
    assert_eq!(clip_rows, 0, "no CLIP model ⇒ no CLIP embedding");
    assert_eq!(stamp, "", "clip_stamp stays blank without a model");
    writer.shutdown();
}

#[test]
fn installing_clip_embeds_without_re_running_vision() {
    // The data-safety-relevant case (plan M3 Q5): a Vision-enriched image, then CLIP is
    // installed. The next pass must embed CLIP WITHOUT re-running OCR/tags — proven by
    // scripting a DIFFERENT OCR text on the second backend and asserting the stored OCR
    // text is unchanged (Vision was not re-run), while a CLIP embedding now exists.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/p", "beach.jpg", 1, 10)]);
    let store = IndexStore::open(&index_path).expect("reopen");
    let images = walk_image_entries(store.read_conn()).expect("walk");
    let writer = media_writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");

    // Pass 1: Vision only (no model). OCR text = "original beach".
    let backend1 = FakeVisionBackend::new().with_text("/p/beach.jpg", "original beach");
    clip_pass(&images, &HashMap::new(), &backend1, &writer, None);
    writer.flush_blocking().expect("flush");

    // Pass 2: CLIP now installed (clip_stamp Some), and a backend that WOULD produce
    // different OCR if Vision re-ran. Vision is current (same mtime/size/engine), so only
    // CLIP should run.
    let backend2 = FakeVisionBackend::new().with_text("/p/beach.jpg", "CHANGED text");
    let statuses = load_statuses(dir.path(), "root");
    let summary = clip_pass(&images, &statuses, &backend2, &writer, Some("clip-v1"));
    writer.flush_blocking().expect("flush");
    assert_eq!(summary.enriched, 1, "the CLIP-only work counts as enriched");

    // CLIP embedding now present; clip_stamp stamped.
    let (clip_rows, stamp) = clip_state(&db_path, "/p/beach.jpg");
    assert_eq!(clip_rows, 1, "CLIP embedding written");
    assert_eq!(stamp, "clip-v1", "clip_stamp stamped to the installed model");

    // Vision was NOT re-run: the OCR text is still the original, not "CHANGED text".
    let index = MediaIndex::open(dir.path(), "root");
    assert_eq!(
        index.search_ocr("original", 10).expect("s").len(),
        1,
        "original OCR intact"
    );
    assert_eq!(
        index.search_ocr("CHANGED", 10).expect("s").len(),
        0,
        "Vision was not re-run (no CHANGED text)"
    );

    // Pass 3: nothing stale now (Vision current, CLIP current) ⇒ no work.
    let statuses = load_statuses(dir.path(), "root");
    let again = clip_pass(&images, &statuses, &backend2, &writer, Some("clip-v1"));
    assert_eq!(again.enriched, 0, "both sides current ⇒ nothing re-enriched");
    writer.shutdown();
}

#[test]
fn a_clip_model_bump_re_embeds_without_touching_vision() {
    // A CLIP model/OS change bumps clip_stamp, so a fresh CLIP embedding is written while
    // the Vision side (unchanged engine) is left alone.
    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[("/p", "beach.jpg", 1, 10)]);
    let store = IndexStore::open(&index_path).expect("reopen");
    let images = walk_image_entries(store.read_conn()).expect("walk");
    let backend = FakeVisionBackend::new();
    let writer = media_writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");

    clip_pass(&images, &HashMap::new(), &backend, &writer, Some("clip-v1"));
    writer.flush_blocking().expect("flush");
    assert_eq!(clip_state(&db_path, "/p/beach.jpg").1, "clip-v1");

    // Bump the CLIP stamp ⇒ re-embed.
    let statuses = load_statuses(dir.path(), "root");
    let summary = clip_pass(&images, &statuses, &backend, &writer, Some("clip-v2"));
    writer.flush_blocking().expect("flush");
    assert_eq!(summary.enriched, 1, "the model bump re-embeds CLIP");
    assert_eq!(clip_state(&db_path, "/p/beach.jpg"), (1, "clip-v2".to_string()));
    writer.shutdown();
}
