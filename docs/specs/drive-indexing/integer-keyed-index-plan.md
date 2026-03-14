# Integer-keyed drive index

> **Note (schema v5):** The composite `UNIQUE INDEX idx_parent_name(parent_id, name)` described in this plan has been replaced with a simple `INDEX idx_parent(parent_id)`. The composite index with `platform_case` collation was extremely slow to build (~25 min for 5.1M entries). `resolve_path`/`resolve_component` now query all children by `parent_id` and match names in Rust via `platform_case_compare`. See the indexing `CLAUDE.md` for current schema details.

Replace the path-keyed SQLite schema with integer-keyed parent-child tree to reduce DB size ~6x (3.85 GB → ~600 MB).

## Motivation

The current index for a typical dev machine (5.57M entries) is **3.85 GB**:

| Component | Size | % |
|---|---|---|
| `entries` table (WITHOUT ROWID, path PK) | 1,938 MB | 52.8% |
| `idx_parent` secondary index | 1,659 MB | 45.2% |
| `dir_stats` table | 75 MB | 2.0% |

Two root causes:
1. **Redundant text columns**: `parent_path` (548 MB) and `name` (150 MB) are always derivable from `path`. That's ~700 MB of pure redundancy.
2. **WITHOUT ROWID + text PK amplifies index size**: In a WITHOUT ROWID table, secondary indexes store the full primary key as the row pointer. So `idx_parent` stores `(parent_path, full_path)` per row — averaging 235 bytes × 5.57M rows = 1.31 GB raw → 1,659 MB on disk. The index is almost as large as the table itself.

## New schema

```sql
-- Root entry: id=1, parent_id=0, name="" (sentinel for volume root)
-- Note: parent_id=0 for root is a sentinel; no row with id=0 exists.
-- This is safe because PRAGMA foreign_keys is off (SQLite default).
CREATE TABLE entries (
    id           INTEGER PRIMARY KEY,  -- auto-increment rowid (8 bytes)
    parent_id    INTEGER NOT NULL,     -- FK to entries.id (8 bytes)
    name         TEXT    NOT NULL COLLATE platform_case,  -- see "Case sensitivity" below
    is_directory INTEGER NOT NULL DEFAULT 0,
    is_symlink   INTEGER NOT NULL DEFAULT 0,
    size         INTEGER,
    modified_at  INTEGER
);
CREATE UNIQUE INDEX idx_parent_name ON entries (parent_id, name);

CREATE TABLE dir_stats (
    entry_id             INTEGER PRIMARY KEY,
    recursive_size       INTEGER NOT NULL DEFAULT 0,
    recursive_file_count INTEGER NOT NULL DEFAULT 0,
    recursive_dir_count  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;
```

**`platform_case` collation**: This is a placeholder in the schema above. At runtime, the code registers a custom SQLite collation function before creating tables:
- **macOS**: Register `platform_case` as case-insensitive **and** normalization-insensitive comparison, matching APFS behavior. APFS treats different Unicode normalization forms of the same name as identical (for example, "café" in NFC and NFD are the same file). The collation must **NFD-normalize then case-fold** both operands before comparing. Use the `unicode-normalization` crate for NFD and Rust's `to_lowercase` for case folding (or ICU case folding for full APFS fidelity). Plain `COLLATE NOCASE` is insufficient — it only handles ASCII case and no normalization.
- **Linux**: Register `platform_case` as binary comparison (equivalent to `COLLATE BINARY`, the SQLite default). ext4, btrfs, and most Linux filesystems are case-sensitive.

This way the schema DDL is identical on both platforms — only the collation function's implementation differs. The `idx_parent_name` unique index inherits the collation from the `name` column, so uniqueness enforcement automatically matches the filesystem's case rules.

