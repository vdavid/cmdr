//! `IndexStore` entry-tree reads and writes: child listings, lookups by id /
//! inode / component, inserts, updates, renames/moves, and deletes. Pure code
//! movement from the former monolithic `store.rs`.

use super::*;

impl IndexStore {
    // ── Read methods (integer-keyed, new API) ────────────────────────

    /// List children of a directory by parent entry ID.
    #[cfg(test)]
    pub fn list_children(&self, parent_id: i64) -> Result<Vec<EntryRow>, IndexStoreError> {
        Self::list_children_on(parent_id, &self.read_conn)
    }

    /// List children of a directory by parent entry ID on a given connection.
    pub fn list_children_on(parent_id: i64, conn: &Connection) -> Result<Vec<EntryRow>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, parent_id, name, is_directory, is_symlink, logical_size, physical_size, modified_at, inode
             FROM entries WHERE parent_id = ?1",
        )?;
        let rows = stmt.query_map(params![parent_id], |row| {
            Ok(EntryRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                is_directory: row.get::<_, i32>(3)? != 0,
                is_symlink: row.get::<_, i32>(4)? != 0,
                logical_size: row.get(5)?,
                physical_size: row.get(6)?,
                modified_at: row.get(7)?,
                inode: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// List `(id, name)` pairs of child directories for a given parent entry ID.
    ///
    /// Used by `enrich_entries_with_index` to batch-fetch dir_stats for all
    /// subdirectories visible in a listing, then map back by name.
    pub fn list_child_dir_ids_and_names(
        conn: &Connection,
        parent_id: i64,
    ) -> Result<Vec<(i64, String)>, IndexStoreError> {
        let mut stmt = conn.prepare_cached("SELECT id, name FROM entries WHERE parent_id = ?1 AND is_directory = 1")?;
        let rows = stmt.query_map(params![parent_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Look up an entry by its integer ID.
    pub fn get_entry_by_id(conn: &Connection, id: i64) -> Result<Option<EntryRow>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, parent_id, name, is_directory, is_symlink, logical_size, physical_size, modified_at, inode
             FROM entries WHERE id = ?1",
        )?;
        let result = stmt
            .query_row(params![id], |row| {
                Ok(EntryRow {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    name: row.get(2)?,
                    is_directory: row.get::<_, i32>(3)? != 0,
                    is_symlink: row.get::<_, i32>(4)? != 0,
                    logical_size: row.get(5)?,
                    physical_size: row.get(6)?,
                    modified_at: row.get(7)?,
                    inode: row.get(8)?,
                })
            })
            .optional()?;
        Ok(result)
    }

    /// Get the parent ID of an entry.
    pub fn get_parent_id(conn: &Connection, entry_id: i64) -> Result<Option<i64>, IndexStoreError> {
        let mut stmt = conn.prepare_cached("SELECT parent_id FROM entries WHERE id = ?1")?;
        let result = stmt
            .query_row(params![entry_id], |row| row.get::<_, i64>(0))
            .optional()?;
        Ok(result)
    }

    /// Check if another entry with the same inode already has non-NULL sizes.
    pub fn has_sized_entry_for_inode(
        conn: &Connection,
        inode: u64,
        exclude_id: Option<i64>,
    ) -> Result<bool, IndexStoreError> {
        let found = match exclude_id {
            Some(eid) => {
                let mut stmt = conn.prepare_cached(
                    "SELECT 1 FROM entries WHERE inode = ?1 AND logical_size IS NOT NULL AND id != ?2 LIMIT 1",
                )?;
                stmt.query_row(params![inode, eid], |_| Ok(())).optional()?
            }
            None => {
                let mut stmt =
                    conn.prepare_cached("SELECT 1 FROM entries WHERE inode = ?1 AND logical_size IS NOT NULL LIMIT 1")?;
                stmt.query_row(params![inode], |_| Ok(())).optional()?
            }
        };
        Ok(found.is_some())
    }

    /// Look up an entry by inode. Returns the first matching entry's ID, or `None`.
    ///
    /// Uses the `idx_inode` index. Used by the live event loop's rename
    /// pre-pass: when an `item_renamed` event arrives, the new path is stat'd
    /// to get its inode, then matched against this query. On filesystems that
    /// preserve directory inodes across rename (APFS/HFS+/ext4/btrfs/XFS/NTFS),
    /// a hit means we can `MoveEntryV2` the existing row in place, preserving
    /// its `entry_id` and therefore its `dir_stats`.
    ///
    /// Multiple entries can share an inode (hardlinks for files); the `LIMIT 1`
    /// is fine because the rename pre-pass only needs to know whether _some_
    /// existing entry already represents this inode. For directory renames the
    /// inode is unique by construction.
    pub fn find_entry_by_inode(conn: &Connection, inode: u64) -> Result<Option<i64>, IndexStoreError> {
        let mut stmt = conn.prepare_cached("SELECT id FROM entries WHERE inode = ?1 LIMIT 1")?;
        let result = stmt.query_row(params![inode], |row| row.get::<_, i64>(0)).optional()?;
        Ok(result)
    }

    /// Resolve a path component under a given parent. Returns the child entry ID.
    pub fn resolve_component(conn: &Connection, parent_id: i64, name: &str) -> Result<Option<i64>, IndexStoreError> {
        let mut stmt =
            conn.prepare_cached("SELECT id FROM entries WHERE parent_id = ?1 AND name_folded = ?2 LIMIT 1")?;
        let folded = normalize_for_comparison(name);
        let result = stmt
            .query_row(params![parent_id, folded], |row| row.get::<_, i64>(0))
            .optional()?;
        Ok(result)
    }

    /// Reconstruct the full path for an entry by walking up the parent chain.
    ///
    /// Used by the importance scheduler to key each scored folder by its absolute
    /// path (the index's real identity is the path, not the rebuild-unstable id),
    /// and by tests.
    pub fn reconstruct_path(conn: &Connection, entry_id: i64) -> Result<String, IndexStoreError> {
        reconstruct_path(conn, entry_id)
    }

    // ── Static write helpers (for the writer thread) ─────────────────

    /// Insert a single entry by integer keys. Returns the new entry's ID.
    #[allow(
        clippy::too_many_arguments,
        reason = "refactoring to take &EntryRow would cascade into many callers"
    )]
    pub fn insert_entry_v2(
        conn: &Connection,
        parent_id: i64,
        name: &str,
        is_directory: bool,
        is_symlink: bool,
        logical_size: Option<u64>,
        physical_size: Option<u64>,
        modified_at: Option<u64>,
        inode: Option<u64>,
    ) -> Result<i64, IndexStoreError> {
        let name_folded = normalize_for_comparison(name);
        conn.execute(
            "INSERT INTO entries (parent_id, name, name_folded, is_directory, is_symlink, logical_size, physical_size, modified_at, inode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                parent_id,
                name,
                name_folded,
                is_directory as i32,
                is_symlink as i32,
                logical_size,
                physical_size,
                modified_at,
                inode,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Insert a single entry with an explicit ID. Used by the writer thread
    /// when processing `UpsertEntryV2` inserts, so the ID comes from the shared
    /// `next_id` counter rather than SQLite auto-assignment.
    #[allow(
        clippy::too_many_arguments,
        reason = "refactoring to take &EntryRow would cascade into many callers"
    )]
    pub fn insert_entry_v2_with_id(
        conn: &Connection,
        id: i64,
        parent_id: i64,
        name: &str,
        is_directory: bool,
        is_symlink: bool,
        logical_size: Option<u64>,
        physical_size: Option<u64>,
        modified_at: Option<u64>,
        inode: Option<u64>,
    ) -> Result<i64, IndexStoreError> {
        let name_folded = normalize_for_comparison(name);
        conn.execute(
            "INSERT INTO entries (id, parent_id, name, name_folded, is_directory, is_symlink, logical_size, physical_size, modified_at, inode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                parent_id,
                name,
                name_folded,
                is_directory as i32,
                is_symlink as i32,
                logical_size,
                physical_size,
                modified_at,
                inode,
            ],
        )?;
        Ok(id)
    }

    /// Batch insert entries with pre-assigned IDs inside a savepoint.
    ///
    /// Uses a savepoint instead of `unchecked_transaction()` so it nests correctly
    /// inside explicit transactions (replay's `BEGIN IMMEDIATE`).
    ///
    /// Uses `INSERT OR IGNORE` so a single `(parent_id, name_folded)` collision
    /// (case-sensitive filesystems with `Foo.txt`/`foo.txt` siblings, NFC/NFD
    /// duplicates from cross-OS sync, etc.) skips just that row rather than
    /// rolling back the whole batch of ~2000 entries. Returns a `Vec<bool>`
    /// parallel to `entries` where each element indicates whether the
    /// corresponding row actually landed in the DB. Callers (the writer
    /// thread's accumulator) must consult this so the in-memory aggregation
    /// state never claims more than the DB actually has.
    pub fn insert_entries_v2_batch(conn: &Connection, entries: &[EntryRow]) -> Result<Vec<bool>, IndexStoreError> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        with_savepoint(conn, "insert_entries", |conn| {
            // INSERT OR IGNORE: the table is truncated before full scans and
            // descendants are deleted before subtree scans, so collisions
            // against existing rows are rare, but two siblings with the same
            // `name_folded` can show up on case-sensitive volumes / sync
            // sources. Skip the duplicate, keep the rest.
            let mut stmt = conn.prepare_cached(
                "INSERT OR IGNORE INTO entries (id, parent_id, name, name_folded, is_directory, is_symlink, logical_size, physical_size, modified_at, inode)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            let mut inserted = Vec::with_capacity(entries.len());
            for e in entries {
                let name_folded = normalize_for_comparison(&e.name);
                let rows = stmt.execute(params![
                    e.id,
                    e.parent_id,
                    e.name,
                    name_folded,
                    e.is_directory as i32,
                    e.is_symlink as i32,
                    e.logical_size,
                    e.physical_size,
                    e.modified_at,
                    e.inode,
                ])?;
                inserted.push(rows == 1);
            }
            Ok(inserted)
        })
    }

    /// Update an existing entry by ID.
    #[allow(clippy::too_many_arguments, reason = "mirrors insert_entry_v2 signature")]
    pub fn update_entry(
        conn: &Connection,
        id: i64,
        is_directory: bool,
        is_symlink: bool,
        logical_size: Option<u64>,
        physical_size: Option<u64>,
        modified_at: Option<u64>,
        inode: Option<u64>,
    ) -> Result<(), IndexStoreError> {
        conn.execute(
            "UPDATE entries SET is_directory = ?1, is_symlink = ?2, logical_size = ?3, physical_size = ?4, \
             modified_at = ?5, inode = ?6 WHERE id = ?7",
            params![
                is_directory as i32,
                is_symlink as i32,
                logical_size,
                physical_size,
                modified_at,
                inode,
                id
            ],
        )?;
        Ok(())
    }

    /// Rename an entry (update its name).
    #[cfg(test)]
    pub fn rename_entry(conn: &Connection, id: i64, new_name: &str) -> Result<(), IndexStoreError> {
        conn.execute(
            "UPDATE entries SET name = ?1, name_folded = ?2 WHERE id = ?3",
            params![new_name, normalize_for_comparison(new_name), id],
        )?;
        Ok(())
    }

    /// Move an entry to a new parent.
    #[cfg(test)]
    pub fn move_entry(conn: &Connection, id: i64, new_parent_id: i64) -> Result<(), IndexStoreError> {
        conn.execute(
            "UPDATE entries SET parent_id = ?1 WHERE id = ?2",
            params![new_parent_id, id],
        )?;
        Ok(())
    }

    /// Delete a single entry and its dir_stats by ID.
    pub fn delete_entry_by_id(conn: &Connection, id: i64) -> Result<(), IndexStoreError> {
        conn.execute("DELETE FROM dir_stats WHERE entry_id = ?1", params![id])?;
        conn.execute("DELETE FROM entries WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Delete all descendants of an entry (but not the entry itself) using recursive CTE.
    ///
    /// Used before subtree rescans to prevent orphaned entries. The root entry is kept
    /// because the scanner's `ScanContext` resolves it by path and uses its existing ID.
    pub fn delete_descendants_by_id(conn: &Connection, root_id: i64) -> Result<(), IndexStoreError> {
        // Collect descendant IDs (excluding root) then delete dir_stats and entries
        conn.execute(
            "WITH RECURSIVE descendants(id) AS (
                SELECT id FROM entries WHERE parent_id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN descendants d ON e.parent_id = d.id
            )
            DELETE FROM dir_stats WHERE entry_id IN (SELECT id FROM descendants)",
            params![root_id],
        )?;
        conn.execute(
            "WITH RECURSIVE descendants(id) AS (
                SELECT id FROM entries WHERE parent_id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN descendants d ON e.parent_id = d.id
            )
            DELETE FROM entries WHERE id IN (SELECT id FROM descendants)",
            params![root_id],
        )?;
        Ok(())
    }

    /// Delete an entire subtree by root entry ID using recursive CTE.
    ///
    /// No internal transaction: safe to call inside an outer `BEGIN IMMEDIATE`.
    pub fn delete_subtree_by_id(conn: &Connection, root_id: i64) -> Result<(), IndexStoreError> {
        // Delete dir_stats first to avoid dangling references
        conn.execute(
            "WITH RECURSIVE subtree(id) AS (
                SELECT id FROM entries WHERE id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
            )
            DELETE FROM dir_stats WHERE entry_id IN (SELECT id FROM subtree)",
            params![root_id],
        )?;
        conn.execute(
            "WITH RECURSIVE subtree(id) AS (
                SELECT id FROM entries WHERE id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
            )
            DELETE FROM entries WHERE id IN (SELECT id FROM subtree)",
            params![root_id],
        )?;
        Ok(())
    }

    /// Get the next available entry ID. Useful for pre-allocating IDs during scan.
    pub fn get_next_id(conn: &Connection) -> Result<i64, IndexStoreError> {
        let max_id: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM entries", [], |row| row.get(0))?;
        Ok(max_id + 1)
    }

    /// Count the total number of entries in the index.
    pub fn get_entry_count(conn: &Connection) -> Result<u64, IndexStoreError> {
        let count: u64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Count directories in the index.
    pub fn get_dir_count(conn: &Connection) -> Result<u64, IndexStoreError> {
        let count: u64 = conn.query_row("SELECT COUNT(*) FROM entries WHERE is_directory = 1", [], |row| {
            row.get(0)
        })?;
        Ok(count)
    }
}
