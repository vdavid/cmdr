//! `MediaIndex`: the consumable read API over a volume's `media.db`.
//!
//! Ported from `importance/read` (`ImportanceIndex`): the ONE way a consumer
//! (`search/`, later the agent + MCP) reaches image-enrichment results (plan
//! Decision 8). No consumer takes a raw `rusqlite` dep on `media.db`; they call
//! this. It owns a `platform_case`-registered read connection and reads the DB
//! directly, so it answers OFFLINE from `media.db` after the volume unmounts.
//!
//! ## FTS query building (a TDD target)
//!
//! Raw user input must NEVER be fed into `... MATCH ?` — ordinary filename/text
//! fragments (`report(v2)`, `foo:bar`, a bareword `AND`/`OR`) throw an fts5 syntax
//! error, and binding doesn't help (the string is parsed as query syntax). Same
//! gotcha as `agent/store`'s `sanitize_fts_query`; [`build_ocr_match_query`] is our
//! sanitizer — it quotes each whitespace token so every term is a literal.

use std::path::PathBuf;

use super::store::{MediaStoreError, media_db_path, open_read_connection, read_embedding_for, read_tag_matches};
use super::vector::{DedupCluster, SimilarImage, VectorStore, cache};

/// One OCR search hit: the matched image's path and a highlighted snippet of the
/// matched text (the "why matched" reason the results grid shows). Crosses the IPC
/// boundary as the `media_index_search_ocr` result, so it derives `Serialize` +
/// `specta::Type` (camelCase).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OcrHit {
    /// The matched image's absolute path.
    pub path: String,
    /// A snippet of the recognized text around the match, with `[` / `]` markers
    /// around the matched terms.
    pub snippet: String,
}

/// A read handle over a volume's `media.db`. Cheap to hold; the read connection is
/// opened lazily per call. A consumer keeps one per volume it searches.
pub struct MediaIndex {
    db_path: PathBuf,
}

impl MediaIndex {
    /// Open the read API for `volume_id` under `data_dir`. Never touches the DB
    /// until a read, so it's cheap and never fails on a missing file — a search of
    /// an un-enriched (or offline, purged) volume returns empty.
    pub fn open(data_dir: &std::path::Path, volume_id: &str) -> Self {
        Self::open_at(media_db_path(data_dir, volume_id))
    }

    /// Open the read API directly at a `media.db` path.
    pub fn open_at(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    /// Search the OCR text. Returns up to `limit` hits, each with a highlighted
    /// snippet. An empty/whitespace query, or a missing DB (offline / never
    /// enriched), returns empty rather than erroring.
    pub fn search_ocr(&self, query: &str, limit: usize) -> Result<Vec<OcrHit>, MediaStoreError> {
        let Some(match_query) = build_ocr_match_query(query) else {
            return Ok(Vec::new());
        };
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }
        let conn = open_read_connection(&self.db_path)?;
        // `snippet(media_ocr, 2, ...)`: column 2 is `text` (0 is the UNINDEXED `path`,
        // 1 the UNINDEXED `source`). `[`/`]` mark the matched terms; `…` is the
        // ellipsis; 12 is the max snippet token count. A path can have two rows (OCR +
        // folded tags); over-fetch then dedup by path in Rust, keeping the best-ranked.
        let mut stmt = conn.prepare(
            "SELECT path, snippet(media_ocr, 2, '[', ']', '…', 12) AS snip
             FROM media_ocr
             WHERE media_ocr MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![match_query, (limit * 2) as i64], |row| {
            Ok(OcrHit {
                path: row.get(0)?,
                snippet: row.get(1)?,
            })
        })?;
        let mut seen = std::collections::HashSet::new();
        let mut hits = Vec::new();
        for hit in rows {
            let hit = hit?;
            if seen.insert(hit.path.clone()) {
                hits.push(hit);
                if hits.len() >= limit {
                    break;
                }
            }
        }
        Ok(hits)
    }

