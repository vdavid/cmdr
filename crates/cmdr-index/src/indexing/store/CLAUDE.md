# Index store (SQLite)

The `IndexStore` handle and per-volume SQLite schema for the drive indexer. `mod.rs` (schema + `platform_case` collation
+ `IndexStore` + data types), `connection.rs` (open/recreate + factories), `entries.rs` (entry-tree CRUD), `dir_tree.rs`
(the compact directory projection whole-index walks rebuild paths from), `dir_stats.rs`, `meta.rs`, `tests/`. Parent
pipeline: `../CLAUDE.md`.

## Must-knows

- **Register `platform_case` on every connection; it isn't persisted.** This module's factories always do. Raw
  `rusqlite` or the `sqlite3` CLI fails on any name-column query — use `index-query` for ad-hoc reads.
- **Keep `UNIQUE (parent_id, name_folded)` (`idx_parent_name_folded`) and the `name_folded` column, and insert with
  `INSERT OR IGNORE`, never `OR REPLACE`.** The constraint is the net against two writers double-inserting (seen once as
  a 1.83 TB ghost size); `OR REPLACE` reassigns integer IDs and orphans children.
- **"More than N children?" is `count_children_capped`, never `COUNT(*)`**: `cap` rows off the `parent_id` index instead
  of O(children) on a 1.14M-child directory.
- **Subtree deletes go POST-ORDER; never make one top-down.** Interrupted post-order still leaves a tree walkable from
  the root that a re-run finishes; top-down severs it, and one stranded 9 793 362 rows permanently. DETAILS § "a subtree
  delete is post-order".
- **The index is a disposable cache, but only PROVEN garbage is thrown away.** Schema-version mismatch or
  `indicates_corruption()` deletes and recreates; `BUSY` / `LOCKED` retry; every other `open` failure errors and KEEPS
  the file. ❌ Never widen `indicates_corruption()`; a real rebuild costs tens of minutes. Bump `SCHEMA_VERSION` for any
  schema change — no migrations, by design.
- **❌ Stamp `EXCLUSION_POLICY_KEY` only on a provably empty DB**: right after a `TruncateData`, or the bare `ROOT`
  sentinel. Absent or stale ⇒ no coverage claim is trusted and every search walks its whole scope. DETAILS § "What
  coverage needs".
- **Open only via `crate::sqlite_util::{open, open_read_only, open_in_memory}`** (enforced by
  `desktop-rust-sqlite-open-direct`). They install the process-wide 64 MiB page-cache slab; the first direct
  `rusqlite::Connection::open*` initializes SQLite and locks the slab out for good.
- **Hot writes use `prepare_cached`, and the cache (64, writers only) must outsize the store's 38 sites.** `rusqlite`'s
  default 16 silently re-compiles: no error, no failing test.
- **Scan calibration lives in PER-WALK-KIND `meta` buckets, never one slot.** A truncating full walk and a
  rescan-in-place differ ~5x, so sharing makes each run predict the other's time. Write suffixed
  (`ScanCalibrationKind::meta_key`) plus unsuffixed keys; read via `read_scan_calibration_set`.

Schema columns, the honest-sizes epoch model that shares them (`listed_epoch`, `min_subtree_epoch`, `current_epoch`),
and the module structure: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
