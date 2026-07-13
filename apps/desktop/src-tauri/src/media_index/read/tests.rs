//! Read-API + FTS tests: the FTS5 availability smoke, the query builder (an M1 TDD
//! target), and an end-to-end search round-trip (incl. the offline-after-unmount
//! posture).

use super::*;
use crate::media_index::predicate::MediaKind;
use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};
use crate::media_index::writer::MediaWriter;

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
            },
            Some(crate::media_index::writer::UpsertAnalysis::ocr_only(
                "a sunset over the beach with palm trees",
            )),
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
