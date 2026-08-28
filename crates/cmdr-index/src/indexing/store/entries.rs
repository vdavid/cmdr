//! `IndexStore` entry-tree reads and writes: child listings, lookups by id /
//! inode / component, inserts, updates, renames/moves, and deletes. Pure code
//! movement from the former monolithic `store.rs`.

use super::{EntryRow, IndexStore, IndexStoreError, normalize_for_comparison, reconstruct_path, with_savepoint};
use rusqlite::{Connection, OptionalExtension, params};

#[cfg(test)]
use super::ROOT_ID;

/// Parent ids per child-lookup query, and ids per `DELETE`. Both stay well under
/// SQLite's default 999-parameter ceiling.
const DELETE_CHUNK: usize = 256;

/// `"?, ?, …"` for an `IN` list of `count` bound parameters.
fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

/// An inode as SQLite stores it: the same 64 bits, reinterpreted as signed.
///
/// SQLite's `INTEGER` is a signed 64-bit column and holds every bit of a `u64`,
/// but binding a bare `u64` asks rusqlite to prove it's positive and raises a
/// `TryFromIntError` when it isn't. That error aborts the whole savepoint, so
/// ONE such row used to lose an entire ~2000-row batch (`ERR-AYVM4`: a 609-entry
/// SMB directory indexed as empty, and stayed empty, because the scan marks the
/// directory listed either way).
///
/// High-bit inodes are ordinary, not corrupt: `ATTR_CMN_FILEID` on a mounted
/// smbfs share is the server's 64-bit file id, or a path hash when the server
/// has no usable one. 43% of one directory's files on a QNAP measured above
/// `i64::MAX` (2026-08-27).
///
/// ❌ Never clamp an inode instead. It is an IDENTITY: saturating would collapse
/// every high-bit value onto `i64::MAX`, and `find_entry_by_inode` would match
/// unrelated files into each other during rename detection and hardlink dedup.
/// The bit-cast round-trips exactly and preserves equality, so `idx_inode` keeps
/// working and rows written before this existed (all of them ≤ `i64::MAX`, where
/// the cast is the identity) need no migration.
///
/// ❗ Both directions and every inode read, write, and lookup must use this
/// pair, or a seek binds a positive value against a row stored as negative.
const fn inode_to_sql(inode: u64) -> i64 {
    inode as i64
}

const fn inode_from_sql(stored: i64) -> u64 {
    stored as u64
}

/// A size or timestamp as SQLite stores it, saturating at `i64::MAX`.
///
/// The same bind failure as [`inode_to_sql`], with the opposite right answer: a
/// size is a MAGNITUDE, so an absurd one is garbage worth clamping rather than
/// an identity worth preserving. The reachable case is `physical_size`, which
/// comes from `st_blocks * 512` and wraps in release on a bogus block count.
/// Losing the row's whole batch over it is the one outcome nobody wants.
fn size_to_sql(value: Option<u64>) -> Option<i64> {
    value.map(|v| i64::try_from(v).unwrap_or(i64::MAX))
}

impl IndexStore {
    // ── Read methods (integer-keyed, new API) ────────────────────────

    /// List children of a directory by parent entry ID.
    #[cfg(any(test, feature = "testing"))]
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
                inode: row.get::<_, Option<i64>>(8)?.map(inode_from_sql),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Count a directory's children, stopping at `cap` rows.
    ///
    /// Returns `min(actual_children, cap)`, so a caller passing `threshold + 1`
    /// learns "more than the threshold?" while touching at most `cap` index rows.
    /// ❌ Don't reach for `COUNT(*)` here: the whole point is that the answer must
    /// not cost O(children) on the directory that motivated the question (1.14M
    /// rows). The inner `SELECT 1 … LIMIT` reads the `parent_id` index only.
    pub fn count_children_capped(parent_id: i64, conn: &Connection, cap: i64) -> Result<usize, IndexStoreError> {
        let mut stmt =
            conn.prepare_cached("SELECT COUNT(*) FROM (SELECT 1 FROM entries WHERE parent_id = ?1 LIMIT ?2)")?;
        let count: i64 = stmt.query_row(params![parent_id, cap], |row| row.get(0))?;
        Ok(count.max(0) as usize)
    }

