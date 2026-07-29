//! Every write path that DELETES rows: the deletion-driven GC, the two user-explicit
//! prunes (an explicit path list and a folder prefix), and the delete-CLIP-model reclaim.
//!
//! GC and the explicit prune share ONE delete primitive, so both report the same "rows
//! that actually left" set (the accounted decrement + the ANN keys ride on it).

use rusqlite::Connection;

use crate::media_index::store::MediaStoreError;

/// One row a delete actually removed: its path (the accounted decrement) and its
/// `media_file` id (the ANN index key to remove — plan M6).
pub(super) struct DeletedRow {
    pub(super) path: String,
    pub(super) file_id: i64,
}

/// Delete the status + text + tag + embedding + clip-embedding rows for each path in one
/// transaction. Returns the rows whose `media_status` row actually existed and was
/// deleted, so the caller decrements the accounted aggregate once per genuinely-removed
/// row (a GC of a path with no row moves nothing) and removes the matching ANN keys.
pub(super) fn apply_gc(conn: &mut Connection, paths: &[String]) -> Result<Vec<DeletedRow>, MediaStoreError> {
    delete_rows_for_paths(conn, paths)
}

/// Prune the four tables for an explicit path list in one transaction (the same delete
/// primitive GC uses, reused for the user-explicit prune). Returns the rows actually
/// removed, so the count matches the images the user removed and the caller can
/// decrement the accounted aggregate (and the ANN keys) per removed row.
pub(super) fn apply_prune_paths(conn: &mut Connection, paths: &[String]) -> Result<Vec<DeletedRow>, MediaStoreError> {
    delete_rows_for_paths(conn, paths)
}

/// Delete every table's rows for each path in one transaction, returning the rows whose
/// `media_status` row existed (so `delete_status.execute` reported a removal). Shared by
/// GC and the explicit prune so both report the SAME "rows that actually left" set.
fn delete_rows_for_paths(conn: &mut Connection, paths: &[String]) -> Result<Vec<DeletedRow>, MediaStoreError> {
    let tx = conn.transaction()?;
    let mut deleted = Vec::new();
    {
        let mut find = tx.prepare_cached("SELECT id FROM media_file WHERE path = ?1")?;
        let mut del_status = tx.prepare_cached("DELETE FROM media_status WHERE file_id = ?1")?;
        let mut del_ocr = tx.prepare_cached("DELETE FROM media_ocr WHERE file_id = ?1")?;
        let mut del_tags = tx.prepare_cached("DELETE FROM media_tags WHERE file_id = ?1")?;
        let mut del_emb = tx.prepare_cached("DELETE FROM media_embedding WHERE file_id = ?1")?;
        let mut del_clip = tx.prepare_cached("DELETE FROM media_clip_embedding WHERE file_id = ?1")?;
        let mut del_file = tx.prepare_cached("DELETE FROM media_file WHERE id = ?1")?;
        for path in paths {
            // A path with no `media_file` row was never enriched: nothing to remove, and
            // it must NOT count toward the accounted decrement.
            let Some(file_id) = find
                .query_map(rusqlite::params![path], |r| r.get::<_, i64>(0))?
                .next()
                .transpose()?
            else {
                continue;
            };
            del_status.execute(rusqlite::params![file_id])?;
            del_ocr.execute(rusqlite::params![file_id])?;
            del_tags.execute(rusqlite::params![file_id])?;
            del_emb.execute(rusqlite::params![file_id])?;
            del_clip.execute(rusqlite::params![file_id])?;
            del_file.execute(rusqlite::params![file_id])?;
            deleted.push(DeletedRow {
                path: path.clone(),
                file_id,
            });
        }
    }
    tx.commit()?;
    Ok(deleted)
}

/// Prune every row at or under a folder `prefix`. The doomed set is derived on the
/// writer thread from the CURRENT committed `media_status` paths, filtered by the SAME
/// trailing-slash-safe [`path_is_within`](crate::media_index::network::config::path_is_within)
/// the exclusion veto uses (so the delete set can't drift from what the veto forbids), then
/// deleted via [`apply_prune_paths`]. An empty `prefix` matches every path (the whole
/// volume — the user excluded the mount root). Returns the paths actually removed (for
/// the accounted decrement + the delete count).
pub(super) fn apply_prune_prefix(conn: &mut Connection, prefix: &str) -> Result<Vec<DeletedRow>, MediaStoreError> {
    let doomed: Vec<String> = {
        let mut stmt = conn.prepare_cached("SELECT path FROM media_file")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for path in rows {
            let path = path?;
            if crate::media_index::network::config::path_is_within(&path, prefix) {
                out.push(path);
            }
        }
        out
    };
    apply_prune_paths(conn, &doomed)
}

/// Delete every `media_clip_embedding` row and reset every `media_status.clip_stamp` to
/// empty (no model), in one transaction. Returns the embedding rows removed. Resetting the
/// stamp is what makes a later re-install re-embed (`needs_clip` sees `'' != model_stamp`).
/// Touches NO Vision column or table — deleting the CLIP model must not re-run OCR/tags.
pub(super) fn apply_prune_all_clip(conn: &mut Connection) -> Result<usize, MediaStoreError> {
    let tx = conn.transaction()?;
    let removed = tx.execute("DELETE FROM media_clip_embedding", [])?;
    tx.execute("UPDATE media_status SET clip_stamp = '' WHERE clip_stamp != ''", [])?;
    tx.commit()?;
    Ok(removed)
}
