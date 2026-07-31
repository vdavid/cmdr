use super::*;
use crate::media_index::backend::Tag;
use crate::media_index::predicate::MediaKind;
use crate::media_index::store::{MediaStore, media_db_path, open_read_connection};

/// A fresh media store + writer over a scratch volume.
fn writer(dir: &Path, volume_id: &str) -> MediaWriter {
    let db_path = media_db_path(dir, volume_id);
    MediaStore::open(&db_path).expect("open media store");
    MediaWriter::spawn(&db_path, volume_id).expect("media writer")
}

/// Seed one fully-enriched image (a row in ALL FOUR tables), so a prune test can
/// assert every kind of row goes.
fn seed(writer: &MediaWriter, path: &str) {
    writer
        .upsert(
            MediaStatusRow {
                path: path.to_string(),
                mtime: Some(1),
                size: Some(2),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "e1".to_string(),
                clip_stamp: String::new(),
            },
            Some(UpsertAnalysis {
                ocr_text: "some text".to_string(),
                tags: vec![Tag {
                    label: "beach".to_string(),
                    score: 0.5,
                }],
                embedding: Some(vec![1.0, 0.0, 0.0]),
            }),
        )
        .expect("seed");
}

/// Count the rows for `path` across all four tables (status, ocr, tags, embedding),
/// so a deletion assertion covers every table, not just `media_status`. A fully
/// `seed`ed image has TWO `media_ocr` rows (the OCR text + the folded tag labels),
/// so a kept image reads `(1, 2, 1, 1)`.
fn row_counts(db_path: &Path, path: &str) -> (i64, i64, i64, i64) {
    let conn = open_read_connection(db_path).expect("open read");
    let count = |sql: &str| -> i64 {
        conn.query_row(sql, rusqlite::params![path], |r| r.get(0))
            .expect("count")
    };
    // Each child table keys on `file_id` now (plan M4), so count via a join on
    // `media_file`. A fully-`seed`ed image reads `(1, 2, 1, 1)`; a deleted one `(0,…)`.
    (
        count("SELECT COUNT(*) FROM media_status s JOIN media_file f ON f.id = s.file_id WHERE f.path = ?1"),
        count("SELECT COUNT(*) FROM media_ocr o JOIN media_file f ON f.id = o.file_id WHERE f.path = ?1"),
        count("SELECT COUNT(*) FROM media_tags t JOIN media_file f ON f.id = t.file_id WHERE f.path = ?1"),
        count("SELECT COUNT(*) FROM media_embedding e JOIN media_file f ON f.id = e.file_id WHERE f.path = ?1"),
    )
}

/// The on-disk size of the DB's `-wal` sidecar, or 0 if it's absent.
fn wal_len(db_path: &Path) -> u64 {
    std::fs::metadata(db_path.with_extension("db-wal"))
        .map(|m| m.len())
        .unwrap_or(0)
}

#[test]
fn checkpoint_truncates_the_wal_at_rest() {
    let dir = tempfile::tempdir().expect("temp");
    let w = writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");

    // Committed enrichment upserts leave frames in the WAL; passive autocheckpoint
    // never truncates the file, so it sits non-empty on disk.
    for i in 0..200 {
        seed(&w, &format!("/photos/{i}.jpg"));
    }
    w.flush_blocking().expect("flush");
    assert!(wal_len(&db_path) > 0, "the WAL holds frames before the checkpoint");

    // The checkpoint hook truncates it to zero (no reader is blocking).
    w.checkpoint_wal().expect("checkpoint");
    assert_eq!(wal_len(&db_path), 0, "the checkpoint truncated the WAL to zero at rest");

    w.shutdown();
}

#[test]
fn checkpoint_tolerates_a_blocking_reader_without_erroring() {
    let dir = tempfile::tempdir().expect("temp");
    let w = writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");
    seed(&w, "/photos/a.jpg");
    w.flush_blocking().expect("flush");

    // Pin an old read snapshot so a later TRUNCATE can't reclaim the frames past it.
    let reader = open_read_connection(&db_path).expect("reader");
    reader.execute_batch("BEGIN").expect("begin read txn");
    let _pinned: i64 = reader
        .query_row("SELECT COUNT(*) FROM media_status", [], |r| r.get(0))
        .expect("pin snapshot");

    // Advance the WAL past the reader's snapshot, then checkpoint. The truncate is
    // blocked, but the hook must NOT surface an error (it degrades to PASSIVE).
    seed(&w, "/photos/b.jpg");
    w.flush_blocking().expect("flush");
    w.checkpoint_wal()
        .expect("checkpoint tolerates the reader without erroring");

    reader.execute_batch("END").ok();

    // The writer keeps working after a blocked checkpoint (the pass path is intact).
    seed(&w, "/photos/c.jpg");
    w.flush_blocking().expect("flush");
    w.shutdown();
}

