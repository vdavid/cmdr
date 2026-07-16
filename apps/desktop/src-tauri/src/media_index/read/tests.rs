//! Read-API + FTS tests: the FTS5 availability smoke, the query builder (a TDD
//! target), and an end-to-end search round-trip (incl. the offline-after-unmount
//! posture).

use super::*;
use crate::media_index::backend::Tag;
use crate::media_index::predicate::MediaKind;
use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};
use crate::media_index::writer::{MediaWriter, UpsertAnalysis};

/// FTS5 availability smoke: a bundled SQLite build must be able to create an fts5
/// virtual table. Decision 2's build-flag worry is closed (agent/store proves it),
/// so this is a cheap runtime guard, not a milestone gate.
#[test]
fn fts5_virtual_table_can_be_created() {
    let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
    conn.execute_batch("CREATE VIRTUAL TABLE t USING fts5(body);")
        .expect("bundled SQLite must compile FTS5");
    conn.execute("INSERT INTO t (body) VALUES ('hello world')", [])
        .expect("insert");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM t WHERE t MATCH 'hello'", [], |r| r.get(0))
        .expect("match");
    assert_eq!(n, 1);
}

// ── build_ocr_match_query (TDD target) ────────────────────────────────────

#[test]
fn empty_or_whitespace_query_is_none() {
    assert_eq!(build_ocr_match_query(""), None);
    assert_eq!(build_ocr_match_query("   "), None);
}

#[test]
fn each_token_is_quoted_as_a_literal() {
    // Multiple terms ⇒ each quoted, space-joined (implicit AND).
    assert_eq!(
        build_ocr_match_query("beach sunset"),
        Some("\"beach\" \"sunset\"".to_string())
    );
}

#[test]
fn special_characters_that_would_be_fts_syntax_are_quoted() {
    // Parens, colons, and bareword operators would throw an fts5 syntax error raw;
    // quoting makes them literals. We assert the built query parses + runs.
    for raw in ["report(v2)", "foo:bar", "AND", "NOT", "a\"b", "c-d"] {
        let q = build_ocr_match_query(raw).expect("non-empty");
        let conn = rusqlite::Connection::open_in_memory().expect("db");
        conn.execute_batch("CREATE VIRTUAL TABLE t USING fts5(text);")
            .expect("fts5");
        // The built query must be valid fts5 syntax (no error), the whole point of
        // the sanitizer. A raw `MATCH ?` with these inputs would throw.
        let res: Result<i64, _> = conn.query_row("SELECT COUNT(*) FROM t WHERE t MATCH ?1", [&q], |r| r.get(0));
        assert!(res.is_ok(), "sanitized query for {raw:?} must be valid fts5: {q}");
    }
}

// ── End-to-end search + offline read ──────────────────────────────────────

#[test]
fn search_finds_the_image_by_ocr_text_and_survives_unmount() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path).expect("writer");

    writer
        .upsert(
            MediaStatusRow {
                path: "/photos/beach.jpg".to_string(),
                mtime: Some(1),
                size: Some(2),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "e1".to_string(),
                clip_stamp: String::new(),
            },
            Some(UpsertAnalysis::ocr_only("a sunset over the beach with palm trees")),
        )
        .expect("upsert");
    writer.flush_blocking().expect("flush");

    let index = MediaIndex::open(dir.path(), "root");
    let hits = index.search_ocr("beach", 10).expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "/photos/beach.jpg");
    assert!(
        hits[0].snippet.contains('['),
        "the snippet highlights the match: {}",
        hits[0].snippet
    );

    // A term that isn't in the text returns nothing (implicit-AND of quoted terms).
    assert!(index.search_ocr("mountain", 10).expect("search").is_empty());

    assert_eq!(index.enriched_count().expect("count"), 1);

    // Offline: the read API answers from `media.db` directly. The writer thread is
    // the only live handle; dropping it (an "unmount") leaves the DB on disk, and a
    // fresh read still returns the hit — the offline-after-unmount property.
    writer.shutdown();
    let offline = MediaIndex::open(dir.path(), "root");
    assert_eq!(offline.search_ocr("palm", 10).expect("offline search").len(), 1);
}

// ── Tag search is case-insensitive ────────────────────────────────────────

