# Fix hardlink size overcounting in indexing

## Problem

The app shows ~2.5x inflated sizes for directories with many hardlinks (for example, `target/debug/`: 233 GB displayed
vs ~93 GB actual per `du -sk`).

**Root cause**: The scanner correctly deduplicates hardlinks during the full scan (stores `NULL` sizes for 2nd+ links
via an in-memory `HashSet<u64>` of inodes). But the reconciler (live events), verifier (navigation drift correction),
and post-replay verification all use `UpsertEntryV2` with full sizes from `stat()` — no dedup. This overwrites the
scanner's `NULL` values, "undoing" the dedup. Over time, more and more secondary links get their sizes restored,
inflating the totals.

**Why the current dedup can't survive live events**: The schema has no `inode` column, so there's no way to check at
upsert time whether another entry for the same inode already has sizes. The scanner's `HashSet` is in-memory and
ephemeral.

## Solution: store inode in schema, centralize dedup in the writer

Make hardlink dedup **explicit and structural** by storing the inode in the DB and checking it at write time.

**Core invariant**: For any inode with `nlink > 1`, at most one entry in the DB has non-NULL sizes.

### Schema v8

Add `inode INTEGER` column to `entries`. Add an index for dedup lookups:

```sql
-- In CREATE TABLE entries, after modified_at:
inode INTEGER

-- After table creation:
CREATE INDEX IF NOT EXISTS idx_inode ON entries (inode);
```

