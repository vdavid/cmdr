# Index store (SQLite) details

Depth for `src-tauri/src/indexing/store/`: the `IndexStore` handle and the concern-split CRUD. Must-know invariants
live in `CLAUDE.md`. The SQLite schema itself is described below; the honest-sizes epoch model that shares its columns
lives in `../writer/DETAILS.md` § "Honest sizes", and the broader indexing pipeline in `../DETAILS.md`.

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
  rename / move / delete, counts, `get_next_id`. Whole-index consumers get three shapes, in descending cost:
  `all_entries` (every row, hundreds of MB on a NAS), `all_directories` (folders only, as full `EntryRow`s), and the
  streaming `for_each_directory` / `for_each_file_child`, which hand out only the columns path reconstruction and
  per-parent folding need and so let the caller hold a compact structure instead of a row per entry. Reach for a
  streaming one unless the consumer genuinely wants the metadata. `delete_descendants_by_id` descends in bounded chunks
  rather than issuing one recursive-CTE `DELETE` (which materialized 10.9M ids into a single ephemeral table and
  transaction on a real index), and deletes POST-ORDER (see below).
- `dir_tree.rs`: `DirTree`, the compact in-memory projection of the directory rows that `for_each_directory`
  exists to feed — one name arena plus a 24-byte `(id, parent_id, name slice)` record per folder, id-ordered and
  binary-searched. The shape every whole-index walk (`media_index`'s image walk, `importance`'s recompute walk)
  reconstructs paths from; measurements and the alternatives weighed live in
  `apps/desktop/src-tauri/src/media_index/scheduler/DETAILS.md`.
- `dir_stats.rs`: `dir_stats` reads and writes plus `recompute_min_subtree_epoch`.
- `meta.rs`: meta-table + epoch helpers, `mark_dirs_listed`, `get_all_directory_paths`, `clear_all`, and the
  aggregates-are-known-good marker (`ledger_heal_done` / `mark_ledger_heal_done` / `clear_ledger_heal_done`, keyed on
  `LEDGER_HEAL_KEY`). Its absence means the aggregates are UNPAID and the next launch rebuilds them: a never-healed
  pre-ledger DB, or a bulk walk that suppressed ancestor propagation and hasn't run its terminal aggregate yet. See the
  `../writer/DETAILS.md` § "The dir_stats ledger".

`resolve_component` always queries by `(parent_id, name_folded)` using the `idx_parent_name_folded` composite **UNIQUE**
index. On Linux/Windows `normalize_for_comparison()` is the identity function, so `name_folded = name` and the index
behaves identically to a `(parent_id, name)` index. A schema-version mismatch triggers drop+rebuild.
`IndexStoreError` carries the typed SQLite classifiers callers branch on (never the message string): `sqlite_code()`,
`is_fatal_storage_error()`, `as_index_failure()`, `is_primary_key_conflict()`, `is_transient_lock_error()`, and
`indicates_corruption()`. `is_primary_key_conflict()` separates an `entries.id` collision (extended 1555, the writer
heals it by resyncing its counter) from a `(parent_id, name_folded)` conflict (2067, which must never be retried under a
fresh id); rationale and the writer side: `../writer/DETAILS.md` § "Decision: a PRIMARY KEY conflict".

**`with_savepoint` releases on the error path too (load-bearing).** The failure arm runs
`ROLLBACK TO <name>; RELEASE <name>`. `ROLLBACK TO` alone undoes the work but leaves the savepoint — and the implicit
transaction it opened — in place, so a single failed `upsert_dir_stats_by_id` / `insert_entries_v2_batch` /
`mark_dirs_listed` would park the writer's connection in an open transaction holding the write lock: every other
connection then sees `database is locked` indefinitely, and the writer's own later writes never commit. Regression:
`store::tests::a_failed_savepoint_call_leaves_the_connection_in_autocommit`.

## Scan calibration is stored PER WALK KIND

The frontend's tier-1 progress denominator and its ETA seed come from the previous scan's `total_entries` and
`scan_duration_ms` in `meta`. The two walks that write them differ by roughly 5x in wall clock on the same volume (the
parallel truncate-and-rebuild vs the serial per-directory change check), so one slot for both means each run predicts
the other's time. Measured on the boot disk: a ~3 minute rebuild seeded from a 1,180,696 ms change check would promise
~20 minutes, and the reverse promises 3 minutes for a 20-minute run.

So every completed scan writes its numbers TWICE:

- **`<key>_full_walk` / `<key>_change_check`** — the per-kind bucket, keyed by `ScanCalibrationKind::meta_key(base)` for
  `total_entries`, `total_physical_bytes`, and `scan_duration_ms`. `FullWalk` covers both truncating runs (a first scan
  and a full rebuild are the same walker, so they calibrate each other).
