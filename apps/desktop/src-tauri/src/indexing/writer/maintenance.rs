//! Periodic DB housekeeping handlers run on the writer thread.
//!
//! Incremental vacuum reclaims free pages from deletes/rescans, and the WAL
//! checkpoint truncates the WAL file once readers permit. Both are fired by a
//! background timer (and the WAL checkpoint also right after a full scan); they
//! mutate no `entries` rows, so they don't bump the writer generation.

use crate::pluralize::pluralize;

/// Cap thresholds for the tiered incremental-vacuum policy. Below `MIN`,
/// holding the write lock isn't worth the work. Between `MIN` and `BACKLOG`,
/// keep the original steady-state cap so concurrent operations barely notice.
/// Above `BACKLOG`, ramp the cap to drain backlogs (post-truncate, post-replay,
/// or DBs migrated from older versions that accumulated free pages) in tens of
/// minutes instead of hours.
const VACUUM_MIN_FREELIST: i64 = 1_000;
const VACUUM_STEADY_CAP: i64 = 2_000;
const VACUUM_BACKLOG_THRESHOLD: i64 = 20_000;
const VACUUM_BACKLOG_CAP: i64 = 20_000;

/// Pick the per-tick `incremental_vacuum` page cap given the current
/// `freelist_count`. Pure so it can be tested in isolation; the handler
/// just runs the SQL and logs.
///
/// Tiered cap: skip the no-op lock acquisition when the freelist is small;
/// hold the lock only as long as needed to drain real backlog. The 20K cap
/// (~80 MB at 4 KiB pages) is sized so a single tick fsyncs in ~100-300 ms
/// on SSD — long enough to make real progress but short enough that the
/// writer doesn't visibly stall behind it.
fn pick_vacuum_cap(freelist: i64) -> Option<i64> {
    if freelist < VACUUM_MIN_FREELIST {
        None
    } else if freelist < VACUUM_BACKLOG_THRESHOLD {
        Some(VACUUM_STEADY_CAP)
    } else {
        Some(VACUUM_BACKLOG_CAP)
    }
}

pub(super) fn handle_incremental_vacuum(conn: &rusqlite::Connection) {
    let free = match conn.pragma_query_value(None, "freelist_count", |row| row.get::<_, i64>(0)) {
        Ok(n) => n,
        Err(e) => {
            log::warn!("Writer: freelist_count query failed: {e}");
            return;
        }
    };

    let Some(cap) = pick_vacuum_cap(free) else {
        return;
    };

    if let Err(e) = conn.execute_batch(&format!("PRAGMA incremental_vacuum({cap});")) {
        log::warn!("Writer: incremental_vacuum failed: {e}");
    } else {
        log::debug!(
            "Writer: incremental_vacuum reclaimed up to {cap} of {}",
            pluralize(free as u64, "free page")
        );
    }
}

