//! The two write paths that RECORD enrichment: the Vision upsert (the status row plus OCR
//! text, tags, and the feature print) and the independent CLIP upsert (stamp plus
//! embedding). Each is one transaction; see [`super`] for the two-part staleness rationale.

use rusqlite::Connection;

use super::UpsertAnalysis;
use crate::media_index::store::{MediaStatusRow, MediaStoreError, encode_embedding};

/// Upsert one status row and replace its searchable text, tags, and embedding in one
/// transaction. The prior text/tags/embedding rows are always cleared first (a
/// re-enrichment must not leave stale rows), then re-inserted only when `analysis` is
/// `Some` (a success). The OCR FTS row is written only for non-empty text; the folded
/// tag FTS row + structured `media_tags` only for non-empty tags; the embedding only
/// when present.
///
/// Returns whether this upsert INSERTED a new `media_status` row (no prior row for the
/// path) vs updated an existing one — a cheap PK existence check inside the same
/// transaction, so the caller can bump the accounted aggregate only on a genuinely-new
/// completion (a re-enrich or `done`↔`failed` transition leaves the count unchanged).
pub(super) fn apply_upsert(
    conn: &mut Connection,
    row: &MediaStatusRow,
    analysis: Option<&UpsertAnalysis>,
) -> Result<bool, MediaStoreError> {
    let tx = conn.transaction()?;
    // Resolve the path to its `media_file` id, creating the identity row if it's new. A
    // brand-new `media_file` row means a genuinely-new image (media_file ⇔ media_status
    // 1:1: they're written together and deleted together), which the caller uses to bump
    // the accounted aggregate only on a first completion.
    let (file_id, inserted) = resolve_or_create_file_id(&tx, &row.path)?;
    {
        tx.execute(
            "INSERT INTO media_status (file_id, mtime, size, media_kind, state, engine_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(file_id) DO UPDATE SET
                mtime = ?2, size = ?3, media_kind = ?4, state = ?5, engine_version = ?6",
            rusqlite::params![
                file_id,
                row.mtime.map(|v| v as i64),
                row.size.map(|v| v as i64),
                row.media_kind.as_token(),
                row.state.as_token(),
                row.engine_version,
            ],
        )?;
        // Clear every prior derived row for this file (one `WHERE file_id = ?` each).
        tx.execute("DELETE FROM media_ocr WHERE file_id = ?1", rusqlite::params![file_id])?;
        tx.execute("DELETE FROM media_tags WHERE file_id = ?1", rusqlite::params![file_id])?;
        tx.execute(
            "DELETE FROM media_embedding WHERE file_id = ?1",
            rusqlite::params![file_id],
        )?;

        if let Some(analysis) = analysis {
            if !analysis.ocr_text.is_empty() {
                tx.execute(
                    "INSERT INTO media_ocr (file_id, source, text) VALUES (?1, 'ocr', ?2)",
                    rusqlite::params![file_id, analysis.ocr_text],
                )?;
            }
            if !analysis.tags.is_empty() {
                // Fold the tag labels into the FTS as one searchable row, and store
                // the structured (label, score) rows for tag-score filtering.
                let labels = analysis
                    .tags
                    .iter()
                    .map(|t| t.label.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                tx.execute(
                    "INSERT INTO media_ocr (file_id, source, text) VALUES (?1, 'tag', ?2)",
                    rusqlite::params![file_id, labels],
                )?;
                let mut ins_tag =
                    tx.prepare_cached("INSERT INTO media_tags (file_id, label, score) VALUES (?1, ?2, ?3)")?;
                for tag in &analysis.tags {
                    ins_tag.execute(rusqlite::params![file_id, tag.label, tag.score as f64])?;
                }
            }
            if let Some(vector) = &analysis.embedding {
                tx.execute(
                    "INSERT INTO media_embedding (file_id, dims, vector) VALUES (?1, ?2, ?3)",
                    rusqlite::params![file_id, vector.len() as i64, encode_embedding(vector)],
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(inserted)
}

/// Resolve `path` to its `media_file` id, inserting a new identity row when the path is not
/// yet known. Returns `(file_id, inserted)` where `inserted` is `true` only when a fresh row
/// was created — the "genuinely-new image" signal the accounted aggregate rides on.
fn resolve_or_create_file_id(tx: &rusqlite::Transaction<'_>, path: &str) -> Result<(i64, bool), MediaStoreError> {
    if let Some(id) = lookup_file_id(tx, path)? {
        return Ok((id, false));
    }
    tx.execute("INSERT INTO media_file (path) VALUES (?1)", rusqlite::params![path])?;
    Ok((tx.last_insert_rowid(), true))
}

/// Look up an existing `media_file` id for `path`, or `None` if the path is unknown.
pub(super) fn lookup_file_id(conn: &Connection, path: &str) -> Result<Option<i64>, MediaStoreError> {
    let mut stmt = conn.prepare_cached("SELECT id FROM media_file WHERE path = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![path], |r| r.get::<_, i64>(0))?;
    match rows.next() {
        Some(Ok(id)) => Ok(Some(id)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// What a CLIP upsert did — the writer buffers the matching ANN op off this (plan M6).
pub(super) enum ClipWrite {
    /// The embedding row was replaced with a fresh vector (ANN: upsert the key).
    Stored { file_id: i64 },
    /// The row was stamped but the embedding cleared — a CLIP failure/skip (ANN:
    /// remove the key so a ghost vector can't linger).
    Cleared { file_id: i64 },
    /// No `media_status` row for the path; nothing was written.
    NoRow,
}

/// Stamp `path`'s `media_status.clip_stamp` and replace its `media_clip_embedding` in one
/// transaction, touching NO Vision column or table. If no `media_status` row exists (CLIP
/// only runs when Vision is current, so this shouldn't happen) the embedding write is
/// skipped rather than orphaned.
pub(super) fn apply_upsert_clip(
    conn: &mut Connection,
    path: &str,
    clip_stamp: &str,
    embedding: Option<&[f32]>,
) -> Result<ClipWrite, MediaStoreError> {
    let tx = conn.transaction()?;
    let mut write = ClipWrite::NoRow;
    {
        // CLIP is eligible only for a path Vision already covered, so its `media_file` +
        // `media_status` rows exist. A missing row (shouldn't happen) skips the write
        // rather than orphaning an embedding.
        if let Some(file_id) = lookup_file_id(&tx, path)? {
            let updated = tx.execute(
                "UPDATE media_status SET clip_stamp = ?2 WHERE file_id = ?1",
                rusqlite::params![file_id, clip_stamp],
            )?;
            tx.execute(
                "DELETE FROM media_clip_embedding WHERE file_id = ?1",
                rusqlite::params![file_id],
            )?;
            write = ClipWrite::Cleared { file_id };
            if updated > 0
                && let Some(vector) = embedding
            {
                tx.execute(
                    "INSERT INTO media_clip_embedding (file_id, dims, vector) VALUES (?1, ?2, ?3)",
                    rusqlite::params![file_id, vector.len() as i64, encode_embedding(vector)],
                )?;
                write = ClipWrite::Stored { file_id };
            }
        }
    }
    tx.commit()?;
    Ok(write)
}