**Implementation note**: Use `rusqlite::Connection::create_collation("platform_case", ...)` at connection init time, before any table creation or query. The collation must be registered on every connection (it's not persisted in the DB file).

**Tooling note**: Opening the DB file with the `sqlite3` CLI (or any tool that doesn't register the `platform_case` collation) will fail on queries that touch the `name` column or `idx_parent_name` index. Add a comment in the schema code and the module's CLAUDE.md to warn about this.

### Why this works

- **`idx_parent_name (parent_id, name)`** is the workhorse index. It serves two purposes:
  1. **List directory children**: `WHERE parent_id = ?` (fast integer prefix scan)
  2. **Resolve path component**: `WHERE parent_id = ? AND name = ?` (single index seek)
- **No path column at all**. Full paths are reconstructed by walking up the parent chain, or (more commonly) never needed because the frontend already knows the current directory path.
- **dir_stats keyed by integer** instead of path string. Lookup: resolve dir path → entry_id → stats.

### Estimated sizes

| Component | Current | New | Savings |
|---|---|---|---|
| `entries` table | 1,938 MB | ~450 MB | 1,488 MB |
| `idx_parent_name` | 1,659 MB | ~200 MB | 1,459 MB |
| `dir_stats` | 75 MB | ~20 MB | 55 MB |
| **Total** | **3,672 MB** | **~670 MB** | **~3,000 MB** |

Per-row math:
- entries: (8 + 8 + 28 + 1 + 1 + 8 + 8) = ~62 bytes + SQLite overhead → ~80 bytes × 5.57M = ~445 MB
- idx_parent_name: (8 + 28 + 8) = ~44 bytes × 5.57M = ~245 MB (with overhead: ~200 MB because integer keys pack efficiently)
- dir_stats: (8 + 8 + 8 + 8) = 32 bytes × 538K = ~17 MB

## Design decisions

### Path resolution strategy

Every external input arrives as a filesystem path (watcher events, user navigation, IPC commands). The system needs to resolve `path → entry_id` efficiently.

**Approach: component-by-component walk with a full-path LRU cache.**

To resolve `/Users/foo/bar/baz.txt`:
1. Start from root entry (id=1)
2. Look up `(parent_id=1, name="Users")` → id=42
3. Look up `(parent_id=42, name="foo")` → id=1337
4. Look up `(parent_id=1337, name="bar")` → id=98765
5. Look up `(parent_id=98765, name="baz.txt")` → id=654321

Each step is a single seek on `idx_parent_name`. With the 64 MB page cache, the entire index (~200 MB) fits ~30% in cache, and hot working-set directories are always cached. In practice, each lookup is a single cached page read.

**Why an LRU cache on top:**
- Watcher events cluster by directory (file A/B/C all resolve the same prefix)
- Directory listings resolve siblings (same parent_id for all)
- A 50K-entry LRU (~10 MB RAM) would hit 95%+ of lookups during normal operation

**Cache key: full path string → entry_id.** This is the fastest option because external inputs (watcher events, IPC commands) arrive as full paths, so the cache lookup is a single hash without decomposing the path. On cache miss, the resolver walks component-by-component and populates intermediate entries too (for example, resolving `/a/b/c` also caches `/a` → id and `/a/b` → id).

**Case-aware cache keys**: The cache must respect the filesystem's case and normalization rules. On macOS (case-insensitive APFS), `/Users/Foo` and `/Users/foo` are the same entry, so the cache must treat them as the same key. Approach: wrap the key in a newtype (`CacheKey`) that implements `Hash` and `Eq` using NFD normalization + case folding on macOS (same algorithm as the `platform_case` collation), and raw byte comparison on Linux. The original-case path string is stored as-is inside the wrapper — only hashing and equality use normalization + case-folding. This means cached paths display correctly if ever shown to the user, and invalidation by prefix also uses the same comparison. On Linux the wrapper compiles down to the same behavior as a plain `String` key (zero overhead).

**Cache invalidation**: only needed on delete and rename (remove stale entries from cache). Both are rare events. Invalidation is by path prefix — iterate the cache and drop all entries whose key starts with the affected path (using the same case-aware comparison as lookups). With a 50K-entry cache this is a fast linear scan (~microseconds).

### Subtree operations

Current range-query trick (`path > prefix/ AND path < prefix0`) won't work. Replace with recursive CTEs:

```sql
-- Delete subtree (both entries and their dir_stats)
WITH RECURSIVE subtree(id) AS (
    SELECT id FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
)
DELETE FROM dir_stats WHERE entry_id IN (SELECT id FROM subtree);

WITH RECURSIVE subtree(id) AS (
    SELECT id FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
)
DELETE FROM entries WHERE id IN (SELECT id FROM subtree);
```

Both deletes use the same recursive CTE pattern. The `dir_stats` delete runs first to avoid dangling references. In practice, both run inside the same transaction so order only matters for clarity.

Recursive CTEs in SQLite are well-optimized with the parent_id index. For a directory with 1K descendants, this is essentially a breadth-first traversal through cached index pages.

**Benchmark results** (5.57M total rows, 5 runs, median):

| Subtree size | Path-keyed range scan | Integer-keyed recursive CTE |
|---|---|---|
| ~100K entries | 214 ms | 180 ms |
| ~1K entries | 1.6 ms | 1.5 ms |

The recursive CTE is ~16% faster for large subtrees. The bottleneck is WAL write I/O, not traversal. No performance concern here.

**Alternative considered**: closure table or nested set model. Too much write overhead for a live-updated index. Recursive CTE on demand is simpler and fast enough.

### Aggregator changes

The aggregator currently:
1. Loads all directory paths, sorts by depth (counting `/` characters), processes bottom-up
2. Groups direct children by `parent_path`

With integer keys:
1. Load all `(id, parent_id)` pairs for directories
2. Topological sort via parent_id (equivalent to depth-first, reverse = bottom-up)
3. Group direct children by `parent_id`

This is actually cleaner and faster than string-based depth counting. Topological sort on integer pairs is trivially fast in memory.

### Scanner ID assignment

During full scan, the scanner needs to assign `parent_id` to each entry. jwalk visits directories in traversal order, so the parent directory is always already inserted.

**Approach: parent-ID stack + pre-allocated IDs.**

1. Insert root sentinel (id=1) via `INSERT OR IGNORE`
2. `SELECT MAX(id) + 1 FROM entries` → `next_id` (always ≥ 2 after root exists)
3. Maintain a `HashMap<PathBuf, i64>` mapping directory path → assigned id
4. For each scanned entry:
   - Look up parent_id from the map using the entry's parent path
   - Assign `id = next_id; next_id += 1`
   - If directory: add `(full_path, id)` to the map
5. Batch-insert entries with pre-assigned IDs (no need for RETURNING), in transactions of **10K–50K rows**. This keeps transaction size bounded (avoids journal bloat) while minimizing commit overhead.

The map size = number of unique directories encountered so far. For 538K dirs at ~200 bytes/entry, that's ~100 MB of temporary RAM during scan. Acceptable.

After scan completes, the map is dropped.

### Root entry

Insert a sentinel root entry at scan start:

```sql
INSERT OR IGNORE INTO entries (id, parent_id, name, is_directory)
VALUES (1, 0, '', 1);
```

All top-level entries (`/Users`, `/Applications`, etc.) have `parent_id = 1`. This eliminates the special-casing currently done for `/` in the aggregator and elsewhere.

### Reconciler path resolution

The reconciler receives `FsChangeEvent { path, event_id, flags }` from the watcher.

For each event:
1. Normalize path (firmlinks)
2. Check exclusions
3. `stat()` the file
4. **Resolve path → (parent_id, name, maybe existing_id)**:
   - Split path into components
   - Walk from root using LRU cache + `idx_parent_name` lookups
   - If all components resolve: this is an update to an existing entry
   - If the last component doesn't resolve: this is a new entry (parent must exist)
   - If an intermediate component doesn't resolve: path no longer exists (stale event, skip)
5. Issue appropriate write message with integer IDs

For creates: `InsertEntry { parent_id, name, ... }` → writer assigns next ID
For updates: `UpdateEntry { entry_id, ... }` → writer updates in place
For deletes: `DeleteEntry { entry_id }` or `DeleteSubtree { entry_id }`

### Delta propagation

Current: walk string path upward via `rfind('/')`.
New: walk `parent_id` chain upward via DB lookups.

```
fn propagate_delta(conn, entry_id, size_delta, ...) {
    let mut current_id = get_parent_id(conn, entry_id);
    while current_id != 0 {  // 0 = above root sentinel
        update_dir_stats(conn, current_id, delta);
        current_id = get_parent_id(conn, current_id);
    }
}
```

Each `get_parent_id` is a single integer PK lookup (nanoseconds on cached data). A typical path is 5-10 levels deep, so 5-10 lookups per propagation. This is comparable to the current string-based approach, which does 5-10 `rfind` operations + 5-10 `INSERT OR REPLACE` on dir_stats.

**TODO — batch delta propagation**: For a burst of watcher events in the same directory, the same parent chain gets walked repeatedly. Optimization: accumulate deltas per directory in a `HashMap<i64, Delta>`, then walk each unique ancestor chain once. Not needed for v1 (individual walks are fast enough on cached integer lookups) but worth revisiting if profiling shows hot spots during heavy file activity.

### IPC boundary stays path-based

**The frontend doesn't change.** The Tauri IPC boundary continues to use filesystem paths for all commands:
- `prioritize_dir(path)` → backend resolves path → id internally
- `get_dir_stats_batch(paths)` → backend resolves paths → ids, fetches stats
- `index-dir-updated { paths }` → backend already has the original path string from the watcher event

This is the right call because:
1. The frontend naturally works with filesystem paths (that's what it displays)
2. Adding an ID-based API would be a leaky abstraction (IDs are an internal optimization)
3. The resolution cost is negligible (cached lookups for ~50 dirs per page)

### Enrichment optimization

`enrich_entries_with_index` is called on every page fetch. Currently: batch-lookup dir_stats by path.

With integer keys, it's actually faster:
1. Resolve the current directory path → `dir_id` (one tree walk, almost always cached)
2. `SELECT id FROM entries WHERE parent_id = ?1 AND is_directory = 1` → child dir IDs
3. `SELECT * FROM dir_stats WHERE entry_id IN (...)` → stats for those IDs

Two queries, both on integer indexes. The current approach does N string lookups for N directories in the listing.

**Possible optimization**: The query in step 2 uses `idx_parent_name` for the `parent_id` prefix but needs a table lookup for each row to check `is_directory`. For directories with many children (for example, node_modules), this means many table lookups just to filter. If enrichment becomes a bottleneck, consider a covering index `(parent_id, is_directory)` to avoid the table lookups entirely. Not needed for v1 — the sequential rowid lookups are fast enough for typical page sizes.

### Renames and moves

Bonus improvement: renames and moves become simple updates instead of delete+insert.

Current (path is PK, can't update):
```
DELETE FROM entries WHERE path = old_path
DELETE FROM dir_stats WHERE path = old_path
-- (plus subtree if directory)
INSERT entries (new_path, ...)
-- recompute all subtree aggregates
```

New (just update parent_id or name):
```
UPDATE entries SET name = new_name WHERE id = entry_id  -- rename
UPDATE entries SET parent_id = new_parent_id WHERE id = entry_id  -- move
-- dir_stats stays valid! Entry ID doesn't change.
```

This is significantly more efficient for large directory moves and avoids recomputing all descendant aggregates.

### Search compatibility

Search will be implemented at the DB level and must be fast. With only `name` (basename) stored, the three search patterns are:

1. **Basename search** (`WHERE name LIKE '%query%'`): Works directly. Full table scan on 5.57M `name` values. Can be accelerated later with an FTS5 index on `name` for prefix/substring/fuzzy matching.
2. **Scoped search** ("find X under /foo/bar"): Resolve directory path → `entry_id`, recursive CTE (a `WITH RECURSIVE` SQL query that walks the parent-child tree) to collect subtree entry IDs, then filter by `name`. Two indexed operations composed — performs well because the recursive CTE traverses `idx_parent_name` and the name filter applies to the result set. For very large subtrees (100K+ descendants, for example all of node_modules), the CTE materializes all IDs before filtering. This is a known trade-off vs. the current path-range trick. If it becomes a bottleneck, an FTS5 index scoped by a `depth`/`ancestor` column could replace the CTE approach.
3. **Full-path substring search** ("find entries whose full path contains X"): The expensive case — requires reconstructing paths for matching. In practice this is rare in file managers. Can be served by first matching basenames, then reconstructing and filtering full paths for the (small) result set.

This schema does not block search. Basename + scoped search (the common cases) work well natively. Full-path search is achievable with post-reconstruction filtering. If profiling shows it's too slow, a future optimization could add a dedicated search table or FTS5 index with reconstructed paths, populated asynchronously after scan.

## Migration

**Schema version bump**: `SCHEMA_VERSION` goes from "1" to "2". Existing installs get their DB automatically deleted and rebuilt on next launch (by-design: disposable cache pattern). No migration code needed.

## Implementation milestones

### Milestone 1: schema + store

- [x] New schema in `store.rs`: entries with integer PK, dir_stats with entry_id, root sentinel
- [x] `platform_case` collation: register at connection init, case-insensitive on macOS, binary on Linux
- [x] `PathResolver` struct: component-by-component walk with full-path LRU cache (case-aware `CacheKey` wrapper), invalidation on delete/rename
- [x] Updated read queries: `list_children(parent_id)`, `get_entry_by_id(id)`, `resolve_path(path) → Option<i64>`
- [x] Updated write queries: `insert_entry(parent_id, name, ...)`, `update_entry(id, ...)`, `delete_entry(id)`, `delete_subtree(id)` using recursive CTE
- [x] `get_dir_stats(entry_id)`, `get_dir_stats_batch(entry_ids)`
- [x] Tests for all new queries, path resolution, cache invalidation

### Milestone 2: scanner + writer

- [x] `ScannedEntry` struct: replaced with `EntryRow` (id, parent_id, name, is_directory, is_symlink, size, modified_at)
  - `ScanContext` struct manages parent-ID map during scan
- [x] Scanner: parent-ID stack, pre-allocated IDs, root sentinel insertion
- [x] Writer: updated `WriteMessage` variants to use integer IDs
  - `InsertEntriesV2(Vec<EntryRow>)` — entries have pre-assigned IDs
  - `UpsertEntryV2 { parent_id, name, ... }` — for live events
  - `DeleteEntryById(i64)` / `DeleteSubtreeById(i64)` — by entry ID
  - `PropagateDeltaById { entry_id, ... }` — walk parent_id chain
- [x] Tests for scan with integer IDs, writer message processing

### Milestone 3: aggregator

- [x] `compute_all_aggregates`: topological sort on `(id, parent_id)`, bottom-up computation
- [x] `compute_subtree_aggregates`: recursive CTE to collect subtree dir IDs, then bottom-up
- [x] `propagate_delta`: walk `parent_id` chain with integer lookups
- [x] Root entry aggregation: root sentinel (id=1) included naturally, no special case
- [x] Tests for all three aggregation modes

### Milestone 4: reconciler + watcher integration

- [x] `process_fs_event`: resolve event path → entry_id via `store::resolve_path()`
  - Handle "entry not found" (new file) vs "entry found" (update) vs "stale event" (intermediate missing)
- [x] Correct handling of creates, updates, deletes, subtree deletes
- [x] `MustScanSubDirs` handling with integer IDs
- [x] Cache invalidation on deletes and renames
- [x] Delta propagation using entry IDs
- [x] Replay event loop: uses read connection for all event resolution
- [x] Tests for event processing with integer IDs

### Milestone 5: mod.rs + IPC + enrichment

- [x] `IndexManager`: owns a `PathResolver`, uses it for IPC commands
- [x] `enrich_entries_with_index`: resolve parent dir → id, batch-fetch child dir_stats via `list_child_dir_ids_and_names`
- [x] IPC commands: unchanged interface (still accept paths), resolve internally
- [x] `index-dir-updated` events: continue sending paths (reconciler already has them)
- [x] `get_dir_stats(path)` / `get_dir_stats_batch(paths)`: resolve path → id → stats via PathResolver
- [x] End-to-end integration test: scan, enrich, watcher events, verify stats

### Milestone 6: verification + cleanup

- [x] verifier.rs remains placeholder (per-navigation readdir diff is a separate future feature)
- [x] Run full check suite: `./scripts/check.sh --rust` — all green
- [ ] Manual test with MCP: scan, navigate, verify sizes, delete files, watch updates
- [x] Update CLAUDE.md in the indexing directory
- [x] Clean up dead code from the path-keyed schema

## Risk analysis

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Subtle ID assignment bugs during concurrent micro-scans + full scan | Medium | High | Single-writer thread serializes all ID allocation. Pre-allocated IDs in scanner, writer assigns for live events. |
| Path resolver cache going stale after rapid renames | Low | Medium | Conservative invalidation: any delete/rename drops the entire subtree from cache. Cache is soft (miss = DB lookup). |
| Recursive CTE performance for very deep trees | Low | Low | Typical depth is 5-15 levels. Even 50-level deep trees resolve in microseconds on integer indexes. |
| Scan memory for parent-ID map (538K dirs × ~200 bytes) | Low | Low | ~100 MB temporary RAM during scan, freed after. Acceptable for a background task. |
| Frontend assumes path-based events | None | None | IPC boundary unchanged. Backend resolves path↔id internally. |
| Case/normalization mismatch between DB and filesystem | Medium | High | `platform_case` collation registered at connection init. macOS: NFD normalize + case fold (matching APFS behavior). Linux: binary. Same algorithm used in `CacheKey` wrapper. Must be registered on every connection before any query. |