- **The unsuffixed `<key>`** — the last completed scan of any kind. This is what `VolumeIndexStatus.scan_duration_ms`
  (the badge's "took N min" footer) and `IndexStatus` read, and it doubles as the fallback bucket.

`IndexStore::read_scan_calibration_set` reads all three buckets; `ScanCalibrationSet::for_kind(kind)` picks one:
same-kind if it holds anything, else the unsuffixed last-scan bucket, else empty (the caller then falls back to the
rough, untimed tier). The any-kind rung is deliberate: on the first-ever change check a full walk's timing is wrong-ish
but honest company, and better than showing no estimate at all. It also covers a DB written before the per-kind keys
existed, with no migration — the index is a disposable cache (`../CLAUDE.md` § "Rebuild, don't migrate"), so a missing
per-kind key is just "no same-kind calibration yet".

Which bucket a run reads and writes is decided ONCE, by `events::ScanRunKind::calibration_kind()` at the scan-start
funnel, and threaded to the completion handler (`lifecycle/scan_completion.rs` for local,
`lifecycle/network_scan.rs`'s completion arm for SMB/MTP). Pinned by `store::tests::calibration_for_kind_*`.

## Decision: a subtree delete is post-order, so an interruption can never strand rows

`delete_descendants_by_id` deletes files on the way down (a leaf can't strand anything), banks each level's directory
ids, then deletes the directories deepest level first. A directory row therefore goes only once its whole subtree has,
which means every instant of the run leaves a tree still walkable from the index root, and a re-run from the same root
finds exactly what's left.

**Why the ordering is load-bearing.** The delete is autocommitted per batch, so a quit or crash freezes it wherever it
got to. Top-down, that severs the tree at the cut and every row below loses its path to the root — invisible to any
later descent, so nothing can ever collect it. On a copy of the author's production QNAP index one interrupted run left
12 442 990 rows of which 9 793 362 were unreachable (910 316 of them directly parentless), and a relaunch collected
nothing (2026-07-25). Reading children by `parent_id` is what makes the order free to choose: a deleted parent row never
hides its children. Pinned by `tests.rs::interrupting_a_subtree_delete_never_strands_a_row`, which asserts zero orphans
after EVERY prefix of the deletion order (via the `#[cfg(test)]` `delete_descendants_by_id_stopping_after`, whose
mid-batch stops are a superset of the points a real crash can reach, and the `#[cfg(test)]` `find_orphan_entries`).

**Cost of the ordering.** Post-order retains the directory ids of all levels instead of one frontier: 324 128 ids
(2.6 MB) across seven levels on that index, versus a 5 951-id peak for the old single frontier. Both stay orders of
magnitude under the ~87 MB the recursive-CTE form materializes, and files never accumulate at all.

There is deliberately NO repair pass for rows already stranded that way, and there shouldn't be one: an index is a
disposable cache, so a damaged one is invalidated and rebuilt (`../CLAUDE.md` § "Rebuild, don't migrate").

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

## Decision: read connections get an 8x smaller page cache than the write connection

`apply_pragmas` takes a `readonly` flag and delegates the page-cache budget to `crate::sqlite_util::apply_page_cache`,
which every store's `apply_pragmas` shares: `WRITE_PAGE_CACHE_KIB` (16 MiB) for a write connection,
`READ_PAGE_CACHE_KIB` (2 MiB) for a read-only one. It's one helper rather than five copies of a literal precisely
because the failure mode is silent: a store that keeps its own number drifts back up and nothing complains.

**Why the write budget is 16 MiB.** It's coupled to `wal_autocheckpoint = 4000` (~16 MiB of 4 KiB pages, set in the
same function): the cache is sized to hold what a whole autocheckpoint window dirties, so a big write batch commits
without evicting pages it's about to touch again. Change one and reconsider the other. There is at most ONE write
connection per DB (the single writer thread), so this budget is paid a handful of times process-wide.

**Why reads need far less.** Read connections are the many. They're thread-local and live as long as their thread
(`../read/enrichment.rs`'s `THREAD_CONN`, `importance/read/mod.rs`'s `READ_CONN`), so the count tracks tokio's
blocking-thread pool rather than anything semantic. A profiled prod session (v0.36.2, ~10 h uptime, macOS 26.5.2,
`lsof` + `footprint -s`, 2026-07-28) had **156 open connections** across 69 blocking threads (57 × `importance-root.db`,
53 × `index-root.db`, 30 + 10 on the NAS volume, 6 × `media-root.db`), holding ~1.15 GB of a 2.5 GB ceiling. At 2 MiB
the same 156 connections cap at ~310 MB.

2 MiB is SQLite's own default. It comfortably holds the upper interior levels of the hot b-trees, which is what the
enrichment path actually needs: point lookups on `(parent_id, name_folded)` and one directory's worth of range scan. A
whole-index working set never fit at 16 MiB either (a 6.9M-row index runs ~170 k leaf pages), so the big budget was
buying leaf-page retention that the OS file cache already backs. Reads never commit or checkpoint, so nothing here
touches the WAL. **The fix is a ceiling, not a cure** — the connections still accumulate; bounding what each one costs
is what M1 buys.

**Test coverage**: `read_connections_get_a_smaller_page_cache_than_write_connections`, one per store (`tests.rs` here
and in `importance/store`, `media_index/store`, `agent/store`, `operation_log/store`), asserts the exact KiB each role
opens with, so a future edit can't quietly re-inflate reads.