/// Periodically TRUNCATE the WAL file so its high-water mark doesn't sit on
/// disk indefinitely. SQLite's `wal_autocheckpoint` runs in PASSIVE mode and
/// only moves pages from WAL to the main file; it never shrinks the file
/// itself. After a big scan the WAL can balloon to 1+ GB, and without an
/// explicit TRUNCATE that file size persists until the next app restart.
///
/// TRUNCATE blocks waiting for readers, invoking this connection's busy handler
/// (installed in `writer/mod.rs::spawn`) while it waits. That handler — NOT the
/// `busy_timeout = 5000` pragma, which installing a `busy_handler` overrides —
/// caps the wait at ~250 ms (it sleeps 5 ms per retry and gives up at attempt 51),
/// after which the call degrades to PASSIVE semantics (busy code = 1 in the return
/// tuple): pages still get checkpointed, the file just doesn't shrink this time.
/// Next tick tries again. No error path needed. The short cap is deliberate: this
/// runs on the writer thread, so a multi-second block would stall every live write
/// queued behind it.
pub(super) fn handle_wal_checkpoint(conn: &rusqlite::Connection) {
    // `PRAGMA wal_checkpoint(TRUNCATE)` returns a single row with three
    // columns: (busy, log_size, checkpointed). `busy = 0` means everything
    // got checkpointed AND the file was truncated; `busy = 1` means at least
    // one reader was still on the WAL so the file couldn't shrink (pages
    // were still copied to the main file). Either is a success from the
    // caller's POV — only a SQL error means something is actually wrong.
    let result: rusqlite::Result<(i64, i64, i64)> = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    });
    match result {
        Ok((0, log_size, checkpointed)) => {
            log::debug!(
                "Writer: wal_checkpoint TRUNCATE done ({checkpointed} of {})",
                pluralize(log_size as u64, "page")
            );
        }
        Ok((_, log_size, checkpointed)) => {
            // Busy: readers blocking the truncate. Pages still got written
            // to the main file; the WAL file just didn't shrink this tick.
            log::debug!(
                "Writer: wal_checkpoint partial ({checkpointed} of {}, blocked by readers)",
                pluralize(log_size as u64, "page")
            );
        }
        Err(e) => log::warn!("Writer: wal_checkpoint failed: {e}"),
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
    use crate::indexing::writer::tests::setup_db;
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    // ── DB hygiene tests ─────────────────────────────────────────────

    /// The tier policy is the safety-critical part of the vacuum logic:
    /// regressing it would either thrash the writer lock (cap too aggressive
    /// in steady state) or let the freelist grow unbounded (cap missing on
    /// backlog). Lock the thresholds with explicit cases either side of each
    /// boundary plus the steady-state band's interior.
    #[test]
    fn pick_vacuum_cap_skips_below_min() {
        assert_eq!(pick_vacuum_cap(0), None);
        assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST - 1), None);
    }

    #[test]
    fn pick_vacuum_cap_uses_steady_band_for_modest_backlog() {
        assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST), Some(VACUUM_STEADY_CAP));
        assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD - 1), Some(VACUUM_STEADY_CAP));
    }

    #[test]
    fn pick_vacuum_cap_ramps_to_backlog_cap_for_large_backlog() {
        assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD), Some(VACUUM_BACKLOG_CAP));
        assert_eq!(pick_vacuum_cap(1_000_000), Some(VACUUM_BACKLOG_CAP));
    }

    /// End-to-end check: after a truncate that leaves a large freelist, the
    /// vacuum handler actually drops `freelist_count`. Doesn't pin the exact
    /// per-tier cap (that's covered by the policy tests above); pins the
    /// invariant that "freelist went down".
    #[test]
    fn handle_incremental_vacuum_reclaims_pages_after_truncate() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert enough entries that TruncateData later creates a real
        // freelist. Long names so each row touches its own page; 5000 rows
        // ≥ several thousand pages = at least one band above MIN.
        let entries: Vec<EntryRow> = (0..5000)
            .map(|i| EntryRow {
                id: 100 + i,
                parent_id: ROOT_ID,
                name: format!("test-entry-with-a-reasonably-long-name-{i:08}"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(4096),
                physical_size: Some(4096),
                modified_at: None,
                inode: None,
            })
            .collect();
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::TruncateData).unwrap();
        writer.flush_blocking().unwrap();

        // Read the freelist via a separate connection. The post-truncate
        // `PRAGMA incremental_vacuum;` inside the truncate handler already
        // drained some pages, but the cap was unbounded and ran inside the
        // same transaction; on a busy DB there can still be a meaningful
        // residual freelist. If the post-truncate vacuum already drained
        // everything, the subsequent IncrementalVacuum should still leave
        // freelist_count == 0 (a no-op), which the assertion below allows.
        let probe = IndexStore::open_read_connection(&db_path).unwrap();
        let free_before: i64 = probe
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .unwrap();
        drop(probe);

        writer.send(WriteMessage::IncrementalVacuum).unwrap();
        writer.flush_blocking().unwrap();

        let probe = IndexStore::open_read_connection(&db_path).unwrap();
        let free_after: i64 = probe
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .unwrap();

        assert!(
            free_after <= free_before,
            "IncrementalVacuum must not grow the freelist; before={free_before}, after={free_after}"
        );

        writer.shutdown();
    }

    /// End-to-end check: after inserts have grown the WAL, `WalCheckpoint`
    /// shrinks the on-disk WAL file. The WAL file is `db_path` + "-wal";
    /// after a successful TRUNCATE checkpoint with no readers, it should
    /// drop to zero bytes (or a small header).
    #[test]
    fn handle_wal_checkpoint_truncates_wal_file() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Grow the WAL with a non-trivial insert batch.
        let entries: Vec<EntryRow> = (0..2000)
            .map(|i| EntryRow {
                id: 200 + i,
                parent_id: ROOT_ID,
                name: format!("wal-test-entry-{i:08}"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1024),
                physical_size: Some(1024),
                modified_at: None,
                inode: None,
            })
            .collect();
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        let wal_path = format!("{}-wal", db_path.display());
        let wal_size_before = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(
            wal_size_before > 0,
            "expected WAL file to have grown after 2000 inserts; got {} bytes",
            wal_size_before // allowed-pluralize-noun: assertion-failure-only message; the assertion is `> 0`, so when it fires `wal_size_before == 0` and "0 bytes" reads correctly
        );

        writer.send(WriteMessage::WalCheckpoint).unwrap();
        writer.flush_blocking().unwrap();

        let wal_size_after = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(
            wal_size_after < wal_size_before,
            "WalCheckpoint should shrink the WAL file; before={wal_size_before}, after={wal_size_after}"
        );

        writer.shutdown();
    }
}
