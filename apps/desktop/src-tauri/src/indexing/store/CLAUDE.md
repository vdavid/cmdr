# Index store (SQLite)

The `IndexStore` read/write handle and the per-volume SQLite schema for the drive indexer. Split by concern: `mod.rs`
(schema + `platform_case` collation + `IndexStore` + data types), `connection.rs` (open/recreate + connection
factories), `entries.rs` (entry-tree CRUD), `dir_tree.rs` (`DirTree`, the compact directory projection whole-index
walks reconstruct paths from), `dir_stats.rs`, `meta.rs`; tests in `tests.rs`. Parent pipeline:
`../CLAUDE.md`.

## Must-knows

- **Register the `platform_case` collation on every connection** (it isn't persisted). Every read/write connection is
  opened through this module's factories (`open_read_connection` / the writer's) so the collation is always present; open
  a new connection any other way — or run the raw `sqlite3` CLI — and any query touching the name column fails. Use
  `index-query` for ad-hoc reads.
- **Don't drop `UNIQUE (parent_id, name_folded)` (the `idx_parent_name_folded` index) nor the `name_folded` column, and
  insert with `INSERT OR IGNORE`, never `INSERT OR REPLACE`.** The UNIQUE constraint is the safety net against two
  writers double-inserting a row (observed once as a 1.83 TB ghost size); `OR REPLACE` would reassign integer IDs and
  orphan children; `name_folded` is the pre-folded key that keeps the composite index binary-collated and fast.
- **Ask "are there more than N children?" with `count_children_capped`, never `COUNT(*)`.** It wraps the count in an
  inner `SELECT 1 … LIMIT`, so the answer reads at most `cap` rows off the `parent_id` index. A plain `COUNT(*)` is
  O(children), which is exactly the cost the caller (verification's tooth-1 probe) exists to avoid on a 1.14M-child
  directory.
- **Subtree deletes go POST-ORDER; never make one top-down.** `delete_descendants_by_id` drops files on the way down
  and directories deepest-level-first, so any interruption still leaves a tree walkable from the root and a re-run
  finishes it. Top-down severs the tree: one interrupted bulk delete stranded 9 793 362 rows permanently, invisible to
  every later pass and to any repair short of a rebuild. DETAILS § "Decision: a subtree delete is post-order".
- **The index is a disposable cache, but only PROVEN garbage is thrown away.** A schema-version mismatch or corruption
  (`indicates_corruption()`: `SQLITE_CORRUPT*` / `SQLITE_NOTADB`) deletes the DB file and recreates it fresh
  (`delete_and_recreate`), reclaiming disk with no freelist; no online migrations. Every OTHER `open` failure keeps the
  file: `SQLITE_BUSY` / `LOCKED` retry with backoff, and anything else (full disk, read-only volume, `IOERR`, unknown
  code) returns an error. Never widen `indicates_corruption()`; rebuilding a real index costs tens of minutes. Bump
  `SCHEMA_VERSION` (in `mod.rs`) for any schema change; there's no migration path by design.

- **Scan calibration lives in PER-WALK-KIND `meta` buckets, never one slot.** A truncating full walk and a
  rescan-in-place change check differ ~5x in wall clock, so sharing `scan_duration_ms` / `total_entries` makes each run
  predict the other's time. Every completion writes the suffixed keys (`ScanCalibrationKind::meta_key`) AND the
  unsuffixed last-scan ones; reads go through `read_scan_calibration_set` + `ScanCalibrationSet::for_kind`, which falls
  back same-kind → any-kind → nothing. DETAILS § "Scan calibration is stored PER WALK KIND".

The schema columns and the honest-sizes epoch model that shares them (`listed_epoch`, `min_subtree_epoch`,
`current_epoch`), plus the module structure: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