    /// List up to `limit` children of a directory. The operation log's search-leaf
    /// enumeration (`journal_search`) walks a subtree BEFORE a trash / same-FS move
    /// with a bounded budget, so it reads at most `cap + 1` rows total regardless of
    /// how huge the folder is — a 1M-child folder must not pay a 1M-row read before
    /// a sub-second rename. Row order is unspecified (the caller only counts +
    /// buckets); `limit` is applied as a SQL `LIMIT`.
    pub fn list_children_on_limited(
        parent_id: i64,
        conn: &Connection,
        limit: i64,
    ) -> Result<Vec<EntryRow>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, parent_id, name, is_directory, is_symlink, logical_size, physical_size, modified_at, inode
             FROM entries WHERE parent_id = ?1 LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![parent_id, limit], |row| {
            Ok(EntryRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                is_directory: row.get::<_, i32>(3)? != 0,
                is_symlink: row.get::<_, i32>(4)? != 0,
                logical_size: row.get(5)?,
                physical_size: row.get(6)?,
                modified_at: row.get(7)?,
                inode: row.get::<_, Option<i64>>(8)?.map(inode_from_sql),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Read every entry in the index in one query.
    ///
    /// Lets a full-index consumer (the importance recompute) pull the whole tree
    /// once and reconstruct paths / bucket children in memory, instead of issuing
    /// per-directory point queries. Ordered by id only for determinism; callers
    /// index it into their own maps.
    pub fn all_entries(conn: &Connection) -> Result<Vec<EntryRow>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, parent_id, name, is_directory, is_symlink, logical_size, physical_size, modified_at, inode
             FROM entries ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(EntryRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                is_directory: row.get::<_, i32>(3)? != 0,
                is_symlink: row.get::<_, i32>(4)? != 0,
                logical_size: row.get(5)?,
                physical_size: row.get(6)?,
                modified_at: row.get(7)?,
                inode: row.get::<_, Option<i64>>(8)?.map(inode_from_sql),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Read every DIRECTORY entry in the index in one query (files excluded).
    ///
    /// Cheaper than [`all_entries`](IndexStore::all_entries) — on a multi-million-entry
    /// NAS the directories are a small fraction of the rows — but still ~112 bytes plus
    /// a heap `String` per row. ❌ Not for a whole-index walk: those want
    /// [`for_each_directory`](IndexStore::for_each_directory) and a compact structure
    /// (`DirTree`), which holds the same folders at a third of the cost. Ordered by id
    /// for determinism.
    pub fn all_directories(conn: &Connection) -> Result<Vec<EntryRow>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, parent_id, name, is_directory, is_symlink, logical_size, physical_size, modified_at, inode
             FROM entries WHERE is_directory = 1 ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(EntryRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                is_directory: row.get::<_, i32>(3)? != 0,
                is_symlink: row.get::<_, i32>(4)? != 0,
                logical_size: row.get(5)?,
                physical_size: row.get(6)?,
                modified_at: row.get(7)?,
                inode: row.get::<_, Option<i64>>(8)?.map(inode_from_sql),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Stream every DIRECTORY entry's `(id, parent_id, name, modified_at)` through `f`, one row
    /// at a time, ordered by `id`.
    ///
    /// The whole-index-walk subset: a consumer rebuilding absolute paths and scoring folders
    /// needs exactly these four columns, never a full [`EntryRow`] (~112 bytes plus a heap
    /// `String` per row). Streaming them, and handing the name out as a borrowed `&str` off
    /// SQLite's own row buffer, lets the caller fold each directory into a compact structure
    /// without the query itself allocating anything per row. [`DirTree`](super::DirTree) does
    /// exactly that, holding a 391,563-directory NAS index in 24.6 MB against 76.0 MB for the
    /// full-row shape (measured 2026-07-25; see `media_index/DETAILS.md`).
    ///
    /// Prefer this over [`all_directories`](IndexStore::all_directories) whenever the consumer
    /// wants paths rather than metadata. The `ORDER BY id` is what makes the result binary-
    /// searchable, so don't drop it.
    pub fn for_each_directory(
        conn: &Connection,
        mut f: impl FnMut(i64, i64, &str, Option<u64>),
    ) -> Result<(), IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, parent_id, name, modified_at FROM entries WHERE is_directory = 1 ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let parent_id: i64 = row.get(1)?;
            // `get_ref` borrows SQLite's own buffer, so no `String` is allocated per row.
            let name = row.get_ref(2)?.as_str().map_err(rusqlite::Error::from)?;
            let modified_at: Option<u64> = row.get(3)?;
            f(id, parent_id, name, modified_at);
        }
        Ok(())
    }

    /// Stream every FILE entry's `(parent_id, name)` through `f`, one row at a time, with all of
    /// one parent's files CONTIGUOUS.
    ///
    /// The streaming half of the memory-lean importance walk: file rows are the bulk of a NAS
    /// index, and the recompute only needs each file's parent and name (to fold into its
    /// parent's extension set / count / marker flag), never the whole row. Passing them through
    /// a callback means the file rows are never all resident — so a full pass holds O(dirs)
    /// memory, not O(entries).
    ///
    /// **The grouping is the point, and it costs something.** Because each parent's files arrive
    /// together, the caller folds a group through ONE reusable accumulator and closes it at the
    /// boundary, instead of holding an open accumulator per directory for the whole scan (which
    /// is what makes a distinct-extension set cost per-folder memory). `ORDER BY parent_id` buys
    /// that by walking `idx_parent_name_folded` and fetching each row by rowid rather than
    /// scanning the table in storage order: roughly 3× the query time (1.5 s → 4.7 s over 7.4M
    /// file rows on a real root index, measured 2026-07-27). ❌ Don't drop the `ORDER BY` to win
    /// that back without giving the caller another way to close a group.
    pub fn for_each_file_child_by_parent(
        conn: &Connection,
        mut f: impl FnMut(i64, &str),
    ) -> Result<(), IndexStoreError> {
        let mut stmt =
            conn.prepare_cached("SELECT parent_id, name FROM entries WHERE is_directory = 0 ORDER BY parent_id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let parent_id: i64 = row.get(0)?;
            // `get_ref` borrows SQLite's own buffer, so no `String` is allocated per row.
            let name = row.get_ref(1)?.as_str().map_err(rusqlite::Error::from)?;
            f(parent_id, name);
        }
        Ok(())
    }

    /// Stream the child DIRECTORY rows of every parent in `parent_ids` through `f` as
    /// `(id, parent_id, name, modified_at)`.
    ///
    /// The level-by-level descent a SCOPED walk uses: the importance incremental
    /// rescore reads only the changed subtrees, so it expands one whole level per
    /// query instead of paying a point query per directory (or, as the full walk
    /// does, reading the volume). Served by `idx_parent_name_folded`'s leading
    /// `parent_id` column, so the cost tracks the subtree, not the table.
    ///
    /// The caller chunks `parent_ids` to stay under SQLite's bound-parameter limit.
    /// Rows arrive in no guaranteed order — a directory has no per-parent
    /// accumulator to close, unlike the file side.
    pub fn for_each_child_directory_of(
        conn: &Connection,
        parent_ids: &[i64],
        mut f: impl FnMut(i64, i64, &str, Option<u64>),
    ) -> Result<(), IndexStoreError> {
        if parent_ids.is_empty() {
            return Ok(());
        }
        let sql = format!(
            "SELECT id, parent_id, name, modified_at FROM entries \
             WHERE is_directory = 1 AND parent_id IN ({})",
            placeholders(parent_ids.len())
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(parent_ids))?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let parent_id: i64 = row.get(1)?;
            // `get_ref` borrows SQLite's own buffer, so no `String` is allocated per row.
            let name = row.get_ref(2)?.as_str().map_err(rusqlite::Error::from)?;
            let modified_at: Option<u64> = row.get(3)?;
            f(id, parent_id, name, modified_at);
        }
        Ok(())
    }

    /// Stream the child FILE rows of every parent in `parent_ids` through `f` as
    /// `(parent_id, name)`, with all of one parent's files CONTIGUOUS.
    ///
    /// The scoped counterpart to
    /// [`for_each_file_child_by_parent`](IndexStore::for_each_file_child_by_parent),
    /// and it keeps that one's contract: the `ORDER BY parent_id` is what lets the
    /// caller fold a directory's distinct extensions through ONE reusable accumulator
    /// and close it at the group boundary. ❌ Don't drop it. Each parent id appears in
    /// exactly one chunk, so chunking never splits a group.
    pub fn for_each_child_file_of(
        conn: &Connection,
        parent_ids: &[i64],
        mut f: impl FnMut(i64, &str),
    ) -> Result<(), IndexStoreError> {
        if parent_ids.is_empty() {
            return Ok(());
        }
        let sql = format!(
            "SELECT parent_id, name FROM entries WHERE is_directory = 0 AND parent_id IN ({}) ORDER BY parent_id",
            placeholders(parent_ids.len())
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(parent_ids))?;
        while let Some(row) = rows.next()? {
            let parent_id: i64 = row.get(0)?;
            let name = row.get_ref(1)?.as_str().map_err(rusqlite::Error::from)?;
            f(parent_id, name);
        }
        Ok(())
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
                    inode: row.get::<_, Option<i64>>(8)?.map(inode_from_sql),
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
                stmt.query_row(params![inode_to_sql(inode), eid], |_| Ok(()))
                    .optional()?
            }
            None => {
                let mut stmt =
                    conn.prepare_cached("SELECT 1 FROM entries WHERE inode = ?1 AND logical_size IS NOT NULL LIMIT 1")?;
                stmt.query_row(params![inode_to_sql(inode)], |_| Ok(())).optional()?
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
        let result = stmt
            .query_row(params![inode_to_sql(inode)], |row| row.get::<_, i64>(0))
            .optional()?;
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
        // `prepare_cached` for the same reason as `insert_entry_v2_with_id` below:
        // `Connection::execute` re-prepares from SQL text per call.
        let mut stmt = conn.prepare_cached(
            "INSERT INTO entries (parent_id, name, name_folded, is_directory, is_symlink, logical_size, physical_size, modified_at, inode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        stmt.execute(params![
            parent_id,
            name,
            name_folded,
            is_directory as i32,
            is_symlink as i32,
            size_to_sql(logical_size),
            size_to_sql(physical_size),
            size_to_sql(modified_at),
            inode.map(inode_to_sql),
        ])?;
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
        // `prepare_cached`, never `execute` with a literal: this is the LIVE reconcile
        // write path, called once per file the watcher sees, and `Connection::execute`
        // re-prepares from SQL TEXT on every call. Measured on a prod profile
        // (2026-08-03): 1,828 of ~3,398 running samples on the writer thread sat in
        // `sqlite3RunParser` → `sqlite3Insert` → `sqlite3GenerateConstraintChecks`,
        // against 182 in `sqlite3_step`. Re-parsing cost 10x what executing did.
        // The cache lives on the connection, which the writer opens once
        // (`writer/mod.rs`), so the compile happens once per process.
        let mut stmt = conn.prepare_cached(
            "INSERT INTO entries (id, parent_id, name, name_folded, is_directory, is_symlink, logical_size, physical_size, modified_at, inode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        stmt.execute(params![
            id,
            parent_id,
            name,
            name_folded,
            is_directory as i32,
            is_symlink as i32,
            size_to_sql(logical_size),
            size_to_sql(physical_size),
            size_to_sql(modified_at),
            inode.map(inode_to_sql),
        ])?;
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
                    size_to_sql(e.logical_size),
                    size_to_sql(e.physical_size),
                    size_to_sql(e.modified_at),
                    e.inode.map(inode_to_sql),
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
                size_to_sql(logical_size),
                size_to_sql(physical_size),
                size_to_sql(modified_at),
                inode.map(inode_to_sql),
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

