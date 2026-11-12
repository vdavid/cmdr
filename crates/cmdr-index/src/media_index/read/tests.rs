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
    let conn = cmdr_fs::sqlite_util::open_in_memory().expect("open in-memory");
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
        let conn = cmdr_fs::sqlite_util::open_in_memory().expect("db");
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
    let writer = MediaWriter::spawn(&db_path, "root").expect("writer");

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
    let writer = MediaWriter::spawn(&db_path, "root").expect("writer");

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
    let writer = MediaWriter::spawn(&db_path, "root").expect("writer");

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
    let writer = MediaWriter::spawn(&db_path, "root").expect("writer");
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
    let writer = MediaWriter::spawn(&db_path, "root").expect("writer");
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
    let writer = MediaWriter::spawn(&db_path, "root").expect("writer");
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

// ── Excluded folders never surface ─────────────────────────────────────────
// An exclusion has to hold at READ time, whatever the retro-delete did: the purge can fail
// to land (a full disk, a locked db), a folder excluded while its NAS was offline is only
// purged on reconnect, and Ask Cmdr sends what these reads return to a cloud model. Each
// test seeds an image under an excluded folder beside a kept one and asks ONE read path,
// so a regression names the path that leaked.

use crate::media_index::network::config::{NetworkEnrichConfig, set_config};

const SECRET: &str = "/Users/me/IDs/passport.jpg";
const KEPT: &str = "/Users/me/Photos/receipt.jpg";

/// Resets the process-global exclusion on drop, so a failing test can't leave a folder
/// excluded for the next one.
struct Exclusions;

impl Drop for Exclusions {
    fn drop(&mut self) {
        set_config(NetworkEnrichConfig::default());
    }
}

fn exclude(folders: &[&str]) -> Exclusions {
    set_config(NetworkEnrichConfig {
        excluded_folders: folders.iter().map(|f| f.to_string()).collect(),
        ..NetworkEnrichConfig::default()
    });
    Exclusions
}

/// One enriched image carrying everything a read can return: OCR text, a `document` tag,
/// a feature print, and a CLIP vector.
fn seed_searchable(writer: &MediaWriter, path: &str, text: &str, print: Vec<f32>, clip: Vec<f32>) {
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
                ocr_text: text.to_string(),
                tags: vec![Tag {
                    label: "document".to_string(),
                    score: 0.9,
                }],
                embedding: Some(print),
            }),
        )
        .expect("seed image");
    writer
        .upsert_clip(path.to_string(), "clip-v1".to_string(), Some(clip))
        .expect("seed clip");
}

/// A local volume holding [`SECRET`] under the excluded `/Users/me/IDs` and [`KEPT`]
/// beside it, close enough in every space to match the same queries. Field order is drop
/// order: the exclusion resets, the writer stops, then the directory goes.
struct ExcludedFolderVolume {
    index: MediaIndex,
    _exclusions: Exclusions,
    writer: MediaWriter,
    _dir: tempfile::TempDir,
}

impl Drop for ExcludedFolderVolume {
    fn drop(&mut self) {
        self.writer.shutdown();
    }
}

fn excluded_folder_volume(volume_id: &str) -> ExcludedFolderVolume {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), volume_id);
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path, volume_id).expect("writer");
    seed_searchable(
        &writer,
        SECRET,
        "passport number X1234567",
        vec![1.0, 0.0, 0.0],
        vec![1.0, 0.0, 0.0],
    );
    seed_searchable(
        &writer,
        KEPT,
        "passport photo booth receipt",
        vec![0.99, 0.01, 0.0],
        vec![0.9, 0.1, 0.0],
    );
    writer.flush_blocking().expect("flush");
    // A local volume: its stored paths ARE its OS paths.
    crate::media_index::store::seed_mount_root(&db_path, "/");
    cache::invalidate(&db_path);
    ExcludedFolderVolume {
        index: MediaIndex::open(dir.path(), volume_id),
        _exclusions: exclude(&["/Users/me/IDs"]),
        writer,
        _dir: dir,
    }
}

#[test]
fn an_excluded_folder_never_surfaces_in_ocr_search() {
    let volume = excluded_folder_volume("excluded-ocr");
    let hits: Vec<String> = volume
        .index
        .search_ocr("passport", 10)
        .expect("search")
        .into_iter()
        .map(|h| h.path)
        .collect();
    assert_eq!(hits, vec![KEPT]);
}

