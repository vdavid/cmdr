//! Symlink rows never reach enrichment. The drive index records a symlink as a
//! non-folder row, and the privacy exclusion judges the LINK's path, so a link like
//! `scan.jpg -> ~/Documents/IDs/passport.jpg` would otherwise store the excluded
//! target's OCR text under an included path (#532). Both walks drop symlink rows
//! before qualification, and a completed pass's GC collects any row stored earlier.

use super::enrich::{enrich_and_gc, load_statuses, walk_image_entries, walk_image_entries_in_dirs};
use super::enrich_tests::{build_index, media_writer, no_op_hooks, seed_media, touched};
use crate::indexing::store::{IndexStore, resolve_path};
use crate::media_index::backend::fake::FakeVisionBackend;
use crate::media_index::coverage::count_qualifying_images;
use crate::media_index::store::{MediaStore, media_db_path};

/// Add a symlink row named `name` under the already-indexed `dir`, the shape the
/// scanner writes for a link: a non-folder row with `is_symlink` set.
fn add_symlink_row(store: &IndexStore, dir: &str, name: &str) {
    let conn = store.read_conn();
    let parent_id = resolve_path(conn, dir).expect("resolve").expect("dir indexed");
    let next_id: i64 = conn
        .query_row("SELECT MAX(id) + 1 FROM entries", [], |r| r.get(0))
        .expect("next id");
    IndexStore::insert_entry_v2_with_id(
        conn,
        next_id,
        parent_id,
        name,
        false,
        true,
        Some(7),
        Some(7),
        Some(7),
        None,
    )
    .expect("insert symlink");
}

/// An index holding a real `/photos/real.jpg` and a symlink row `/photos/a.jpg` (in
/// production it points into an excluded folder; the index never records the target).
fn index_with_symlink(dir: &std::path::Path) -> IndexStore {
    let index_path = dir.join("index-root.db");
    build_index(&index_path, &[("/photos", "real.jpg", 1, 10)]);
    let store = IndexStore::open(&index_path).expect("reopen");
    add_symlink_row(&store, "/photos", "a.jpg");
    store
}

#[test]
fn the_full_walk_and_its_counts_skip_a_symlink_row() {
    let dir = tempfile::tempdir().expect("temp");
    let store = index_with_symlink(dir.path());

    let images = walk_image_entries(store.read_conn()).expect("walk");
    let paths: Vec<&str> = images.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["/photos/real.jpg"], "the symlink row never qualifies");

    let counts = count_qualifying_images(store.read_conn()).expect("count");
    assert_eq!(counts.total, 1, "the counts walk agrees with the enrich walk");
}

#[test]
fn the_scoped_walk_skips_a_symlink_row() {
    let dir = tempfile::tempdir().expect("temp");
    let store = index_with_symlink(dir.path());

    let (_walked, images) = walk_image_entries_in_dirs(store.read_conn(), &touched(&["/photos"])).expect("walk");
    let paths: Vec<&str> = images.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["/photos/real.jpg"],
        "the live tick never qualifies a symlink row"
    );
}

#[test]
fn a_pass_writes_no_row_for_a_symlink_and_gcs_one_stored_earlier() {
    // A row stored before the fix (the link's path, the excluded target's text) must not
    // survive the next completed pass: the walk no longer yields it, so the whole-store GC
    // sees it as absent and deletes it.
    let dir = tempfile::tempdir().expect("temp");
    let store = index_with_symlink(dir.path());
    let writer = media_writer(dir.path(), "root");
    seed_media(&writer, "/photos/a.jpg");
    writer.flush_blocking().expect("flush");

    let images = walk_image_entries(store.read_conn()).expect("walk");
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
    assert_eq!(summary.enriched, 1, "only the real file reaches the backend");
    assert_eq!(summary.gc_count, 1, "the stored symlink row is collected");

    let media = MediaStore::open(&media_db_path(dir.path(), "root")).expect("reopen");
    assert!(media.status_for("/photos/real.jpg").expect("read").is_some());
    assert!(media.status_for("/photos/a.jpg").expect("read").is_none());
    writer.shutdown();
}