    /// Delete all descendants of an entry (but not the entry itself), returning
    /// how many rows went.
    ///
    /// Used before a subtree rescan to prevent orphaned entries. The root entry is
    /// kept because the scanner's `ScanContext` resolves it by path and reuses its
    /// existing ID.
    ///
    /// **Post-order: a directory row goes only once its whole subtree is gone.**
    /// Deleting top-down instead severs the tree at whatever point the process
    /// dies, and every row below the cut loses its path to the root, so no later
    /// descent can reach it — one interrupted bulk delete on the author's QNAP
    /// stranded 9 793 362 rows permanently. Files are leaves, so they go on the way
    /// DOWN; directory ids are recorded per level and deleted deepest level first.
    /// Pinned by `tests/subtree_deletes.rs::interrupting_a_subtree_delete_never_strands_a_row`.
    ///
    /// **Walks the tree in bounded chunks rather than one recursive-CTE `DELETE`.**
    /// A single CTE delete materializes every descendant id into one ephemeral
    /// table and one transaction: on a real QNAP index that meant 10 898 710 ids
    /// (~87 MB of ephemeral rowids, twice) and a multi-GB WAL spike in one shot.
    /// Descending level by level caps both — the retained dir ids peaked at
    /// 324 128 (2.6 MB) across all levels of that index, files never accumulate,
    /// and the whole delete took 23 s (2026-07-25, measured over a copy of the
    /// production DB). Reading children by `parent_id` is what makes the order
    /// free to choose: a deleted parent row never hides its children.
    pub fn delete_descendants_by_id(conn: &Connection, root_id: i64) -> Result<u64, IndexStoreError> {
        Self::delete_descendants_inner(conn, root_id, &mut None)
    }

