//! `MediaIndex`: the consumable read API over a volume's `media.db`.
//!
//! Ported from `importance/read` (`ImportanceIndex`): the ONE way a consumer
//! (`search/`, and the Ask Cmdr / MCP `search_photos` tool via
//! `mcp::executor::photos`) reaches image-enrichment results (plan Decision 8). No
//! consumer takes a raw `rusqlite` dep on `media.db`; they call
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

use super::ann;
use super::store::{
    EmbeddingTable, MediaStoreError, media_db_path, open_read_connection, read_embedding_for, read_embeddings_for_ids,
    read_tag_matches,
};
use super::vector::{DedupCluster, SimilarImage, VectorStore, cache, cosine_f16};

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

/// How many paths one `IN (…)` clause binds. SQLite's default host-parameter ceiling is
/// 999, and a rename over a big folder blows past that, so [`MediaIndex::facts_for_paths`]
/// chunks rather than building one giant statement.
const PATH_CHUNK: usize = 900;

/// Everything the media index stored for ONE image — the lookup direction's row. Always
/// carries the requested `path`, so a caller gets an answer for every path it asked about.
///
/// `indexed == false` means "no `media_status` row": never enriched, or on a volume whose
/// `media.db` doesn't exist. `indexed == true` with `ocr_text: None` means the opposite:
/// enrichment ran and found no text. Keeping the two apart is the point — one says "ask
/// again later", the other says "there's nothing to find".
///
/// The OCR text is image-derived USER CONTENT (a passport scan's text IS the passport
/// number). Anything shipping this off-device needs the Ask Cmdr consent gate; see
/// `mcp/executor/image_facts.rs` and `docs/security.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageFacts {
    /// The path exactly as requested (not the stored spelling).
    pub path: String,
    /// Whether the media index has an enrichment row for this path at all.
    pub indexed: bool,
    /// The FULL recognized text, or `None` when the image has none (or isn't indexed).
    pub ocr_text: Option<String>,
    /// The Vision scene/object tags, highest confidence first.
    pub tags: Vec<ImageTag>,
}