#[test]
fn an_excluded_folder_never_surfaces_in_tag_search() {
    let volume = excluded_folder_volume("excluded-tag");
    let hits: Vec<String> = volume
        .index
        .images_with_tag("document", 0.0)
        .expect("search")
        .into_iter()
        .map(|h| h.path)
        .collect();
    assert_eq!(hits, vec![KEPT]);
}

#[test]
fn an_excluded_folder_never_surfaces_in_description_search() {
    let volume = excluded_folder_volume("excluded-semantic");
    let hits: Vec<String> = volume
        .index
        .search_semantic(&[1.0, 0.0, 0.0], 10)
        .into_iter()
        .map(|h| h.path)
        .collect();
    assert_eq!(hits, vec![KEPT]);
}

#[test]
fn an_excluded_image_is_no_find_similar_source() {
    let volume = excluded_folder_volume("excluded-similar-source");
    assert!(volume.index.find_similar(SECRET, 10).expect("similar").is_empty());
}

#[test]
fn an_excluded_image_is_no_find_similar_result() {
    // The kept image's only neighbor is the excluded one.
    let volume = excluded_folder_volume("excluded-similar-result");
    assert!(volume.index.find_similar(KEPT, 10).expect("similar").is_empty());
}

#[test]
fn an_excluded_image_is_no_near_duplicate() {
    // The two feature prints are near-identical, so the only cluster would pair them.
    let volume = excluded_folder_volume("excluded-dedup");
    assert!(volume.index.dedup_clusters(0.9).is_empty());
}

#[test]
fn an_excluded_image_has_no_facts() {
    // `image_facts` reads the FULL OCR text, the most sensitive thing any read returns.
    let volume = excluded_folder_volume("excluded-facts");
    let facts = volume.index.facts_for_paths(&[SECRET, KEPT]).expect("facts");
    assert!(
        !facts[0].indexed && facts[0].ocr_text.is_none() && facts[0].tags.is_empty(),
        "reads exactly as never indexed"
    );
    assert!(facts[1].indexed);
}

#[test]
fn an_excluded_image_has_no_stored_status() {
    // The file-status badge then reads it as excluded rather than indexed.
    let volume = excluded_folder_volume("excluded-status");
    let status = volume.index.status_for_paths(&[SECRET.to_string(), KEPT.to_string()]);
    assert!(!status.contains_key(SECRET));
    assert!(status.contains_key(KEPT));
}

#[test]
fn an_offline_network_volume_places_its_rows_by_the_mount_root_it_was_indexed_under() {
    // The NAS is unmounted, so nothing live knows its mount root, yet its media.db still
    // answers. The recorded root maps the OS-path exclusion onto its index-relative rows.
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "excluded-offline-nas");
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path, "excluded-offline-nas").expect("writer");
    seed_facts(&writer, "/Photos/scan.jpg", "invoice from the tax office", vec![]);
    seed_facts(&writer, "/Docs/scan.jpg", "invoice from the plumber", vec![]);
    writer.flush_blocking().expect("flush");
    crate::media_index::store::seed_mount_root(&db_path, "/Volumes/testshare");
    let _exclusions = exclude(&["/Volumes/testshare/Photos"]);

    let hits: Vec<String> = MediaIndex::open(dir.path(), "excluded-offline-nas")
        .search_ocr("invoice", 10)
        .expect("search")
        .into_iter()
        .map(|h| h.path)
        .collect();
    writer.shutdown();
    assert_eq!(hits, vec!["/Docs/scan.jpg"]);
}

#[test]
fn a_volume_with_no_known_mount_root_shows_nothing_while_a_folder_is_excluded() {
    // Neither mounted nor ever recorded (a NAS indexed before roots were recorded, offline
    // since): its rows can't be placed against the exclusion, so none may surface until
    // the volume's next pass records the root.
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "excluded-unknown-root");
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path, "excluded-unknown-root").expect("writer");
    seed_facts(&writer, "/Photos/scan.jpg", "invoice from the tax office", vec![]);
    writer.flush_blocking().expect("flush");
    let _exclusions = exclude(&["/Volumes/testshare/Photos"]);

    let hits = MediaIndex::open(dir.path(), "excluded-unknown-root")
        .search_ocr("invoice", 10)
        .expect("search");
    writer.shutdown();
    assert!(hits.is_empty(), "an unplaceable row fails closed");
}