Schema version mismatch triggers drop+rebuild automatically (it's a disposable cache), so no migration needed.

### File: `store.rs`

1. Bump `SCHEMA_VERSION` to `"8"`.
2. Add `inode INTEGER` column to `CREATE TABLE entries` (after `modified_at`), in both macOS and non-macOS variants.
3. Add `CREATE INDEX IF NOT EXISTS idx_inode ON entries (inode)` after table creation.
4. Update `insert_entry_v2` (~line 750) and `insert_entries_v2_batch` (~line 801) to include `inode` in the INSERT —
   both macOS and non-macOS SQL variants.
5. Update `update_entry` (~line 863) to include `inode` in the UPDATE.
6. Add a query function for the dedup check:
   ```rust
   /// Check if another entry with the same inode already has non-NULL sizes.
   pub fn has_sized_entry_for_inode(
       conn: &Connection, inode: u64, exclude_id: Option<i64>
   ) -> Result<bool, IndexStoreError>
   ```
   SQL: `SELECT 1 FROM entries WHERE inode = ?1 AND logical_size IS NOT NULL [AND id != ?2] LIMIT 1`

### File: `store.rs` — `EntryRow` and SELECT queries

Add `inode: Option<u64>` field to `EntryRow`. The scanner populates this from `MetadataExt::ino()` for every file on
Unix.

**All SELECT queries that construct `EntryRow` must include `inode`**:
- `list_children_on` (~line 578) — used by verifier for DB snapshot
- `get_entry_by_id` (~line 614) — used by writer's UPDATE path to read old entry
- Any test helper constructors that build `EntryRow` directly

### File: `scanner.rs`

The scanner already extracts inode via `entry_size_and_mtime` (5-tuple including inode and nlink). Currently inode is
used only for the in-memory `HashSet` dedup and then discarded. Change: store it in `EntryRow.inode` for all files.

The in-memory `HashSet` dedup stays — it's more efficient for batch scanning than per-row DB queries.

### File: `writer.rs` — `WriteMessage::UpsertEntryV2`

Add two fields:

```rust
UpsertEntryV2 {
    // ... existing fields ...
    inode: Option<u64>,   // Always set on Unix, for DB storage
    nlink: Option<u64>,   // Hard link count from stat(); dedup query runs when > 1
}
```

**Why two fields**: `inode` is always stored in the DB (keeps the column complete for any future use and for the dedup
query to work). `nlink` gates the dedup query — when nlink ≤ 1, no query needed (fast path for ~95% of files).

### File: `writer.rs` — `UpsertEntryV2` handler

Add dedup logic for **both INSERT and UPDATE** paths. The dedup check runs when all of:
- `inode` is `Some`
- `nlink` is `Some(n)` where `n > 1`
- The caller provided non-NULL sizes (dirs/symlinks always have None sizes → skip)

```
Before calling insert_entry_v2 or update_entry:
  1. Run dedup check:
     INSERT: SELECT 1 FROM entries WHERE inode = ?inode AND logical_size IS NOT NULL LIMIT 1
     UPDATE: SELECT 1 FROM entries WHERE inode = ?inode AND logical_size IS NOT NULL AND id != ?self LIMIT 1

  2. If found → override logical_size and physical_size to None before writing
  3. If not found → keep caller's sizes

  4. Call insert_entry_v2 / update_entry with the (possibly overridden) sizes
```

**Always write inode to DB** regardless of dedup outcome.

**Delta propagation**: Works correctly without changes. On INSERT with overridden NULL sizes: file_count_delta = +1,
size_delta = 0. On UPDATE where old and new sizes are both NULL: delta = 0.

### File: `reconciler.rs`

In `handle_creation_or_modification` (~line 622) and `reconcile_subtree` (~line 386), extract inode and nlink:

```rust
#[cfg(unix)]
let (inode, nlink) = {
    use std::os::unix::fs::MetadataExt;
    (Some(metadata.ino()), Some(metadata.nlink()))
};
#[cfg(not(unix))]
let (inode, nlink) = (None, None);
```

Pass both in `UpsertEntryV2`. The `entry_size_and_mtime` function stays unchanged — the dedup decision is centralized
in the writer.

### File: `verifier.rs`

The verifier builds a `DiskEntry` struct from `readdir` + `stat` in Phase 2, then constructs `UpsertEntryV2` from it
in Phase 3. **Add `inode: Option<u64>` and `nlink: Option<u64>` to `DiskEntry`**, populated during Phase 2 alongside
the existing size/mtime extraction. Then pass through to `UpsertEntryV2` at all three call sites (~lines 217, 249, 279).

Also update `insert_children_from_disk` (~line 470, test-only helper) to include inode/nlink.

### File: `event_loop.rs`

`verify_affected_dirs` (~line 1000) also sends `UpsertEntryV2` — same inode/nlink extraction pattern needed here.

### What does NOT change

- **`AccumulatorMaps::accumulate`** (writer.rs:377): `stats.2 += 1` stays. Each hardlink IS a visible directory entry.
  The scanner already stores NULL sizes for dups, so `unwrap_or(0)` contributes 0 bytes. Count is correct.
- **Aggregator SQL** (`bulk_get_children_stats_by_id`, `scoped_get_children_stats_by_id`): File count and size
  aggregation work correctly once the DB has correct NULL/non-NULL sizes. No SQL changes needed.
- **File count display**: 280K files for `target/debug/` is correct — that's the number of directory entries.
- **Scanner dedup**: The in-memory `HashSet` stays for batch efficiency. The DB inode column is for live dedup only.
- **Delta propagation logic**: Unchanged. Works correctly with the size overrides.
- **Search index** (`search.rs`): Queries specific columns, doesn't select `inode`. No impact.

### Self-healing

When a hardlink is deleted (nlink drops), the remaining link's next event has its current nlink from stat(). If nlink
is now 1, no dedup check runs — sizes are written normally. If the PRIMARY link was deleted and only a secondary
(NULL-sized) link remains, the next reconciler or verifier event for that link restores its sizes (dedup query finds no
other entry with that inode → this becomes the new primary).

**Known limitation**: Self-healing requires an event for the remaining link. If no event fires (the remaining link is
untouched and its directory isn't navigated to), the file stays at NULL until the next full scan. This is a minor
temporary undercounting for a single file — vastly better than 2.5x overcounting across the entire directory tree.

### Verifier noise

After this fix, the verifier will detect a "mismatch" for every secondary hardlink on navigation (DB has NULL size,
disk has real size) and send a `UpsertEntryV2`. The writer's dedup will override sizes back to NULL, producing zero
delta. This is unnecessary traffic but harmless — the writer channel has 20K capacity and this adds at most a few
hundred messages per directory. We can optimize this later (skip size comparison in the verifier when the DB entry has
NULL sizes and the file has nlink > 1) if it becomes a concern.

## Testing

1. **Unit tests** in `store.rs`: test `has_sized_entry_for_inode` query with and without `exclude_id`.
2. **Unit tests** in `writer.rs`: test `UpsertEntryV2` dedup behavior:
   - INSERT primary link → sizes stored, inode stored
   - INSERT secondary link (same inode, nlink > 1) → sizes stored as NULL, inode stored
   - UPDATE secondary link via reconciler → sizes stay NULL (dedup fires)
   - DELETE primary → next UPDATE on secondary → sizes restored (self-healing)
   - UPDATE with nlink=1 (no longer hardlinked) → sizes written normally
3. **Existing tests**: `cargo nextest run indexing` — all pass. Update all existing test call sites that construct
   `UpsertEntryV2` or `EntryRow` to include the new `inode`/`nlink` fields.
4. **Manual verification**: Clear index, rescan, check `target/debug/` tooltip matches `du -sk` (~93 GB, not 233 GB).

## Verification

1. `cd apps/desktop/src-tauri && cargo nextest run indexing`
2. `./scripts/check.sh --rust`
3. Clear index in the app (Settings → Debug → Clear index), let it rescan
4. Compare `target/debug/` tooltip with `du -sk ~/projects-git/vdavid/cmdr/target/debug`
5. Wait for a few watcher events (modify a file in target/), re-check sizes haven't inflated
