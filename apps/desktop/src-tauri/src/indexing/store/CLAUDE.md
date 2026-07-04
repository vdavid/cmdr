# Index store (SQLite)

The `IndexStore` read/write handle and the per-volume SQLite schema for the drive indexer. Split by concern: `mod.rs`
(schema + `platform_case` collation + `IndexStore` + data types), `connection.rs` (open/recreate + connection
factories), `entries.rs` (entry-tree CRUD), `dir_stats.rs`, `meta.rs`; tests in `tests.rs`. Parent pipeline:
[`../CLAUDE.md`](../CLAUDE.md).

## Must-knows

- **Register the `platform_case` collation on every connection** (it isn't persisted). Every read/write connection is
  opened through this module's factories (`open_read_connection` / the writer's) so the collation is always present; open
  a new connection any other way — or run the raw `sqlite3` CLI — and any query touching the name column fails. Use
  `index-query` for ad-hoc reads.
- **Don't drop `UNIQUE (parent_id, name_folded)` (the `idx_parent_name_folded` index) nor the `name_folded` column, and
  insert with `INSERT OR IGNORE`, never `INSERT OR REPLACE`.** The UNIQUE constraint is the safety net against two
  writers double-inserting a row (observed once as a 1.83 TB ghost size); `OR REPLACE` would reassign integer IDs and
  orphan children; `name_folded` is the pre-folded key that keeps the composite index binary-collated and fast.
- **The index is a disposable cache**: a schema-version mismatch or corruption deletes the DB file and recreates it fresh
  (`delete_and_recreate`) — reclaims disk with no freelist, no online migrations. Bump `SCHEMA_VERSION` (in `mod.rs`) for
  any schema change; there's no migration path by design.

The schema columns and the honest-sizes epoch model that shares them (`listed_epoch`, `min_subtree_epoch`,
`current_epoch`), plus the module structure: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