#[test]
fn prune_under_folder_deletes_rows_at_or_under_and_only_those() {
    let dir = tempfile::tempdir().expect("temp");
    let w = writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");
    seed(&w, "/a/x.jpg");
    seed(&w, "/a/sub/y.jpg");
    seed(&w, "/b/z.jpg");
    w.flush_blocking().expect("flush");

    // Pruning /a removes both rows under it, across all four tables, and only those.
    let deleted = w.prune_under_folder("/a").expect("prune");
    assert_eq!(deleted, 2, "both rows under /a are counted");

    assert_eq!(row_counts(&db_path, "/a/x.jpg"), (0, 0, 0, 0), "/a/x.jpg fully gone");
    assert_eq!(
        row_counts(&db_path, "/a/sub/y.jpg"),
        (0, 0, 0, 0),
        "/a/sub/y.jpg fully gone (nested under /a)"
    );
    assert_eq!(row_counts(&db_path, "/b/z.jpg"), (1, 2, 1, 1), "/b/z.jpg untouched");
    w.shutdown();
}

#[test]
fn prune_all_clip_drops_embeddings_resets_stamps_and_keeps_vision() {
    let dir = tempfile::tempdir().expect("temp");
    let w = writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");
    // Two fully-enriched images, each with a CLIP embedding + stamp on top.
    seed(&w, "/a/x.jpg");
    seed(&w, "/a/y.jpg");
    w.upsert_clip("/a/x.jpg".to_string(), "clip-v1".to_string(), Some(vec![1.0, 0.0]))
        .expect("clip x");
    w.upsert_clip("/a/y.jpg".to_string(), "clip-v1".to_string(), Some(vec![0.0, 1.0]))
        .expect("clip y");
    w.flush_blocking().expect("flush");

    let clip_count = |db: &Path| -> i64 {
        let conn = open_read_connection(db).expect("open read");
        conn.query_row("SELECT COUNT(*) FROM media_clip_embedding", [], |r| r.get(0))
            .expect("count clip")
    };
    let stamps_set = |db: &Path| -> i64 {
        let conn = open_read_connection(db).expect("open read");
        conn.query_row("SELECT COUNT(*) FROM media_status WHERE clip_stamp != ''", [], |r| {
            r.get(0)
        })
        .expect("count stamps")
    };
    assert_eq!(clip_count(&db_path), 2, "both clip embeddings seeded");
    assert_eq!(stamps_set(&db_path), 2, "both clip stamps set");

    let removed = w.prune_all_clip().expect("prune all clip");
    assert_eq!(removed, 2, "both clip embedding rows removed");
    assert_eq!(clip_count(&db_path), 0, "no clip embeddings remain");
    assert_eq!(
        stamps_set(&db_path),
        0,
        "every clip stamp reset to empty (re-install re-embeds)"
    );
    // Vision data (status + ocr + tags + feature-print embedding) is untouched.
    assert_eq!(row_counts(&db_path, "/a/x.jpg"), (1, 2, 1, 1), "Vision rows kept for x");
    assert_eq!(row_counts(&db_path, "/a/y.jpg"), (1, 2, 1, 1), "Vision rows kept for y");
    w.shutdown();
}

#[test]
fn rename_moves_all_children_via_one_row_update_and_keeps_only_those() {
    let dir = tempfile::tempdir().expect("temp");
    let vid = "writer-test-rename";
    coverage::invalidate_accounted(vid);
    let w = writer(dir.path(), vid);
    let db_path = media_db_path(dir.path(), vid);
    seed(&w, "/a/x.jpg");
    seed(&w, "/a/keep.jpg");
    // A CLIP embedding + stamp on the renamed image, so the rename must carry it too.
    w.upsert_clip("/a/x.jpg".to_string(), "clip-v1".to_string(), Some(vec![1.0, 0.0]))
        .expect("clip x");
    w.flush_blocking().expect("flush");

    // The whole enrichment moves from /a/x.jpg to /b/y.jpg with one media_file update.
    assert!(w.rename_path("/a/x.jpg", "/b/y.jpg").expect("rename"), "a row moved");

    // Every Vision child followed (keyed on the unchanged file_id), and the CLIP row +
    // stamp came along; the source path holds nothing anymore.
    assert_eq!(row_counts(&db_path, "/a/x.jpg"), (0, 0, 0, 0), "old path fully empty");
    assert_eq!(
        row_counts(&db_path, "/b/y.jpg"),
        (1, 2, 1, 1),
        "children at the new path"
    );
    let conn = open_read_connection(&db_path).expect("open read");
    let clip_at_new: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM media_clip_embedding c JOIN media_file f ON f.id = c.file_id WHERE f.path = ?1",
            rusqlite::params!["/b/y.jpg"],
            |r| r.get(0),
        )
        .expect("clip count");
    assert_eq!(clip_at_new, 1, "the CLIP embedding followed the rename");
    // The unrelated image is untouched.
    assert_eq!(row_counts(&db_path, "/a/keep.jpg"), (1, 2, 1, 1), "sibling untouched");

    // Accounted moved from /a to /b (the rename crossed parent dirs).
    assert_eq!(accounted(vid, "/a"), 1, "one row left in /a (the sibling)");
    assert_eq!(accounted(vid, "/b"), 1, "the renamed row now counts under /b");

    // A rename of a path with no row is a no-op; a rename onto a taken path is refused.
    assert!(!w.rename_path("/nope.jpg", "/z.jpg").expect("rename"), "no source row");
    assert!(
        !w.rename_path("/b/y.jpg", "/a/keep.jpg").expect("rename"),
        "destination already enriched"
    );
    w.shutdown();
    coverage::invalidate_accounted(vid);
}