    /// Test-only: run `delete_descendants_by_id` but stop after exactly
    /// `max_rows` deletions, simulating the process dying mid-prune.
    ///
    /// Stopping mid-`DELETE`-batch is a state a real crash can't produce (SQLite
    /// commits a statement atomically), so sweeping `max_rows` over `1..=total`
    /// checks a SUPERSET of the reachable interruption points — every prefix of
    /// the deletion order, not only the per-batch boundaries.
    #[cfg(test)]
    pub fn delete_descendants_by_id_stopping_after(
        conn: &Connection,
        root_id: i64,
        max_rows: u64,
    ) -> Result<u64, IndexStoreError> {
        Self::delete_descendants_inner(conn, root_id, &mut Some(max_rows))
    }

    fn delete_descendants_inner(
        conn: &Connection,
        root_id: i64,
        budget: &mut Option<u64>,
    ) -> Result<u64, IndexStoreError> {
        let mut deleted: u64 = 0;
        // Descend breadth-first, deleting the FILES of each level as we meet them
        // (a leaf can't strand anything) and banking the level's directory ids.
        let mut levels: Vec<Vec<i64>> = Vec::new();
        let mut frontier = vec![root_id];
        let mut at_root = true;
        while !frontier.is_empty() {
            let mut next_level = Vec::new();
            for parents in frontier.chunks(DELETE_CHUNK) {
                let children = Self::child_ids_of(conn, parents)?;
                let mut files = Vec::new();
                for (id, is_dir) in children {
                    if is_dir {
                        next_level.push(id);
                    } else {
                        files.push(id);
                    }
                }
                if !Self::delete_batched(conn, &files, budget, &mut deleted)? {
                    return Ok(deleted);
                }
            }
            let done = std::mem::replace(&mut frontier, next_level);
            // `root_id` itself always survives; every deeper level is ours to drop.
            if at_root {
                at_root = false;
            } else {
                levels.push(done);
            }
        }
        // Now the directories, deepest level first: each one's descendants are
        // already gone, so an interruption here leaves a walkable tree.
        for level in levels.iter().rev() {
            if !Self::delete_batched(conn, level, budget, &mut deleted)? {
                return Ok(deleted);
            }
        }
        Ok(deleted)
    }