/// Store one image tagged `sky` (Vision's taxonomy labels are lowercase), then read
/// it back. `images_with_tag` must fold the query so a capitalized `"Sky"` finds it,
/// and the FTS-folded tag words must already tokenize case-insensitively.
#[test]
fn tag_search_is_case_insensitive() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path).expect("writer");

    writer
        .upsert(
            MediaStatusRow {
                path: "/photos/clouds.jpg".to_string(),
                mtime: Some(1),
                size: Some(2),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "e1".to_string(),
                clip_stamp: String::new(),
            },
            Some(UpsertAnalysis {
                tags: vec![Tag {
                    label: "sky".to_string(),
                    score: 0.9,
                }],
                ..Default::default()
            }),
        )
        .expect("upsert");
    writer.flush_blocking().expect("flush");

    let index = MediaIndex::open(dir.path(), "root");

    // Structured tag-score filter: a capitalized query must find the lowercase tag.
    let hits = index.images_with_tag("Sky", 0.0).expect("tag search");
    assert_eq!(hits.len(), 1, "capitalized 'Sky' must match stored 'sky'");
    assert_eq!(hits[0].path, "/photos/clouds.jpg");
    // The exact-case query still works.
    assert_eq!(index.images_with_tag("sky", 0.0).expect("tag search").len(), 1);

    // The FTS-folded tag words (source='tag' rows) already tokenize case-insensitively,
    // so an uppercase keyword search finds the folded tag too.
    assert_eq!(
        index.search_ocr("SKY", 10).expect("ocr search").len(),
        1,
        "FTS tokenization folds case, so uppercase keyword finds the folded tag"
    );
}

// ── Semantic (CLIP) search (plan M3) ───────────────────────────────────────

/// Seed a status row + a CLIP embedding for `path` (CLIP requires an existing status row,
/// since a real CLIP-only pass only runs for a Vision-current image).
fn seed_clip(writer: &MediaWriter, path: &str, vector: Vec<f32>) {
    writer
        .upsert(
            MediaStatusRow {
                path: path.to_string(),
                mtime: Some(1),
                size: Some(1),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "e1".to_string(),
                clip_stamp: String::new(),
            },
            Some(UpsertAnalysis::ocr_only("x")),
        )
        .expect("seed status");
    writer
        .upsert_clip(path.to_string(), "clip-v1".to_string(), Some(vector))
        .expect("seed clip");
}

#[test]
fn search_semantic_ranks_by_clip_cosine_and_honors_k() {
    let dir = tempfile::tempdir().expect("temp");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("store");
    let writer = MediaWriter::spawn(&db_path).expect("writer");
    // Three images at orthogonal directions in the (fake) CLIP space.
    seed_clip(&writer, "/cat.jpg", vec![1.0, 0.0, 0.0]);
    seed_clip(&writer, "/dog.jpg", vec![0.0, 1.0, 0.0]);
    seed_clip(&writer, "/beach.jpg", vec![0.0, 0.0, 1.0]);
    writer.flush_blocking().expect("flush");

    let index = MediaIndex::open(dir.path(), "root");
    // A query vector closest to /beach.jpg's direction.
    let hits = index.search_semantic(&[0.1, 0.2, 0.9], 2);
    assert_eq!(hits.len(), 2, "k caps the result count");
    assert_eq!(hits[0].path, "/beach.jpg", "the nearest CLIP vector ranks first");
    assert!(hits[0].score > hits[1].score, "sorted by cosine descending");

    // The CLIP cache reads media.db directly, so it still answers with the volume gone.
    crate::media_index::vector::cache::invalidate(&db_path);
    writer.shutdown();
    let offline = MediaIndex::open(dir.path(), "root").search_semantic(&[1.0, 0.0, 0.0], 1);
    assert_eq!(offline.len(), 1);
    assert_eq!(offline[0].path, "/cat.jpg", "semantic search answers offline from media.db");
}

#[test]
fn search_semantic_is_empty_without_clip_embeddings() {
    // A volume with OCR/tags but no CLIP model (no clip embeddings) returns nothing.
    let dir = tempfile::tempdir().expect("temp");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("store");
    let writer = MediaWriter::spawn(&db_path).expect("writer");
    writer
        .upsert(
            MediaStatusRow {
                path: "/x.jpg".to_string(),
                mtime: Some(1),
                size: Some(1),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "e1".to_string(),
                clip_stamp: String::new(),
            },
            Some(UpsertAnalysis::ocr_only("beach")),
        )
        .expect("seed");
    writer.flush_blocking().expect("flush");
    let index = MediaIndex::open(dir.path(), "root");
    assert!(
        index.search_semantic(&[1.0, 0.0, 0.0], 5).is_empty(),
        "no CLIP embeddings ⇒ no semantic hits"
    );
    writer.shutdown();
}