#[test]
fn prune_under_folder_is_trailing_slash_safe() {
    let dir = tempfile::tempdir().expect("temp");
    let w = writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");
    seed(&w, "/Photos/a.jpg");
    seed(&w, "/Photos2/b.jpg");
    w.flush_blocking().expect("flush");

    // A sibling that shares a name prefix is NOT within (/Photos2 ≠ under /Photos).
    let deleted = w.prune_under_folder("/Photos").expect("prune");
    assert_eq!(deleted, 1, "only the real child of /Photos goes");
    assert_eq!(row_counts(&db_path, "/Photos/a.jpg").0, 0, "/Photos/a.jpg gone");
    assert_eq!(row_counts(&db_path, "/Photos2/b.jpg").0, 1, "/Photos2 kept");
    w.shutdown();
}

#[test]
fn prune_paths_deletes_only_the_explicit_set() {
    let dir = tempfile::tempdir().expect("temp");
    let w = writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");
    seed(&w, "/p1.jpg");
    seed(&w, "/p2.jpg");
    seed(&w, "/p3.jpg");
    w.flush_blocking().expect("flush");

    let deleted = w
        .prune_paths(vec!["/p1.jpg".to_string(), "/p3.jpg".to_string()])
        .expect("prune");
    assert_eq!(deleted, 2);
    assert_eq!(row_counts(&db_path, "/p1.jpg"), (0, 0, 0, 0), "/p1 gone");
    assert_eq!(row_counts(&db_path, "/p2.jpg"), (1, 2, 1, 1), "/p2 kept");
    assert_eq!(row_counts(&db_path, "/p3.jpg"), (0, 0, 0, 0), "/p3 gone");
    w.shutdown();
}

#[test]
fn prune_then_vacuum_round_trips() {
    // A smoke that VACUUM after a prune commits cleanly and the rows stay gone
    // (VACUUM can't run inside a transaction, so this guards the message ordering).
    let dir = tempfile::tempdir().expect("temp");
    let w = writer(dir.path(), "root");
    let db_path = media_db_path(dir.path(), "root");
    seed(&w, "/a/x.jpg");
    w.flush_blocking().expect("flush");
    assert_eq!(w.prune_under_folder("/a").expect("prune"), 1);
    w.vacuum().expect("vacuum");
    assert_eq!(row_counts(&db_path, "/a/x.jpg"), (0, 0, 0, 0), "row gone after vacuum");
    w.shutdown();
}

// ── The accounted aggregate maintained through the writer path ───────────────
// These use a UNIQUE volume id per test: the accounted cache is process-global and
// keyed by volume id alone, so reusing one id would cross-contaminate.

/// Upsert one image with a given state through `w` and block until it lands.
fn upsert_state(w: &MediaWriter, path: &str, mtime: u64, state: EnrichmentState) {
    w.upsert(
        MediaStatusRow {
            path: path.to_string(),
            mtime: Some(mtime),
            size: Some(2),
            media_kind: MediaKind::Image,
            state,
            engine_version: "e1".to_string(),
            clip_stamp: String::new(),
        },
        (state == EnrichmentState::Done).then(|| UpsertAnalysis::ocr_only("t")),
    )
    .expect("upsert");
    w.flush_blocking().expect("flush");
}

/// The accounted subtree total for `dir` on `volume_id`.
fn accounted(volume_id: &str, dir: &str) -> u64 {
    coverage::accounted_subtrees(volume_id, &[dir.to_string()])[0]
}