    /// Delete `ids` in `DELETE_CHUNK`-sized batches, adding to `deleted`.
    /// Returns `false` when a test budget ran out, so the caller stops.
    fn delete_batched(
        conn: &Connection,
        ids: &[i64],
        budget: &mut Option<u64>,
        deleted: &mut u64,
    ) -> Result<bool, IndexStoreError> {
        for chunk in ids.chunks(DELETE_CHUNK) {
            let chunk = match budget {
                Some(0) => return Ok(false),
                Some(left) => {
                    let take = chunk.len().min(usize::try_from(*left).unwrap_or(usize::MAX));
                    *left -= take as u64;
                    &chunk[..take]
                }
                None => chunk,
            };
            Self::delete_rows_by_id(conn, chunk)?;
            *deleted += chunk.len() as u64;
        }
        Ok(true)
    }

    /// The `(id, is_directory)` of every direct child of the given parent ids.
    /// Reads off the `idx_parent_name_folded` composite index.
    fn child_ids_of(conn: &Connection, parent_ids: &[i64]) -> Result<Vec<(i64, bool)>, IndexStoreError> {
        if parent_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; parent_ids.len()].join(", ");
        let sql = format!("SELECT id, is_directory FROM entries WHERE parent_id IN ({placeholders})");
        let mut stmt = conn.prepare_cached(&sql)?;
        let values: Vec<&dyn rusqlite::types::ToSql> =
            parent_ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
        let rows = stmt.query_map(&*values, |row| Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Delete the given entries and their `dir_stats` rows. `dir_stats` goes
    /// first so no row ever references a missing entry.
    fn delete_rows_by_id(conn: &Connection, ids: &[i64]) -> Result<(), IndexStoreError> {
        if ids.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let values: Vec<&dyn rusqlite::types::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
        let stats_sql = format!("DELETE FROM dir_stats WHERE entry_id IN ({placeholders})");
        conn.prepare_cached(&stats_sql)?.execute(&*values)?;
        let entries_sql = format!("DELETE FROM entries WHERE id IN ({placeholders})");
        conn.prepare_cached(&entries_sql)?.execute(&*values)?;
        Ok(())
    }

    /// Test-only: every entry whose `parent_id` points at a row that no longer
    /// exists, as `(all ids, the directory ids among them)`.
    ///
    /// Such rows are unreachable from the index root, so nothing in the app can
    /// list, enrich, or path-resolve them; they only bloat the file and every
    /// O(entries) walk. Post-order deletion is what keeps them from ever being
    /// created, and this is how the tests prove it. The index root is excluded: its
    /// `parent_id` is the `ROOT_PARENT_ID` sentinel, which has no row by design.
    #[cfg(test)]
    pub fn find_orphan_entries(conn: &Connection) -> Result<(Vec<i64>, Vec<i64>), IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT e.id, e.is_directory FROM entries e
             LEFT JOIN entries p ON e.parent_id = p.id
             WHERE p.id IS NULL AND e.id != ?1",
        )?;
        let rows = stmt.query_map(params![ROOT_ID], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?))
        })?;
        let mut all = Vec::new();
        let mut dirs = Vec::new();
        for row in rows {
            let (id, is_dir) = row?;
            if is_dir {
                dirs.push(id);
            }
            all.push(id);
        }
        Ok((all, dirs))
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
