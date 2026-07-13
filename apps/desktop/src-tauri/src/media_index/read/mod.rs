//! `MediaIndex`: the consumable read API over a volume's `media.db`.
//!
//! Ported from `importance/read` (`ImportanceIndex`): the ONE way a consumer
//! (`search/`, later the agent + MCP) reaches image-enrichment results (plan
//! Decision 8). No consumer takes a raw `rusqlite` dep on `media.db`; they call
//! this. It owns a `platform_case`-registered read connection and reads the DB
//! directly, so it answers OFFLINE from `media.db` after the volume unmounts.
//!
//! ## FTS query building (an M1 TDD target)
//!
//! Raw user input must NEVER be fed into `... MATCH ?` — ordinary filename/text
//! fragments (`report(v2)`, `foo:bar`, a bareword `AND`/`OR`) throw an fts5 syntax
//! error, and binding doesn't help (the string is parsed as query syntax). Same
//! gotcha as `agent/store`'s `sanitize_fts_query`; [`build_ocr_match_query`] is our
//! sanitizer — it quotes each whitespace token so every term is a literal.

use std::path::PathBuf;

use super::store::{MediaStoreError, media_db_path, open_read_connection};

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
        // `snippet(media_ocr, 1, ...)`: column 1 is `text` (0 is the UNINDEXED
        // `path`). `[`/`]` mark the matched terms; `…` is the ellipsis; 12 is the
        // max snippet token count.
        let mut stmt = conn.prepare(
            "SELECT path, snippet(media_ocr, 1, '[', ']', '…', 12) AS snip
             FROM media_ocr
             WHERE media_ocr MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![match_query, limit as i64], |row| {
            Ok(OcrHit {
                path: row.get(0)?,
                snippet: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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
