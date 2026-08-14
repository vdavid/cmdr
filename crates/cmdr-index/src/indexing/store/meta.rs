//! `IndexStore` meta-table and epoch helpers, plus whole-index counts and
//! `clear_all`. Pure code movement from the former monolithic `store.rs`.

use super::{CURRENT_EPOCH_KEY, IndexStore, IndexStoreError, LEDGER_HEAL_KEY, UnreadableCause, with_savepoint};
use rusqlite::{Connection, OptionalExtension, params};

#[cfg(test)]
use super::{ROOT_ID, reconstruct_path_from_map, reset_schema};

impl IndexStore {
    /// Set a meta key-value pair.
    pub fn update_meta(conn: &Connection, key: &str, value: &str) -> Result<(), IndexStoreError> {
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Delete a meta key (no-op if absent).
    pub fn delete_meta(conn: &Connection, key: &str) -> Result<(), IndexStoreError> {
        conn.execute("DELETE FROM meta WHERE key = ?1", params![key])?;
        Ok(())
    }

    /// Get a single meta value by key. Read by the search loader to recover a
    /// volume's `volume_path` (mount root) from its persisted index DB.
    pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>, IndexStoreError> {
        Self::read_meta_value(conn, key)
    }

    /// Whether this DB's `dir_stats` aggregates are known-good (the
    /// `LEDGER_HEAL_KEY` meta key is present). The launch heal decision reads
    /// this to decide whether to arm the writer latch. Only presence matters;
    /// the stored value is a marker.
    pub fn ledger_heal_done(conn: &Connection) -> Result<bool, IndexStoreError> {
        Ok(Self::read_meta_value(conn, LEDGER_HEAL_KEY)?.is_some())
    }

    /// Mark the ledger paid: write the `LEDGER_HEAL_KEY` marker so a later launch
    /// skips the heal. Called from the writer's `ComputeAllAggregates` handler on
    /// success only (`set_heal_key_on_success`), so a failed rebuild leaves the
    /// key unset and re-heals next launch.
    pub fn mark_ledger_heal_done(conn: &Connection) -> Result<(), IndexStoreError> {
        Self::update_meta(conn, LEDGER_HEAL_KEY, "1")
    }

    /// Mark the ledger UNPAID: drop the `LEDGER_HEAL_KEY` marker because the
    /// aggregates are about to drift knowingly (a bulk walk suppressing per-entry
    /// propagation). It's durable on purpose: if the process dies mid-walk, the
    /// missing marker is what tells the next launch to rebuild. Called from the
    /// writer's `MarkLedgerUnpaid` handler.
    pub fn clear_ledger_heal_done(conn: &Connection) -> Result<(), IndexStoreError> {
        Self::delete_meta(conn, LEDGER_HEAL_KEY)
    }

    /// Read the volume's `current_epoch` from `meta`.
    ///
    /// Absent (older / first-run DB) or unparseable ⇒ `1`: a volume with no
    /// recorded epoch behaves as "all current" rather than "all stale". See
    /// `CURRENT_EPOCH_KEY` and the "Honest sizes" model in `indexing/DETAILS.md`.
    pub fn read_current_epoch(conn: &Connection) -> Result<u64, IndexStoreError> {
        let raw = Self::read_meta_value(conn, CURRENT_EPOCH_KEY)?;
        Ok(raw.and_then(|v| v.parse::<u64>().ok()).unwrap_or(1))
    }

    /// Ensure `current_epoch` exists in `meta`, seeding it to `"1"` if absent.
    /// Returns the epoch after seeding. Idempotent: leaves an existing value
    /// untouched. Used by a scan at start so the first scan stamps epoch 1.
    pub fn seed_current_epoch(conn: &Connection) -> Result<u64, IndexStoreError> {
        if Self::read_meta_value(conn, CURRENT_EPOCH_KEY)?.is_none() {
            Self::update_meta(conn, CURRENT_EPOCH_KEY, "1")?;
            return Ok(1);
        }
        Self::read_current_epoch(conn)
    }

    /// Bump `current_epoch` by one and persist it, returning the new value.
    /// A continuity break (reconnect, watcher death, overflow, rescan) calls
    /// this; a scan/reconcile only *stamps* with the value, never bumps. Seeds
    /// to `1` first if absent, so the first bump yields `2`.
    pub fn bump_current_epoch(conn: &Connection) -> Result<u64, IndexStoreError> {
        let next = Self::read_current_epoch(conn)?.saturating_add(1);
        Self::update_meta(conn, CURRENT_EPOCH_KEY, &next.to_string())?;
        Ok(next)
    }

    /// Stamp a batch of directories' `listed_epoch` by primary key.
    ///
    /// PK-keyed `UPDATE` (no `platform_case` cost), chunked so a huge id list
    /// doesn't exceed SQLite's bound-parameter limit. Records "these dirs'
    /// direct contents were successfully listed at epoch E". A dir whose listing
    /// errored is never passed here, so it stays `listed_epoch = 0` (honest
    /// "unknown", distinct from a genuinely-empty `0 bytes`).
    pub fn mark_dirs_listed(conn: &Connection, ids: &[i64], epoch: u64) -> Result<(), IndexStoreError> {
        if ids.is_empty() {
            return Ok(());
        }
        // Stay well under SQLite's default 999-parameter ceiling (+1 for epoch).
        const CHUNK: usize = 900;
        with_savepoint(conn, "mark_dirs_listed", |conn| {
            for chunk in ids.chunks(CHUNK) {
                let placeholders: String = (0..chunk.len())
                    .map(|i| format!("?{}", i + 2))
                    .collect::<Vec<_>>()
                    .join(", ");
                // Clearing `unreadable_cause` here is what heals the mark: a
                // directory we just listed is, by definition, readable again
                // (Full Disk Access granted, a permission fixed, a mount back).
                let sql =
                    format!("UPDATE entries SET listed_epoch = ?1, unreadable_cause = 0 WHERE id IN ({placeholders})");
                let mut stmt = conn.prepare_cached(&sql)?;
                let mut values: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(chunk.len() + 1);
                let epoch_i = epoch as i64;
                values.push(&epoch_i as &dyn rusqlite::types::ToSql);
                for id in chunk {
                    values.push(id as &dyn rusqlite::types::ToSql);
                }
                stmt.execute(&*values)?;
            }
            Ok(())
        })
    }

    /// Mark a batch of directories as ones nothing is going to read into, with
    /// the reason; `None` clears the mark.
    ///
    /// Deliberately separate from `listed_epoch`: an unreadable directory was
    /// never listed, so it stays at `0` and its subtree keeps absorbing every
    /// ancestor's `min_subtree_epoch` to `0` — the sizes stay honest. What the
    /// mark buys is that the coverage frontier can SKIP it instead of handing it
    /// to the walk again on every single search. Clearing it is what a later
    /// successful listing does, so granting Full Disk Access heals the mark
    /// without a rebuild.
    pub fn mark_dirs_unreadable(
        conn: &Connection,
        ids: &[i64],
        cause: Option<UnreadableCause>,
    ) -> Result<(), IndexStoreError> {
        if ids.is_empty() {
            return Ok(());
        }
        // Stay well under SQLite's default 999-parameter ceiling (+1 for the cause).
        const CHUNK: usize = 900;
        with_savepoint(conn, "mark_dirs_unreadable", |conn| {
            for chunk in ids.chunks(CHUNK) {
                let placeholders: String = (0..chunk.len())
                    .map(|i| format!("?{}", i + 2))
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!("UPDATE entries SET unreadable_cause = ?1 WHERE id IN ({placeholders})");
                let mut stmt = conn.prepare_cached(&sql)?;
                let stored = UnreadableCause::to_stored(cause);
                let mut values: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(chunk.len() + 1);
                values.push(&stored as &dyn rusqlite::types::ToSql);
                for id in chunk {
                    values.push(id as &dyn rusqlite::types::ToSql);
                }
                stmt.execute(&*values)?;
            }
            Ok(())
        })
    }

    /// Clear the mark from every directory carrying exactly `cause`, returning how
    /// many rows that was. The other causes are left alone.
    ///
    /// The heal for [`UnreadableCause::Abandoned`]: a wedged mount that came back
    /// has no successful listing to clear its own mark (nothing walks a directory
    /// the frontier stopped offering), so something has to reopen the ground.
    /// ❌ Never call it for `Denied` or `Declined`: those are answers, not
    /// timeouts, and re-offering them re-pays the same refusal on every search.
    ///
    /// ⚠️ `unreadable_cause` carries no index, so this is a full table scan — 6M
    /// rows on a boot disk. Call it only when something is known to be marked; the
    /// backoff in `writer/abandoned_retry.rs` is what makes sure of that.
    pub fn clear_unreadable_cause(conn: &Connection, cause: UnreadableCause) -> Result<usize, IndexStoreError> {
        let mut stmt = conn.prepare_cached("UPDATE entries SET unreadable_cause = 0 WHERE unreadable_cause = ?1")?;
        Ok(stmt.execute(params![UnreadableCause::to_stored(Some(cause))])?)
    }

    /// Why nothing is coming for this entry's contents, or `None` when something
    /// still might. The outer `None` means the entry doesn't exist.
    pub fn get_unreadable_cause_by_id(
        conn: &Connection,
        id: i64,
    ) -> Result<Option<Option<UnreadableCause>>, IndexStoreError> {
        let mut stmt = conn.prepare_cached("SELECT unreadable_cause FROM entries WHERE id = ?1")?;
        let result = stmt.query_row(params![id], |row| row.get::<_, i64>(0)).optional()?;
        Ok(result.map(UnreadableCause::from_stored))
    }

    /// The highest entry id this DB has ever handed out, or `0` for an empty one.
    ///
    /// A write watermark, not a count: ids come from one monotonic per-volume
    /// counter, so any walk that inserts rows raises this, and it costs an
    /// `O(log n)` seek to the end of the primary-key index rather than the
    /// `O(n)` a `COUNT(*)` would. Paired with `current_epoch` it identifies which
    /// state of the index an answer was computed against
    /// (`read/coverage.rs`'s `CoverageToken`).
    pub fn read_high_water_id(conn: &Connection) -> Result<i64, IndexStoreError> {
        let max: Option<i64> = conn
            .prepare_cached("SELECT MAX(id) FROM entries")?
            .query_row([], |row| row.get::<_, Option<i64>>(0))?;
        Ok(max.unwrap_or(0))
    }

    /// Read an entry's `listed_epoch` by id. `None` if the entry doesn't exist.
    pub fn get_listed_epoch_by_id(conn: &Connection, id: i64) -> Result<Option<u64>, IndexStoreError> {
        let mut stmt = conn.prepare_cached("SELECT listed_epoch FROM entries WHERE id = ?1")?;
        let result = stmt.query_row(params![id], |row| row.get::<_, u64>(0)).optional()?;
        Ok(result)
    }

    /// Get all directory paths from the entries table.
    ///
    /// Reconstructs full paths from the integer-keyed tree for backward compat.
    #[cfg(test)]
    pub fn get_all_directory_paths(conn: &Connection) -> Result<Vec<String>, IndexStoreError> {
        // Collect all directory (id, parent_id, name) tuples, then reconstruct paths in memory
        let mut stmt = conn.prepare("SELECT id, parent_id, name FROM entries WHERE is_directory = 1 AND id != ?1")?;
        let rows = stmt.query_map(params![ROOT_ID], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
        })?;

        let dir_entries: Vec<(i64, i64, String)> = rows.collect::<Result<Vec<_>, _>>()?;

        // Also include all entries (not just dirs) for path reconstruction of ancestors
        let mut all_stmt = conn.prepare("SELECT id, parent_id, name FROM entries")?;
        let all_rows = all_stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
        })?;
        let all_entries: Vec<(i64, i64, String)> = all_rows.collect::<Result<Vec<_>, _>>()?;
        let full_map: std::collections::HashMap<i64, (i64, &str)> = all_entries
            .iter()
            .map(|(id, pid, name)| (*id, (*pid, name.as_str())))
            .collect();

        let mut paths = Vec::with_capacity(dir_entries.len());
        for (id, _, _) in &dir_entries {
            let path = reconstruct_path_from_map(*id, &full_map);
            paths.push(path);
        }
        Ok(paths)
    }

    /// Drop all tables and recreate the schema (full reset).
    #[cfg(test)]
    pub fn clear_all(conn: &Connection) -> Result<(), IndexStoreError> {
        reset_schema(conn)?;
        Ok(())
    }
}
