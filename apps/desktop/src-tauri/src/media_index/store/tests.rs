//! Store tests: the path-keyed staleness predicate (a TDD target), the
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
        clip_stamp: String::new(),
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

// ── Embedding codec + tags/embedding round-trip ────────────────────────

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
fn sum_bytes_for_paths_totals_the_doomed_rows_content_and_only_those() {
    use crate::media_index::backend::Tag;
    use crate::media_index::writer::UpsertAnalysis;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("open");
    let writer = MediaWriter::spawn(&db_path).expect("writer");

    // Image a: OCR text "hello" (5 bytes) + a 2-float embedding (8 bytes), no tags.
    writer
        .upsert(
            MediaStatusRow {
                path: "/a.jpg".to_string(),
                mtime: Some(1),
                size: Some(1),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "e1".to_string(),
                clip_stamp: String::new(),
            },
            Some(UpsertAnalysis {
                ocr_text: "hello".to_string(),
                tags: vec![],
                embedding: Some(vec![0.1, 0.2]),
            }),
        )
        .expect("upsert a");
    // Image b: no OCR text, one tag "beach" (folded FTS text 5 + structured label 5),
    // and a 1-float embedding (4 bytes).
    writer
        .upsert(
            MediaStatusRow {
                path: "/b.jpg".to_string(),
                mtime: Some(1),
                size: Some(1),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "e1".to_string(),
                clip_stamp: String::new(),
            },
            Some(UpsertAnalysis {
                ocr_text: String::new(),
                tags: vec![Tag {
                    label: "beach".to_string(),
                    score: 0.9,
                }],
                embedding: Some(vec![1.0]),
            }),
        )
        .expect("upsert b");
    writer.flush_blocking().expect("flush");

    let store = MediaStore::open(&db_path).expect("reopen");
    let conn = store.read_conn();

    let only_a: std::collections::HashSet<String> = ["/a.jpg".to_string()].into_iter().collect();
    let only_b: std::collections::HashSet<String> = ["/b.jpg".to_string()].into_iter().collect();
    let both: std::collections::HashSet<String> = ["/a.jpg".to_string(), "/b.jpg".to_string()].into_iter().collect();
    let none: std::collections::HashSet<String> = std::collections::HashSet::new();

    assert_eq!(
        sum_bytes_for_paths(conn, &only_a).expect("sum a"),
        5 + 8,
        "a: ocr text + embedding"
    );
    assert_eq!(
        sum_bytes_for_paths(conn, &only_b).expect("sum b"),
        5 + 5 + 4,
        "b: folded tag text + structured label + embedding"
    );
    assert_eq!(
        sum_bytes_for_paths(conn, &both).expect("sum both"),
        (5 + 8) + (5 + 5 + 4),
        "both is the sum of each"
    );
    assert_eq!(
        sum_bytes_for_paths(conn, &none).expect("sum none"),
        0,
        "empty set totals nothing"
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

// ── needs_clip: the INDEPENDENT CLIP staleness half (plan M3) ───────────────

fn row_with_clip(clip_stamp: &str) -> MediaStatusRow {
    let mut r = row(Some(1), Some(2), "e1");
    r.clip_stamp = clip_stamp.to_string();
    r
}

#[test]
fn no_installed_model_is_never_clip_stale() {
    // clip_stamp None (no model) ⇒ nothing is ever CLIP-stale, even a row that never
    // had a CLIP embedding.
    assert!(!needs_clip(None, None));
    assert!(!needs_clip(Some(&row_with_clip("")), None));
}

#[test]
fn a_row_without_a_clip_embedding_is_stale_once_a_model_installs() {
    // A fresh row (no CLIP stamp) becomes stale the moment a model is installed.
    assert!(needs_clip(Some(&row_with_clip("")), Some("clip-v1")));
    // A path with no row at all is stale too (a fresh image).
    assert!(needs_clip(None, Some("clip-v1")));
}

#[test]
fn a_matching_clip_stamp_is_current_and_a_bump_is_stale() {
    // Same stamp ⇒ current (not stale); a different stamp (model or OS change) ⇒ stale.
    assert!(!needs_clip(Some(&row_with_clip("clip-v1")), Some("clip-v1")));
    assert!(needs_clip(Some(&row_with_clip("clip-v1")), Some("clip-v2")));
}

#[test]
fn clip_and_vision_staleness_are_independent() {
    // A Vision-current row can still be CLIP-stale (installing CLIP), and vice versa —
    // the two predicates never conflate (plan M3 Q5).
    let vision_current_clip_stale = row_with_clip(""); // engine e1 current, no clip yet
    assert!(!needs_enrichment(
        Some(&vision_current_clip_stale),
        Some(1),
        Some(2),
        "e1"
    ));
    assert!(needs_clip(Some(&vision_current_clip_stale), Some("clip-v1")));
}
