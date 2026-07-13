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
        .upsert(
            row(Some(7), Some(8), "e1"),
            Some(crate::media_index::writer::UpsertAnalysis::ocr_only("text")),
        )
        .expect("upsert");
    writer.flush_blocking().expect("flush");

    let store = MediaStore::open(&db_path).expect("reopen");
    let got = store.status_for("/a.jpg").expect("read").expect("present");
    assert_eq!(got.mtime, Some(7));
    assert_eq!(got.size, Some(8));
    assert_eq!(got.state, EnrichmentState::Done);
    writer.shutdown();
}

// ── Embedding codec + tags/embedding round-trip (M2) ────────────────────────

#[test]
fn embedding_codec_round_trips() {
    let v = vec![1.5f32, -2.0, 0.0, 3.25];
    let bytes = encode_embedding(&v);
    assert_eq!(bytes.len(), v.len() * 4);
    assert_eq!(decode_embedding(&bytes), Some(v));
    // A non-multiple-of-4 length is rejected (a corrupt row degrades to "no vector").
    assert_eq!(decode_embedding(&[1, 2, 3]), None);
}

#[test]
fn tags_and_embedding_round_trip_and_filter_by_score() {
    use crate::media_index::backend::Tag;
    use crate::media_index::writer::UpsertAnalysis;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("open");
    let writer = MediaWriter::spawn(&db_path).expect("writer");
    writer
        .upsert(
            row(Some(1), Some(2), "e1"),
            Some(UpsertAnalysis {
                ocr_text: "poster text".to_string(),
                tags: vec![
                    Tag {
                        label: "beach".to_string(),
                        score: 0.9,
                    },
                    Tag {
                        label: "sky".to_string(),
                        score: 0.3,
                    },
                ],
                embedding: Some(vec![0.1, 0.2, 0.3, 0.4]),
            }),
        )
        .expect("upsert");
    writer.flush_blocking().expect("flush");

    let store = MediaStore::open(&db_path).expect("reopen");
    let conn = store.read_conn();

    // The embedding persists and decodes.
    assert_eq!(
        read_embedding_for(conn, "/a.jpg").expect("read"),
        Some(vec![0.1, 0.2, 0.3, 0.4])
    );

    // Tag-score filtering: `beach` above 0.5 matches; `sky` at 0.3 doesn't clear 0.5.
    let beach = read_tag_matches(conn, "beach", 0.5).expect("read");
    assert_eq!(beach, vec![("/a.jpg".to_string(), 0.9)]);
    assert!(read_tag_matches(conn, "sky", 0.5).expect("read").is_empty());
    // Below the floor, `sky` at 0.0 threshold does match.
    assert_eq!(
        read_tag_matches(conn, "sky", 0.0).expect("read"),
        vec![("/a.jpg".to_string(), 0.3)]
    );

    writer.shutdown();
}

#[test]
fn re_enrichment_replaces_prior_tags_and_embedding() {
    use crate::media_index::backend::Tag;
    use crate::media_index::writer::UpsertAnalysis;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("open");
    let writer = MediaWriter::spawn(&db_path).expect("writer");
    // First enrichment.
    writer
        .upsert(
            row(Some(1), Some(2), "e1"),
            Some(UpsertAnalysis {
                ocr_text: String::new(),
                tags: vec![Tag {
                    label: "old".to_string(),
                    score: 0.8,
                }],
                embedding: Some(vec![1.0, 0.0]),
            }),
        )
        .expect("upsert 1");
    // A failure clears the derived rows (no stale tags/embedding survive).
    writer.upsert(row(Some(3), Some(4), "e1"), None).expect("upsert 2");
    writer.flush_blocking().expect("flush");

    let store = MediaStore::open(&db_path).expect("reopen");
    let conn = store.read_conn();
    assert!(
        read_tag_matches(conn, "old", 0.0).expect("read").is_empty(),
        "stale tags cleared"
    );
    assert_eq!(
        read_embedding_for(conn, "/a.jpg").expect("read"),
        None,
        "stale embedding cleared"
    );
    writer.shutdown();
}
