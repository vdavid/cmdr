//! Enrichment-core tests (M1 TDD targets): the fake-backend pass over a synthetic
//! index, path-keyed staleness in action, deletion-driven GC (and why it must NOT
//! fire mid-rescan), the edge-triggered `Completed` consumption, and the
//! throttle/cancel decision. No FFI, no registry, no async driver.

use std::collections::{HashMap, HashSet};

use super::enrich::{ImageEntry, enrich_and_gc, gc_targets, load_statuses, walk_image_entries};
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::media_index::backend::fake::FakeVisionBackend;
use crate::media_index::predicate::MediaKind;
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
    let summary = enrich_and_gc(&images, &HashMap::new(), &backend, &writer, &|| false).expect("pass");
    assert_eq!(summary.enriched, 1);

    let index = MediaIndex::open(dir.path(), "root");
    let hits = index.search_ocr("beach", 10).expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "/photos/beach.jpg");
    // A second pass over the loaded statuses re-enriches nothing (path-keyed
    // staleness: same mtime/size/engine ⇒ fresh).
    let statuses = load_statuses(dir.path(), "root");
    let again = enrich_and_gc(&images, &statuses, &backend, &writer, &|| false).expect("second pass");
    assert_eq!(again.enriched, 0, "unchanged images aren't re-enriched");
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
                },
                Some("some text".to_string()),
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
    let summary = enrich_and_gc(&images, &statuses, &backend, &writer, &|| false).expect("pass");
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
            },
            Some("keep me".to_string()),
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
    let summary = enrich_and_gc(&images, &statuses, &backend, &writer, &|| true).expect("pass");
    assert_eq!(summary.enriched, 0);
    assert_eq!(summary.gc_count, 0, "a cancelled pass skips GC");

    let store = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen");
    assert!(
        store.status_for("/survivor.jpg").expect("read").is_some(),
        "a cancelled pass never GCs"
    );
    writer.shutdown();
}
