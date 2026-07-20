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

// ── Lookup direction: facts_for_paths ─────────────────────────────────────

/// Seed one enriched image with OCR text and tags.
fn seed_facts(writer: &MediaWriter, path: &str, ocr: &str, tags: Vec<Tag>) {
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
                ocr_text: ocr.to_string(),
                tags,
                ..Default::default()
            }),
        )
        .expect("seed facts");
}

/// The lookup direction: given paths the caller already has, return the stored facts.
/// Pins the three properties the rename flow depends on: the FULL OCR text (not a
/// snippet), OCR text and tags as DISTINCT fields (they share `media_ocr` behind a
/// `source` column, so a naive read would fold the tag labels into the text), and one
/// entry per requested path so a never-enriched file is representable rather than dropped.
#[test]
fn facts_for_paths_returns_full_text_distinct_tags_and_keeps_unknown_paths() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path).expect("writer");

    let long_text = "Invoice 2026-07-14 total 1 234 SEK paid by card, thank you for your business";
    seed_facts(
        &writer,
        "/photos/receipt.jpg",
        long_text,
        vec![
            Tag {
                label: "document".to_string(),
                score: 0.91,
            },
            Tag {
                label: "paper".to_string(),
                score: 0.44,
            },
        ],
    );
    // An enriched image with tags but no recognized text.
    seed_facts(
        &writer,
        "/photos/sky.jpg",
        "",
        vec![Tag {
            label: "sky".to_string(),
            score: 0.8,
        }],
    );
    writer.flush_blocking().expect("flush");

    let index = MediaIndex::open(dir.path(), "root");
    let facts = index
        .facts_for_paths(&["/photos/receipt.jpg", "/photos/sky.jpg", "/photos/never.jpg"])
        .expect("facts");

    assert_eq!(facts.len(), 3, "one entry per requested path, in request order");
    assert_eq!(facts[0].path, "/photos/receipt.jpg");
    assert_eq!(facts[1].path, "/photos/sky.jpg");
    assert_eq!(facts[2].path, "/photos/never.jpg");

    // The FULL stored text, not a snippet: a model reasons over the whole thing.
    assert!(facts[0].indexed);
    assert_eq!(facts[0].ocr_text.as_deref(), Some(long_text));
    // Tags come back structurally, highest score first — never folded into `ocr_text`.
    let labels: Vec<&str> = facts[0].tags.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, vec!["document", "paper"]);
    assert!((facts[0].tags[0].score - 0.91).abs() < 1e-6);
    assert!(
        !facts[0].ocr_text.as_deref().expect("text").contains("document"),
        "the folded `source = 'tag'` FTS row must not leak into the OCR text"
    );

    // Enriched, no text found ⇒ indexed with no `ocr_text`, distinct from never-indexed.
    assert!(facts[1].indexed);
    assert_eq!(facts[1].ocr_text, None);
    assert_eq!(facts[1].tags.len(), 1);

    // Never enriched ⇒ present but flagged, so the caller can say "not indexed yet".
    assert!(!facts[2].indexed);
    assert_eq!(facts[2].ocr_text, None);
    assert!(facts[2].tags.is_empty());

    writer.shutdown();
}

/// A missing DB (never enriched, or offline and purged) must never error — the module's
/// convention — and must still answer per-path so the caller can tell "not indexed yet".
#[test]
fn facts_for_paths_on_a_missing_db_answers_not_indexed_rather_than_erroring() {
    let dir = tempfile::tempdir().expect("temp dir");
    let index = MediaIndex::open(dir.path(), "never-enriched");
    let facts = index.facts_for_paths(&["/a.jpg", "/b.jpg"]).expect("no error");
    assert_eq!(facts.len(), 2);
    assert!(
        facts
            .iter()
            .all(|f| !f.indexed && f.ocr_text.is_none() && f.tags.is_empty())
    );
    assert!(index.facts_for_paths(&[]).expect("empty").is_empty());
}

/// More paths than SQLite's 999-host-parameter ceiling must chunk, not throw. A rename
/// over a big folder hits this immediately.
#[test]
fn facts_for_paths_chunks_past_the_sqlite_parameter_limit() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "root");
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path).expect("writer");
    seed_facts(&writer, "/photos/img-1500.jpg", "needle", vec![]);
    writer.flush_blocking().expect("flush");

    let paths: Vec<String> = (0..2_000).map(|i| format!("/photos/img-{i}.jpg")).collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let facts = MediaIndex::open(dir.path(), "root")
        .facts_for_paths(&refs)
        .expect("chunked read");

    assert_eq!(facts.len(), 2_000);
    let seeded = facts.iter().find(|f| f.path == "/photos/img-1500.jpg").expect("seeded");
    assert_eq!(seeded.ocr_text.as_deref(), Some("needle"));
    assert_eq!(facts.iter().filter(|f| f.indexed).count(), 1);

    writer.shutdown();
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
    cache::invalidate(&db_path);
    writer.shutdown();
    let offline = MediaIndex::open(dir.path(), "root").search_semantic(&[1.0, 0.0, 0.0], 1);
    assert_eq!(offline.len(), 1);
    assert_eq!(
        offline[0].path, "/cat.jpg",
        "semantic search answers offline from media.db"
    );
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
