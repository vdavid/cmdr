# Index store (SQLite) details

Depth for `src-tauri/src/indexing/store/`: the `IndexStore` handle and the concern-split CRUD. Must-know invariants
live in [CLAUDE.md](CLAUDE.md). The SQLite schema itself and the honest-sizes epoch model that shares its columns are
one coupled mechanism and live in the parent [`../DETAILS.md`](../DETAILS.md) § "SQLite schema" + § "Honest sizes"; the
broader indexing pipeline is the rest of that file.

## Module structure

The `IndexStore` read/write handle and SQLite schema, split into a `store/` submodule by concern. `mod.rs` holds the
shared core: the schema (integer-keyed entries with `name_folded` on all platforms, `inode` for hardlink dedup,
`dir_stats` by entry_id, `meta`), `platform_case` collation, DDL/pragmas/reset, the path helpers (`resolve_path`,
`reconstruct_path*`), the `IndexStore` struct + `with_savepoint`, and the data types (`EntryRow`, `DirStats`,
`DirStatsById`, `ScanContext`, `IndexStatus`, `ScanCalibration`, `IndexStoreError`); the `tests` module lives in the
sibling `tests.rs`.

The `impl IndexStore` block is divided into four sibling files (each `impl IndexStore { … }` over the struct above,
pulling shared items via `use super::*`):

- `connection.rs`: open/recreate, connection factories, DB-size + status reads, the `pub(super)` `read_meta_value`
  helper.
- `entries.rs`: entry-tree reads and writes — child listings, lookups by id / inode / component, insert / update /
  rename / move / delete, counts, `get_next_id`.
- `dir_stats.rs`: `dir_stats` reads and writes plus `recompute_min_subtree_epoch`.
- `meta.rs`: meta-table + epoch helpers, `mark_dirs_listed`, `get_all_directory_paths`, `clear_all`, and the
  aggregates-are-known-good marker (`ledger_heal_done` / `mark_ledger_heal_done` / `clear_ledger_heal_done`, keyed on
  `LEDGER_HEAL_KEY`). Its absence means the aggregates are UNPAID and the next launch rebuilds them: a never-healed
  pre-ledger DB, or a bulk walk that suppressed ancestor propagation and hasn't run its terminal aggregate yet. See the
  parent `DETAILS.md` § "The dir_stats ledger".

`resolve_component` always queries by `(parent_id, name_folded)` using the `idx_parent_name_folded` composite **UNIQUE**
index. On Linux/Windows `normalize_for_comparison()` is the identity function, so `name_folded = name` and the index
behaves identically to a `(parent_id, name)` index. A schema-version mismatch triggers drop+rebuild.
`IndexStoreError` carries the typed SQLite classifiers callers branch on (never the message string): `sqlite_code()`,
`is_fatal_storage_error()`, `as_index_failure()`, `is_primary_key_conflict()`, `is_transient_lock_error()`, and
`indicates_corruption()`. `is_primary_key_conflict()` separates an `entries.id` collision (extended 1555, the writer
heals it by resyncing its counter) from a `(parent_id, name_folded)` conflict (2067, which must never be retried under a
fresh id); rationale and the writer side: parent [`DETAILS.md`](../DETAILS.md) § "Decision: a PRIMARY KEY conflict on an
upsert insert resyncs the counter and retries once".

**`with_savepoint` releases on the error path too (load-bearing).** The failure arm runs
`ROLLBACK TO <name>; RELEASE <name>`. `ROLLBACK TO` alone undoes the work but leaves the savepoint — and the implicit
transaction it opened — in place, so a single failed `upsert_dir_stats_by_id` / `insert_entries_v2_batch` /
`mark_dirs_listed` would park the writer's connection in an open transaction holding the write lock: every other
connection then sees `database is locked` indefinitely, and the writer's own later writes never commit. Regression:
`store::tests::a_failed_savepoint_call_leaves_the_connection_in_autocommit`.

## Decision: only proven corruption deletes an index; everything else fails loudly

`IndexStore::open` classifies a `try_open` failure by typed SQLite code and picks one of three branches:

- **Delete and recreate**: a `SchemaMismatch` (a clean upgrade, logged at info) or `indicates_corruption()`
  (`SQLITE_CORRUPT*`, `SQLITE_NOTADB`: the bytes are provably unusable, logged at warn).
- **Retry**: `is_transient_lock_error()` (`SQLITE_BUSY`, `SQLITE_LOCKED`, `SQLITE_PROTOCOL`) backs off per
  `OPEN_RETRY_BACKOFF_MS` (100 ms, 300 ms, so three attempts and at most 400 ms of added latency), then returns the
  error.
- **Return the error, file untouched**: everything else, including the storage-death classes `SQLITE_IOERR`,
  `SQLITE_FULL`, `SQLITE_READONLY`, and `SQLITE_CANTOPEN`, plus any code we don't recognize.

**Why**: "the index is a disposable cache" justifies deleting on a schema bump or a corrupt file, but not on a
transient or environmental one. A real index holds millions of entries (6.9M on the author's machine) and costs tens of
minutes plus heavy disk churn to rebuild, so a checkpoint-length write lock, a momentarily full disk, or a read-only
volume must never destroy it. Deleting is the destructive branch, so it carries the burden of proof: `is_fatal_storage_error()`
(which stops the index) is deliberately WIDER than `indicates_corruption()` (which throws the file away), and an
unrecognized code takes the conservative branch. Don't widen `indicates_corruption()` without the same standard of proof.

Both production callers (`IndexManager::new_for_kind`, `start_indexing_for` in `state.rs`) already map the error to a
`String` and abort the start, so a hard failure surfaces as "indexing didn't start" rather than a panic or a silently
empty index; the on-disk DB is still there for the next attempt.

`apply_pragmas` sets `busy_timeout` FIRST, before `journal_mode = WAL` and the root-sentinel insert. Both take a lock,
and a busy handler that isn't installed yet can't back them off, so the ordering is what makes contention transient in
the first place; the retry loop above is the second line of defense.

**Test coverage** (`tests.rs`): `busy_db_is_retried_not_deleted` induces a real `SQLITE_BUSY` (a second connection holds
`BEGIN EXCLUSIVE` past the 5 s `busy_timeout`, hence the test's ~6 s runtime) and asserts the entries survive;
`unwritable_db_is_not_deleted_on_open_failure` chmods the file to 0444; `corruption_recovery_deletes_and_recreates` and
the two schema-mismatch tests keep the recreate paths intact.

`has_sized_entry_for_inode()` checks whether another entry with the same inode already has non-NULL sizes;
`find_entry_by_inode()` returns the first row with a given inode (the live event loop's rename pre-pass). Both path-keyed
(backward compat) and integer-keyed APIs exist.