#[test]
fn a_share_remounted_elsewhere_stays_placed_by_every_root_it_was_indexed_under() {
    // Indexed at /Volumes/testshare, where the folder was excluded, then again at
    // /Volumes/testshare-1 after a remount. Both roots stay known, so the folder stays hidden
    // with the share offline.
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "excluded-remounted-nas");
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path, "excluded-remounted-nas").expect("writer");
    seed_facts(&writer, "/Photos/scan.jpg", "invoice from the tax office", vec![]);
    seed_facts(&writer, "/Docs/scan.jpg", "invoice from the plumber", vec![]);
    writer.flush_blocking().expect("flush");
    crate::media_index::store::seed_mount_root(&db_path, "/Volumes/testshare");
    crate::media_index::store::seed_mount_root(&db_path, "/Volumes/testshare-1");
    let _exclusions = exclude(&["/Volumes/testshare/Photos"]);

    let hits: Vec<String> = MediaIndex::open(dir.path(), "excluded-remounted-nas")
        .search_ocr("invoice", 10)
        .expect("search")
        .into_iter()
        .map(|h| h.path)
        .collect();
    writer.shutdown();
    assert_eq!(hits, vec!["/Docs/scan.jpg"]);
}

#[cfg(target_os = "macos")]
#[test]
fn an_nfc_exclusion_hides_an_nfd_stored_path() {
    // macOS hands the index NFD names; the exclusion arrives in whatever form it was typed
    // or stored. The read has to treat both as the same folder.
    use unicode_normalization::UnicodeNormalization;
    let nfd_path: String = "/Users/me/Útikönyv/scan.jpg".nfd().collect();
    let nfc_folder: String = "/Users/me/Útikönyv".nfc().collect();
    assert!(
        !nfd_path.starts_with(&nfc_folder),
        "premise: the two forms differ byte for byte"
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "excluded-nfc");
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path, "excluded-nfc").expect("writer");
    seed_facts(&writer, &nfd_path, "passport number X1234567", vec![]);
    writer.flush_blocking().expect("flush");
    crate::media_index::store::seed_mount_root(&db_path, "/");
    let _exclusions = exclude(&[&nfc_folder]);

    let hits = MediaIndex::open(dir.path(), "excluded-nfc")
        .search_ocr("passport", 10)
        .expect("search");
    writer.shutdown();
    assert!(hits.is_empty(), "the NFD row sits under the NFC folder");
}

#[test]
fn a_share_mounted_at_a_new_root_stays_excluded_at_the_root_it_was_excluded_under() {
    // Mounted today at /Volumes/testshare-1, indexed earlier at /Volumes/testshare, where the folder
    // was excluded. Placing the rows by the live root alone would put them under testshare-1
    // and show them.
    use cmdr_fs::volume::InMemoryVolume;
    use std::sync::Arc;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = media_db_path(dir.path(), "excluded-live-remount");
    MediaStore::open(&db_path).expect("open store");
    let writer = MediaWriter::spawn(&db_path, "excluded-live-remount").expect("writer");
    seed_facts(&writer, "/Photos/scan.jpg", "invoice from the tax office", vec![]);
    seed_facts(&writer, "/Docs/scan.jpg", "invoice from the plumber", vec![]);
    writer.flush_blocking().expect("flush");
    crate::media_index::store::seed_mount_root(&db_path, "/Volumes/testshare");
    let provider = crate::indexing::host::volumes::FakeVolumeProvider::shared();
    provider.register(
        "excluded-live-remount",
        Arc::new(InMemoryVolume::new("testshare").with_root("/Volumes/testshare-1")),
    );
    let _installed = crate::indexing::host::volumes::install_for_test(provider);
    let _exclusions = exclude(&["/Volumes/testshare/Photos"]);

    let hits: Vec<String> = MediaIndex::open(dir.path(), "excluded-live-remount")
        .search_ocr("invoice", 10)
        .expect("search")
        .into_iter()
        .map(|h| h.path)
        .collect();
    writer.shutdown();
    assert_eq!(hits, vec!["/Docs/scan.jpg"]);
}