    /// The number of enriched images stored for this volume (a `COUNT(*)` over
    /// `media_status`) — the minimal per-volume coverage surface. `0` for a
    /// missing/never-enriched DB.
    pub fn enriched_count(&self) -> Result<u64, MediaStoreError> {
        if !self.db_path.exists() {
            return Ok(0);
        }
        let conn = open_read_connection(&self.db_path)?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM media_status", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// The `k` images most similar to the one at `source_path` (by feature-print
    /// cosine), highest first, excluding the source itself. Reads the source's stored
    /// embedding, then brute-force ranks it against the volume's resident vector cache
    /// (loaded once, kept warm — plan § Query-time vector residency). An empty result
    /// when the source has no embedding, or the volume is un-enriched/offline.
    pub fn find_similar(&self, source_path: &str, k: usize) -> Result<Vec<SimilarImage>, MediaStoreError> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }
        let conn = open_read_connection(&self.db_path)?;
        let Some(query) = read_embedding_for(&conn, source_path)? else {
            return Ok(Vec::new());
        };
        drop(conn);
        let store = cache::get_or_load(&self.db_path);
        Ok(store.top_k(&query, k, Some(source_path)))
    }

    /// Group the volume's images into near-duplicate clusters (feature-print cosine at
    /// or above `threshold`). Reads the resident vector cache; empty for an
    /// un-enriched/offline volume.
    pub fn dedup_clusters(&self, threshold: f32) -> Vec<DedupCluster> {
        cache::get_or_load(&self.db_path).dedup_clusters(threshold)
    }

    /// The `k` images whose CLIP embeddings are closest (by cosine) to an
    /// already-encoded `query` text vector — natural-language text→image search (plan
    /// M3). Brute-force ranks over the volume's resident CLIP cache (loaded once, kept
    /// warm — plan § Query-time vector residency); the query text is tokenized +
    /// text-encoded by the command layer (the warm CLIP text tower), which keeps this
    /// method a pure vector query testable with deterministic vectors. Empty when the
    /// volume has no CLIP embeddings (no model installed, un-enriched, or offline).
    pub fn search_semantic(&self, query: &[f32], k: usize) -> Vec<SemanticHit> {
        cache::get_or_load_clip(&self.db_path)
            .top_k(query, k, None)
            .into_iter()
            .map(|hit| SemanticHit {
                path: hit.path,
                score: hit.score,
            })
            .collect()
    }

    /// The images tagged `label` at or above `min_score`, each with the matching
    /// tag's score, highest first — the tag-score filter. Empty for a
    /// missing/never-enriched DB.
    pub fn images_with_tag(&self, label: &str, min_score: f32) -> Result<Vec<TagHit>, MediaStoreError> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }
        // Vision's taxonomy labels are stored lowercase, so Unicode-lowercase the query
        // here to make tag search case-insensitive (a `Sky` query finds the `sky` tag).
        let folded = label.to_lowercase();
        let conn = open_read_connection(&self.db_path)?;
        Ok(read_tag_matches(&conn, &folded, min_score)?
            .into_iter()
            .map(|(path, score)| TagHit { path, score })
            .collect())
    }
}

/// One tag-filter hit: an image path and the confidence of the matched tag. Crosses
/// the IPC boundary (tag search is surfaced by a command), so it derives `Serialize`
/// + `specta::Type`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TagHit {
    /// The matched image's path.
    pub path: String,
    /// The matched tag's confidence in `[0.0, 1.0]`.
    pub score: f32,
}

/// One semantic-search hit: the matched image's path and its CLIP cosine similarity to
/// the text query. The grid renders these as snippet-less tiles with a "matched
/// description" reason (there's no OCR snippet — the match is on the whole-image CLIP
/// embedding). Crosses the IPC boundary, so it derives `Serialize` + `specta::Type`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SemanticHit {
    /// The matched image's path.
    pub path: String,
    /// CLIP cosine similarity to the query text in `[-1.0, 1.0]`.
    pub score: f32,
}

/// Build an fts5 `MATCH` query from raw user input by quoting each whitespace-
/// separated token, so every term is treated as a LITERAL (parens, colons, and
/// bareword operators like `AND`/`OR`/`NOT` can't be misparsed as query syntax).
/// Returns `None` for empty/whitespace-only input (the caller returns no hits).
///
/// An embedded double-quote is escaped by doubling it (fts5's own escaping), so a
/// token like `he"llo` stays a single literal phrase.
pub fn build_ocr_match_query(input: &str) -> Option<String> {
    let quoted: Vec<String> = input
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    if quoted.is_empty() {
        return None;
    }
    // Space-joined quoted terms are implicit-AND in fts5, so all terms must appear.
    Some(quoted.join(" "))
}

#[cfg(test)]
mod tests;