#[test]
fn accounted_increments_only_on_a_genuinely_new_done_or_failed_row() {
    let dir = tempfile::tempdir().expect("temp");
    let vid = "writer-test-accounted-increment";
    coverage::invalidate_accounted(vid);
    let w = writer(dir.path(), vid);

    // A brand-new done row bumps its dir's accounted count.
    upsert_state(&w, "/photos/a.jpg", 1, EnrichmentState::Done);
    assert_eq!(accounted(vid, "/photos"), 1, "a genuinely-new done row counts");

    // Re-enriching the SAME path (a later mtime, still done) must NOT double-count.
    upsert_state(&w, "/photos/a.jpg", 2, EnrichmentState::Done);
    assert_eq!(
        accounted(vid, "/photos"),
        1,
        "a re-enrich of an existing path adds nothing"
    );

    // A done → failed transition on the existing path keeps accounted stable.
    upsert_state(&w, "/photos/a.jpg", 2, EnrichmentState::Failed);
    assert_eq!(
        accounted(vid, "/photos"),
        1,
        "done↔failed on an existing path is stable"
    );

    // A brand-new FAILED row DOES count (a corrupt file is accounted, not pending).
    upsert_state(&w, "/photos/b.jpg", 1, EnrichmentState::Failed);
    assert_eq!(accounted(vid, "/photos"), 2, "a new failed row counts toward accounted");

    w.shutdown();
    coverage::invalidate_accounted(vid);
}

#[test]
fn accounted_decrements_on_delete_and_never_goes_negative() {
    let dir = tempfile::tempdir().expect("temp");
    let vid = "writer-test-accounted-decrement";
    coverage::invalidate_accounted(vid);
    let w = writer(dir.path(), vid);

    upsert_state(&w, "/p/a.jpg", 1, EnrichmentState::Done);
    upsert_state(&w, "/p/b.jpg", 1, EnrichmentState::Done);
    assert_eq!(accounted(vid, "/p"), 2);

    // GC of one path drops the count by one.
    w.gc_paths(vec!["/p/a.jpg".to_string()]).expect("gc");
    w.flush_blocking().expect("flush");
    assert_eq!(accounted(vid, "/p"), 1, "a GC'd row leaves the accounted count");

    // GC of a path with NO row moves nothing (no phantom decrement).
    w.gc_paths(vec!["/p/never.jpg".to_string()]).expect("gc");
    w.flush_blocking().expect("flush");
    assert_eq!(accounted(vid, "/p"), 1, "GC of a non-existent row is a no-op");

    // An explicit prune of the last row drains it to zero.
    assert_eq!(w.prune_paths(vec!["/p/b.jpg".to_string()]).expect("prune"), 1);
    assert_eq!(accounted(vid, "/p"), 0, "the folder drains to zero, never negative");

    w.shutdown();
    coverage::invalidate_accounted(vid);
}

#[test]
fn accounted_is_seeded_from_existing_rows_when_a_writer_spawns() {
    // A row written this session, then the writer torn down and the cache dropped,
    // simulates a fresh launch over a populated `media.db`: the NEW writer's spawn
    // must seed accounted from the on-disk rows, not start at zero.
    let dir = tempfile::tempdir().expect("temp");
    let vid = "writer-test-accounted-seed-on-spawn";
    coverage::invalidate_accounted(vid);
    let w1 = writer(dir.path(), vid);
    upsert_state(&w1, "/seed/a.jpg", 1, EnrichmentState::Done);
    upsert_state(&w1, "/seed/b.jpg", 1, EnrichmentState::Failed);
    w1.shutdown();

    // Drop the in-memory aggregate, then spawn a fresh writer over the same DB.
    coverage::invalidate_accounted(vid);
    let w2 = writer(dir.path(), vid);
    // A flush barrier guarantees the writer thread ran its seed (its first action).
    w2.flush_blocking().expect("flush");
    assert_eq!(accounted(vid, "/seed"), 2, "the spawn seeded both stored rows");

    w2.shutdown();
    coverage::invalidate_accounted(vid);
}

#[test]
fn purge_resets_the_accounted_aggregate() {
    let dir = tempfile::tempdir().expect("temp");
    let vid = "writer-test-accounted-purge";
    coverage::invalidate_accounted(vid);
    let w = writer(dir.path(), vid);
    upsert_state(&w, "/x/a.jpg", 1, EnrichmentState::Done);
    upsert_state(&w, "/y/b.jpg", 1, EnrichmentState::Done);
    assert_eq!(accounted(vid, "/"), 2, "root rolls up both dirs");

    w.purge_volume().expect("purge");
    w.flush_blocking().expect("flush");
    assert_eq!(accounted(vid, "/"), 0, "purge zeroes the whole aggregate");

    w.shutdown();
    coverage::invalidate_accounted(vid);
}
