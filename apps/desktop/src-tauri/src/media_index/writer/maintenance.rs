//! Keeping the DB tidy: the O(1) rename, `VACUUM`, the whole-volume purge, and the WAL
//! checkpoint. None of them writes enrichment data; they move or reclaim what's there.

use std::time::Duration;

use rusqlite::Connection;

use super::upsert::lookup_file_id;
use crate::media_index::store::MediaStoreError;
use crate::pluralize::pluralize;

/// Move a stored image's identity row from `old` to `new` in one transaction — the O(1)
/// rename the integer-id keying buys (plan M4): a single `UPDATE media_file.path`, and every
/// `file_id`-keyed child (status, OCR, tags, embeddings) follows untouched. Returns whether a
/// row moved: `false` when `old` has no row, or `new` is already a distinct enriched path
/// (the `UNIQUE(path)` constraint would reject the update, so it's a no-op, not a crash).
pub(super) fn apply_rename(conn: &mut Connection, old: &str, new: &str) -> Result<bool, MediaStoreError> {
    let tx = conn.transaction()?;
    // Rename only when `old` has a row AND `new` is free (a taken `new` would violate the
    // `UNIQUE(path)` constraint, so skip it rather than error).
    let moved = if let Some(old_id) = lookup_file_id(&tx, old)?
        && lookup_file_id(&tx, new)?.is_none()
    {
        tx.execute(
            "UPDATE media_file SET path = ?2 WHERE id = ?1",
            rusqlite::params![old_id, new],
        )?;
        true
    } else {
        false
    };
    tx.commit()?;
    Ok(moved)
}

/// `VACUUM` the DB (reclaim free pages; can't run inside a transaction, so it's its own
/// statement).
pub(super) fn apply_vacuum(conn: &Connection) -> Result<(), MediaStoreError> {
    conn.execute_batch("VACUUM")?;
    Ok(())
}

/// Drop every derived row. Schema stays.
pub(super) fn apply_purge(conn: &Connection) -> Result<(), MediaStoreError> {
    conn.execute_batch(
        "DELETE FROM media_status; DELETE FROM media_ocr; DELETE FROM media_tags; DELETE FROM media_embedding; DELETE FROM media_clip_embedding; DELETE FROM media_file;",
    )?;
    Ok(())
}

/// TRUNCATE the WAL file so its high-water mark doesn't sit on disk. Mirrors
/// `importance/writer.rs::run_wal_checkpoint` (this whole module is a port of
/// `importance/`): SQLite's default PASSIVE `wal_autocheckpoint` copies frames back
/// into the main DB but reuses the WAL file in place and never shrinks it; only an
/// explicit TRUNCATE reclaims the space. An enrichment pass upserts a row per image,
/// so without this the WAL creeps up in place (plan M9).
///
/// Runs on the writer thread's own connection in autocommit: every message commits its
/// transaction before the loop reads the next, so `wal_checkpoint(TRUNCATE)` (which
/// SQLite refuses inside a transaction) is always safe here.
///
/// A long-lived reader snapshot can block the truncate. We give readers a short, bounded
/// grace (mirroring the index writer's ~250 ms cap in `indexing/writer/maintenance.rs`)
/// then degrade to PASSIVE (`busy = 1`): the frames still checkpoint into the main DB,
/// the file just doesn't shrink this time, and the next pass retries. No retry loop.
pub(super) fn run_wal_checkpoint(conn: &Connection) {
    // A short busy timeout around the truncate: without it the connection's default 5 s
    // timeout (set in `store/connection.rs`) would stall the writer thread (and every
    // write queued behind it) waiting a reader out. Restored right after.
    let _ = conn.busy_timeout(Duration::from_millis(250));
    let result: rusqlite::Result<(i64, i64, i64)> = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    });
    let _ = conn.busy_timeout(Duration::from_millis(5000));
    match result {
        Ok((0, log_size, checkpointed)) => {
            log::debug!(target: "media_index", "wal_checkpoint TRUNCATE done ({checkpointed} of {})", pluralize(log_size as u64, "frame"));
        }
        Ok((_, log_size, checkpointed)) => {
            log::debug!(target: "media_index", "wal_checkpoint partial ({checkpointed} of {}, blocked by readers)", pluralize(log_size as u64, "frame"));
        }
        Err(e) => {
            log::warn!(target: "media_index", "wal_checkpoint failed: {e}");
        }
    }
}