/// One stored Vision tag: its taxonomy label and confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageTag {
    /// The taxonomy label, as stored (lowercase).
    pub label: String,
    /// The tag's confidence in `[0.0, 1.0]`.
    pub score: f32,
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
        // `snippet(media_ocr, 2, ...)`: column 2 is `text` (0 is the UNINDEXED `file_id`,
        // 1 the UNINDEXED `source`). `[`/`]` mark the matched terms; `…` is the
        // ellipsis; 12 is the max snippet token count. A file can have two rows (OCR +
        // folded tags); over-fetch then dedup by path in Rust, keeping the best-ranked. The
        // join maps the matched `file_id` back to its path (plan M4).
        let mut stmt = conn.prepare(
            "SELECT f.path, snippet(media_ocr, 2, '[', ']', '…', 12) AS snip
             FROM media_ocr JOIN media_file f ON f.id = media_ocr.file_id
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
    /// M3). The query text is tokenized + text-encoded by the command layer (the warm
    /// CLIP text tower), which keeps this method a pure vector query testable with
    /// deterministic vectors. Empty when the volume has no CLIP embeddings (no model
    /// installed, un-enriched, or offline).
    ///
    /// Below [`ann::ANN_MIN_VECTORS`] stored vectors this is the exact brute-force
    /// scan over the resident CLIP cache; at or above it, the per-volume usearch
    /// index answers (mmap view, over-fetch + exact re-rank — plan M6), and any
    /// unusable index falls back to the exact scan while a background rebuild heals
    /// it. Callers never see the difference except in latency.
    pub fn search_semantic(&self, query: &[f32], k: usize) -> Vec<SemanticHit> {
        self.search_semantic_with_threshold(query, k, ann::ANN_MIN_VECTORS)
    }

    /// [`Self::search_semantic`] with an explicit ANN threshold (tests exercise the
    /// ANN route with small corpora; production always passes
    /// [`ann::ANN_MIN_VECTORS`]).
    pub(crate) fn search_semantic_with_threshold(&self, query: &[f32], k: usize, threshold: usize) -> Vec<SemanticHit> {
        if k == 0 || query.is_empty() {
            return Vec::new();
        }
        let space = ann::AnnSpace::Clip;
        if let ann::cache::Route::Ann(handle) =
            ann::cache::route(&self.db_path, space, threshold, space.current_model_id())
        {
            match self.search_semantic_ann(&handle, query, k) {
                Ok(hits) => return hits,
                Err(e) => {
                    // A query-time engine failure demotes to the exact scan; the next
                    // route decision (post-invalidate) re-checks the index's health.
                    log::warn!(
                        target: "media_index",
                        "ann semantic search failed for {} ({e}); brute-force fallback",
                        self.db_path.display()
                    );
                }
            }
        }
        self.search_semantic_brute_force(query, k)
    }

    /// The exact path: brute-force cosine over the resident CLIP cache.
    fn search_semantic_brute_force(&self, query: &[f32], k: usize) -> Vec<SemanticHit> {
        cache::get_or_load_clip(&self.db_path)
            .top_k(query, k, None)
            .into_iter()
            .map(|hit| SemanticHit {
                path: hit.path,
                score: hit.score,
            })
            .collect()
    }

    /// The ANN path (plan M6): over-fetch `k ×` [`ann::ANN_OVERFETCH_FACTOR`]
    /// approximate candidates from the HNSW view, then re-rank them EXACTLY — read
    /// each candidate's stored f16 vector and CURRENT path from the DB and score
    /// with the same `cosine_f16` the brute-force scan uses — and return the top
    /// `k`. The exact re-rank keeps result ORDERING at brute-force quality even
    /// when HNSW recall dips, and the DB join both follows renames and drops ghost
    /// keys (a candidate whose row is gone yields no row and falls out).
    fn search_semantic_ann(
        &self,
        handle: &ann::cache::AnnHandle,
        query: &[f32],
        k: usize,
    ) -> Result<Vec<SemanticHit>, MediaStoreError> {
        if query.len() != handle.dims {
            // A query from a different embedding world (model transition edge); the
            // exact scan degrades gracefully (length-mismatched vectors score 0).
            return Err(MediaStoreError::Io(std::io::Error::other("ann query dims mismatch")));
        }
        let fetch = k.saturating_mul(ann::ANN_OVERFETCH_FACTOR).max(k);
        let matches = handle
            .index
            .search(query, fetch)
            .map_err(|e| MediaStoreError::Io(std::io::Error::other(e.to_string())))?;
        if matches.keys.is_empty() {
            return Ok(Vec::new());
        }
        let conn = open_read_connection(&self.db_path)?;
        let candidates = read_embeddings_for_ids(&conn, EmbeddingTable::Clip, &matches.keys)?;
        let mut hits: Vec<SemanticHit> = candidates
            .into_iter()
            .map(|(path, vector)| SemanticHit {
                score: cosine_f16(query, &vector),
                path,
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
        });
        hits.truncate(k);
        Ok(hits)
    }

    /// The stored image facts for paths the caller ALREADY has — the lookup direction,
    /// the mirror of the query-direction searches above. Returns exactly one
    /// [`ImageFacts`] per requested path, in request order, so a never-enriched file is
    /// representable ("not indexed yet") rather than silently dropped. A missing DB
    /// (never enriched, offline and purged) answers every path as not-indexed rather
    /// than erroring, matching [`Self::search_ocr`]'s empty-not-error convention.
    ///
    /// Unlike `search_ocr`, this returns the FULL stored OCR text, not a snippet: the
    /// caller is a model reasoning over what's in the image (naming a file after its
    /// contents), not a UI highlighting a match.
    pub fn facts_for_paths(&self, paths: &[&str]) -> Result<Vec<ImageFacts>, MediaStoreError> {
        let mut facts: Vec<ImageFacts> = paths
            .iter()
            .map(|p| ImageFacts {
                path: (*p).to_string(),
                indexed: false,
                ocr_text: None,
                tags: Vec::new(),
            })
            .collect();
        if facts.is_empty() || !self.db_path.exists() {
            return Ok(facts);
        }
        let mut by_path: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for (i, p) in paths.iter().enumerate() {
            by_path.entry(p).or_insert(i);
        }

        let conn = open_read_connection(&self.db_path)?;
        for chunk in paths.chunks(PATH_CHUNK) {
            let placeholders = std::iter::repeat_n("?", chunk.len()).collect::<Vec<_>>().join(",");
            let params = rusqlite::params_from_iter(chunk.iter());

            // Enrichment presence: a `media_status` row is what makes "indexed, no text
            // found" distinguishable from "never enriched". Joined to `media_file` for the
            // path (plan M4).
            let mut stmt = conn.prepare(&format!(
                "SELECT f.path FROM media_status s JOIN media_file f ON f.id = s.file_id
                 WHERE f.path IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(params, |row| row.get::<_, String>(0))?;
            for row in rows {
                if let Some(&i) = by_path.get(row?.as_str()) {
                    facts[i].indexed = true;
                }
            }

            // ONLY the `source = 'ocr'` rows. `media_ocr` also holds a `source = 'tag'`
            // row per path (the space-joined tag labels folded in for keyword search), so
            // an unfiltered read would hand the caller tag labels dressed up as OCR text.
            let mut stmt = conn.prepare(&format!(
                "SELECT f.path, o.text FROM media_ocr o JOIN media_file f ON f.id = o.file_id
                 WHERE o.source = 'ocr' AND f.path IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (path, text) = row?;
                if let Some(&i) = by_path.get(path.as_str())
                    && !text.is_empty()
                {
                    facts[i].ocr_text = Some(text);
                }
            }

            // Tags come from the STRUCTURED `media_tags` table, so each keeps its own
            // label and confidence instead of the folded, score-less FTS row.
            let mut stmt = conn.prepare(&format!(
                "SELECT f.path, t.label, t.score FROM media_tags t JOIN media_file f ON f.id = t.file_id
                 WHERE f.path IN ({placeholders}) ORDER BY t.score DESC, t.label ASC"
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)? as f32,
                ))
            })?;
            for row in rows {
                let (path, label, score) = row?;
                if let Some(&i) = by_path.get(path.as_str()) {
                    facts[i].tags.push(ImageTag { label, score });
                }
            }
        }
        Ok(facts)
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
