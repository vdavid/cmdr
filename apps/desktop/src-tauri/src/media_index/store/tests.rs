//! Store tests: the path-keyed staleness predicate (an M1 TDD target), the
//! disposable-cache delete-and-recreate on a schema bump, and a status round-trip.

use super::*;
use crate::media_index::writer::MediaWriter;

fn row(mtime: Option<u64>, size: Option<u64>, engine: &str) -> MediaStatusRow {
    MediaStatusRow {
        path: "/a.jpg".to_string(),
        mtime,
        size,
        media_kind: MediaKind::Image,
        state: EnrichmentState::Done,
        engine_version: engine.to_string(),
    }
}

// ── needs_enrichment: the (path, mtime, size) + engine staleness key ────────

#[test]
fn no_row_is_stale() {
    assert!(needs_enrichment(None, Some(1), Some(2), "e1"));
}

#[test]
fn same_identity_and_engine_is_fresh() {
    let stored = row(Some(1), Some(2), "e1");
    assert!(!needs_enrichment(Some(&stored), Some(1), Some(2), "e1"));
}

#[test]
fn changed_mtime_is_stale() {
    let stored = row(Some(1), Some(2), "e1");
    assert!(needs_enrichment(Some(&stored), Some(9), Some(2), "e1"));
}

#[test]
fn changed_size_is_stale() {
    let stored = row(Some(1), Some(2), "e1");
    assert!(needs_enrichment(Some(&stored), Some(1), Some(99), "e1"));
}

#[test]
fn changed_engine_version_is_stale() {
    // An OS/Vision engine bump re-runs OCR even on an unchanged file (data-coverage).
    let stored = row(Some(1), Some(2), "e1");
    assert!(needs_enrichment(Some(&stored), Some(1), Some(2), "e2"));
}

#[test]
fn a_failed_row_at_the_same_identity_is_not_re_hammered() {
    // State does not drive staleness: a failed row at the same identity+engine is
    // "covered" (won't retry every completed scan); only a real file change re-tries.
    let mut stored = row(Some(1), Some(2), "e1");
    stored.state = EnrichmentState::Failed;
    assert!(!needs_enrichment(Some(&stored), Some(1), Some(2), "e1"));
    assert!(
        needs_enrichment(Some(&stored), Some(5), Some(2), "e1"),
        "a changed file re-tries"
    );
}

// ── Disposable cache: delete-and-recreate on a schema bump ──────────────────

#[test]
fn a_schema_mismatch_recreates_the_db_fresh() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "root");

    // Populate a row, then stamp a bogus schema version to simulate an upgrade.
    {
        let store = MediaStore::open(&db_path).expect("open");
        store
            .read_conn()
            .execute(
                "INSERT INTO media_status (path, mtime, size, media_kind, state, engine_version)
                 VALUES ('/x.jpg', 1, 2, 'image', 'done', 'e1')",
                [],
            )
            .expect("insert");
        store
            .read_conn()
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '0')",
                [],
            )
            .expect("bogus version");
    }

    // Reopen: the mismatch must delete-and-recreate, dropping the old row.
    let reopened = MediaStore::open(&db_path).expect("reopen recreates");
    assert!(
        reopened.status_for("/x.jpg").expect("read").is_none(),
        "a schema mismatch recreates the DB fresh (row is gone)"
    );
}

#[test]
fn status_round_trips_through_the_writer() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("open");
    let writer = MediaWriter::spawn(&db_path).expect("writer");
    writer
        .upsert(row(Some(7), Some(8), "e1"), Some("text".to_string()))
        .expect("upsert");
    writer.flush_blocking().expect("flush");

    let store = MediaStore::open(&db_path).expect("reopen");
    let got = store.status_for("/a.jpg").expect("read").expect("present");
    assert_eq!(got.mtime, Some(7));
    assert_eq!(got.size, Some(8));
    assert_eq!(got.state, EnrichmentState::Done);
    writer.shutdown();
}
