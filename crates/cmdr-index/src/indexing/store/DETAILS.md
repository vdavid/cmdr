# Index store (SQLite) details

Depth for `crates/cmdr-index/src/indexing/store/`: the `IndexStore` handle and the concern-split CRUD. Must-know
invariants live in `CLAUDE.md`. The SQLite schema itself is described below; the honest-sizes epoch model that shares
its columns lives in `../writer/DETAILS.md` § "Honest sizes", and the broader indexing pipeline in `../DETAILS.md`.

## Module structure

The `IndexStore` read/write handle and SQLite schema, split into a `store/` submodule by concern. `mod.rs` is the hub:
the `IndexStore` struct + `with_savepoint`, the data types (`EntryRow`, `DirStats`, `DirStatsById`, `ScanContext`,
`IndexStatus`, `ScanCalibration`), and the submodule declarations plus the re-exports that keep every existing
`store::X` path working. The `tests` module lives in the sibling `tests/`, one themed module per concern.

Four leaf layers carry no `IndexStore` methods at all:

- `schema.rs`: `SCHEMA_VERSION`, the `meta` key constants (`CURRENT_EPOCH_KEY`, `LEDGER_HEAL_KEY`,
  `SYSTEM_DIR_EXCLUSIONS_KEY`, `EXCLUSION_POLICY_KEY`), `ROOT_ID` / `ROOT_PARENT_ID`, the table DDL (integer-keyed
  entries with `name_folded` on all platforms, `inode` for hardlink dedup, `dir_stats` by entry_id, `meta`),
  `create_tables` / `ensure_root_sentinel` / `reset_schema`, and `apply_pragmas`.
- `errors.rs`: `IndexStoreError` and its SQLite classification (fatal / transient / corruption / primary-key conflict),
  the typed `IndexFailure` the `Failed` phase carries, and `UnreadableCause`.
- `paths.rs`: `resolve_scan_root`, `resolve_path`, `resolve_path_under`, `reconstruct_path`,
  `reconstruct_path_from_map`.
- `collation.rs`: `register_platform_case_collation`, `platform_case_compare`, `normalize_for_comparison`.

The `impl IndexStore` block is divided into four more sibling files (each `impl IndexStore { … }` over the struct in
`mod.rs`, pulling shared items via `use super::*`):

- `connection.rs`: open/recreate, connection factories, DB-size + status reads, the `pub(super)` `read_meta_value`
  helper.
- `entries.rs`: entry-tree reads and writes — child listings, lookups by id / inode / component, insert / update /
  rename / move / delete, counts, `get_next_id`. Whole-index consumers get three shapes, in descending cost:
  `all_entries` (every row, hundreds of MB on a NAS), `all_directories` (folders only, as full `EntryRow`s), and the
  streaming `for_each_directory` / `for_each_file_child`, which hand out only the columns path reconstruction and
  per-parent folding need and so let the caller hold a compact structure instead of a row per entry. Reach for a
  streaming one unless the consumer genuinely wants the metadata. `delete_descendants_by_id` descends in bounded chunks
  rather than issuing one recursive-CTE `DELETE` (which materialized 10.9M ids into a single ephemeral table and
  transaction on a real index), and deletes POST-ORDER (see below). `for_each_child_directory_of` /
  `for_each_child_file_of` are the SCOPED counterparts: same columns, but for a batch of parent ids at a time, so a
  consumer reading one subtree expands a whole level per query instead of scanning the table. Both are served by
  `idx_parent_name_folded`'s leading `parent_id`; the file one keeps `for_each_file_child_by_parent`'s
  `ORDER BY parent_id` group contract (each parent id sits in exactly one chunk, so chunking never splits a group). The
  importance incremental rescore is the consumer (`../../importance/scheduler/DETAILS.md` § The scoped walk).
- `dir_tree.rs`: `DirTree`, the compact in-memory projection of the directory rows that `for_each_directory` exists to
  feed — one name arena plus a 24-byte `(id, parent_id, name slice)` record per folder, id-ordered and binary-searched.
  The shape every whole-index walk (`media_index`'s image walk, `importance`'s recompute walk) reconstructs paths from;
  measurements and the alternatives weighed live in `crates/cmdr-index/src/media_index/scheduler/DETAILS.md`.
- `dir_stats.rs`: `dir_stats` reads and writes plus `recompute_min_subtree_epoch`.
- `meta.rs`: meta-table + epoch helpers, `mark_dirs_listed`, `mark_dirs_unreadable`, `read_high_water_id`,
  `get_all_directory_paths`, `clear_all`, and the aggregates-are-known-good marker (`ledger_heal_done` /
  `mark_ledger_heal_done` / `clear_ledger_heal_done`, keyed on `LEDGER_HEAL_KEY`). Its absence means the aggregates are
  UNPAID and the next launch rebuilds them: a never-healed pre-ledger DB, or a bulk walk that suppressed ancestor
  propagation and hasn't run its terminal aggregate yet. See the `../writer/DETAILS.md` § "The dir_stats ledger".

`resolve_component` always queries by `(parent_id, name_folded)` using the `idx_parent_name_folded` composite **UNIQUE**
index. On Linux/Windows `normalize_for_comparison()` is the identity function, so `name_folded = name` and the index
behaves identically to a `(parent_id, name)` index. A schema-version mismatch triggers drop+rebuild. `IndexStoreError`
carries the typed SQLite classifiers callers branch on (never the message string): `sqlite_code()`,
`is_fatal_storage_error()`, `as_index_failure()`, `is_primary_key_conflict()`, `is_transient_lock_error()`, and
`indicates_corruption()`. `is_primary_key_conflict()` separates an `entries.id` collision (extended 1555, the writer
heals it by resyncing its counter) from a `(parent_id, name_folded)` conflict (2067, which must never be retried under a
fresh id); rationale and the writer side: `../writer/DETAILS.md` § "Decision: a PRIMARY KEY conflict".

**`with_savepoint` releases on the error path too (load-bearing).** The failure arm runs
`ROLLBACK TO <name>; RELEASE <name>`. `ROLLBACK TO` alone undoes the work but leaves the savepoint — and the implicit
transaction it opened — in place, so a single failed `upsert_dir_stats_by_id` / `insert_entries_v2_batch` /
`mark_dirs_listed` would park the writer's connection in an open transaction holding the write lock: every other
connection then sees `database is locked` indefinitely, and the writer's own later writes never commit. Regression:
`store::tests::open_and_recover::a_failed_savepoint_call_leaves_the_connection_in_autocommit`.

## What coverage needs

Two additions serve the search frontier (`../read/DETAILS.md` § "The coverage frontier"), and neither is optional for
it: the descent rule reads one and refuses to answer without the other.

**`entries.unreadable_cause`** (schema v16) marks a directory nothing is going to read into, and says WHY:
`UnreadableCause::Denied` (1) is a walk that tried and was refused, `Declined` (2) is one no walk will read at all (a
NAS snapshot tree). `0` is the ordinary state, "something may yet read this". A CAUSE rather than a flag because the two
reach the user as different sentences and only the first is one they can act on; telling them apart from the paths would
mean matching folder names, which isn't an option. An unknown stored value reads as `Denied`, the truthful half of any
cause a future schema could add. Deliberately NOT folded into `listed_epoch`: an unreadable directory was never listed,
so it stays at `0` and keeps absorbing its ancestors' `min_subtree_epoch` to `0`, which is what keeps sizes honest. What
the marker buys is that the frontier can SKIP it instead of handing it to the walk again on every single search —
without it, a permission-denied subtree is a permanent repeating slow path with no user signal. The local walk stamps it
(`MarkDirsUnreadable`, sent after its marks) for a read that failed with PERMISSION DENIED only: a stall timeout means a
dead mount or a storm, both of which heal, and pinning those would stop the retry. `mark_dirs_listed` CLEARS the column
in the same `UPDATE` that stamps `listed_epoch` — a directory we just listed is by definition readable again — so
granting Full Disk Access heals it with no rebuild and no separate pass. `mark_dirs_unreadable(conn, ids, None)` is the
explicit clear, for a caller that has a reason to reset it without a listing.

**`meta.exclusion_policy_built_for`** (`EXCLUSION_POLICY_KEY`) records WHICH scan-exclusion policy the DB's rows were
written under: an FNV-1a fingerprint of `EXCLUDED_PREFIXES`, `JUNK_BASENAMES`, `PSEUDO_FS_BASENAMES`, and (macOS)
`FIRMLINKED_SYSTEM_PREFIXES`, content-derived so editing any list re-arms every existing index with no version constant
for anyone to forget to bump. Why it exists: an excluded directory gets no `entries` row at all, so it drives nothing to
zero and its parents read as fully covered — true only while the policy is the one the rows were written under. REMOVE a
name and the subtrees it used to skip stay row-less while their parents keep claiming coverage: permanently invisible to
search, with nothing to trigger a re-walk. So an absent or stale stamp means **no coverage claim in that database is
trusted** and the whole scope goes to the walk.

❌ **Stamp it ONLY while the DB provably holds no row beneath a directory today's policy excludes**, which is exactly
two moments: right after a `TruncateData`, and on a database that has never held an entry at all (`entry_count <= 1`,
the `ROOT` sentinel alone — `lifecycle/state.rs`'s `prepare_database_for_a_walk`, where a search-driven walk stands a
cold volume's index up and would otherwise write rows nothing ever trusts). A reconcile or a scoped fill over an index
that ALREADY holds rows never re-lists the volume, so it can't clear what an older policy let in, and must leave the
stamp alone. The three sites: `lifecycle/manager/start.rs` (local), `lifecycle/network_scan.rs` (SMB/MTP, alongside the
NAS list's own `SYSTEM_DIR_EXCLUSIONS_KEY` stamp, which is the same pattern for a different list), and the cold
bootstrap above. `CMDR_E2E_START_PATH` is deliberately outside the fingerprint: it narrows the effective policy at
runtime but it's a per-run fixture path, and folding it in would write a machine-specific value into every E2E index.

**`read_high_water_id`** is `MAX(id)` on `entries`, the cheap write watermark half of `CoverageToken`. A watermark, not
a count: ids come from one monotonic per-volume counter, so any walk that inserts rows raises it, at the cost of a seek
to the end of the primary-key index instead of the `O(n)` a `COUNT(*)` would pay.

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
funnel, and threaded to the completion handler (`lifecycle/scan_completion.rs` for local, `lifecycle/network_scan.rs`'s
completion arm for SMB/MTP). Pinned by `store::tests::meta_and_calibration::calibration_for_kind_*`.

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
hides its children. Pinned by `tests/subtree_deletes.rs::interrupting_a_subtree_delete_never_strands_a_row`, which
asserts zero orphans after EVERY prefix of the deletion order (via the `#[cfg(test)]`
`delete_descendants_by_id_stopping_after`, whose mid-batch stops are a superset of the points a real crash can reach,
and the `#[cfg(test)]` `find_orphan_entries`).

**Cost of the ordering.** Post-order retains the directory ids of all levels instead of one frontier: 324 128 ids (2.6
MB) across seven levels on that index, versus a 5 951-id peak for the old single frontier. Both stay orders of magnitude
under the ~87 MB the recursive-CTE form materializes, and files never accumulate at all.

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

**Why**: "the index is a disposable cache" justifies deleting on a schema bump or a corrupt file, but not on a transient
or environmental one. A real index holds millions of entries (6.9M on the author's machine) and costs tens of minutes
plus heavy disk churn to rebuild, so a checkpoint-length write lock, a momentarily full disk, or a read-only volume must
never destroy it. Deleting is the destructive branch, so it carries the burden of proof: `is_fatal_storage_error()`
(which stops the index) is deliberately WIDER than `indicates_corruption()` (which throws the file away), and an
unrecognized code takes the conservative branch. Don't widen `indicates_corruption()` without the same standard of
proof.

Both production callers (`IndexManager::new_for_kind`, `start_indexing_for` in `state.rs`) already map the error to a
`String` and abort the start, so a hard failure surfaces as "indexing didn't start" rather than a panic or a silently
empty index; the on-disk DB is still there for the next attempt.

`apply_pragmas` sets `busy_timeout` FIRST, before `journal_mode = WAL` and the root-sentinel insert. Both take a lock,
and a busy handler that isn't installed yet can't back them off, so the ordering is what makes contention transient in
the first place; the retry loop above is the second line of defense.

**Test coverage** (`tests/open_and_recover.rs`): `busy_db_is_retried_not_deleted` induces a real `SQLITE_BUSY` (a second
connection holds `BEGIN EXCLUSIVE` past the 5 s `busy_timeout`, hence the test's ~6 s runtime) and asserts the entries
survive; `unwritable_db_is_not_deleted_on_open_failure` chmods the file to 0444;
`corruption_recovery_deletes_and_recreates` and the two schema-mismatch tests keep the recreate paths intact.

`has_sized_entry_for_inode()` checks whether another entry with the same inode already has non-NULL sizes;
`find_entry_by_inode()` returns the first row with a given inode (the live event loop's rename pre-pass). Both
path-keyed (backward compat) and integer-keyed APIs exist.

## Decision: SQLite page memory is one process-wide slab, not a per-connection budget

The canonical home for the whole app's SQLite memory model; the code is `crate::sqlite_util`.

**The problem.** Read connections are thread-local and live as long as their thread (`../read/enrichment.rs`'s
`THREAD_CONNS`, `importance/read/mod.rs`'s `READ_CONNS`), so their count tracks tokio's blocking-thread pool rather than
anything semantic. A profiled prod session (v0.36.2, ~10 h uptime, macOS 26.5.2, `lsof` + `footprint -s`, 2026-07-28)
had **156 open connections** across 69 blocking threads (57 × `importance-root.db`, 53 × `index-root.db`, 30 + 10 on the
NAS volume, 6 × `media-root.db`), holding ~1.15 GB of a 2.5 GB footprint. Any per-connection `cache_size` is a ceiling
that multiplies by a number nothing controls.

**The fix.** `sqlite_util::install_shared_page_cache` hands SQLite one 64 MiB slab via
`sqlite3_config(SQLITE_CONFIG_PAGECACHE, pBuf, sz, N)`. Total page-cache memory is then that one number no matter how
many connections exist, and it's allocated on demand out of the slab: a connection running a real scan can take a large
share while a hundred idle ones hold nothing. The bundled SQLite defines `SQLITE_ENABLE_MEMORY_MANAGEMENT`, so `pcache1`
runs a UNIFIED page group (one LRU across every connection) rather than per-cache groups, which is what makes the
sharing dynamic instead of first-come-first-served. Slot size is `4096 + sqlite3_config(SQLITE_CONFIG_PCACHE_HDRSZ)`
rounded to 8, queried rather than guessed: a slot one byte too small is never used and every allocation silently falls
through to the heap (verified against the bundled amalgamation's `pcache1Alloc`, libsqlite3-sys 0.38.1, 2026-07-29).

**Why 64 MiB.** It holds a whole `wal_autocheckpoint` window (16 MiB) for two concurrently scanning volumes plus every
hot DB's upper b-tree levels and a real leaf working set, and it's ~5x below the 310 MB the per-connection ceiling
allowed at the profiled connection count. The tradeoff is honest: the slab is allocated and touched up front, so it's a
FIXED resident cost even for a session that opens one small DB, where the old model would have held less. We take that
trade because the failure the profile found was steady-state growth, not a peak, and a predictable 64 MiB beats an
unpredictable 310 MB.

**Ordering is the whole game.** `sqlite3_config` only works before SQLite initializes itself, and the first connection
opened ANYWHERE in the process initializes it. So every connection opens through
`sqlite_util::{open, open_read_only, open_in_memory}`, which force the slab first; a direct
`rusqlite::Connection::open*` that won the race would permanently and silently restore the old profile. The
`desktop-rust-sqlite-open-direct` check forbids one outside `sqlite_util.rs`, and `ensure_shared_page_cache()` reports
`TooLate` (with a `warn!`) if it ever happens anyway.

**Alternative weighed:** `sqlite3_soft_heap_limit64` also bounds the process dynamically and costs nothing at rest (the
bundled `SQLITE_ENABLE_MEMORY_MANAGEMENT` build can reclaim page cache under it). We chose the slab because it's a hard
bound with no reclaim heuristics, it degrades gracefully (SQLite falls back to `sqlite3Malloc` when the slab is
exhausted, and flips a global under-pressure flag that makes every cache recycle rather than grow), and it leaves the
heap accounting alone.

### The per-connection budgets on top

`apply_pragmas` takes a `readonly` flag and delegates to `crate::sqlite_util::apply_page_cache`, which every store's
`apply_pragmas` shares: `WRITE_PAGE_CACHE_KIB` (16 MiB) for a write connection, `READ_PAGE_CACHE_KIB` (8 MiB) for a
read-only one. With the slab installed these are UPPER BOUNDS per connection, not reservations, so they no longer
multiply into a process-wide number. One helper rather than five copies of a literal, because the failure mode is
silent: a store that keeps its own number drifts and nothing complains.

**Why the write budget is 16 MiB.** It's coupled to `wal_autocheckpoint = 4000` (~16 MiB of 4 KiB pages, set in the same
function): the cache is sized to hold what a whole autocheckpoint window dirties, so a big write batch commits without
evicting pages it's about to touch again. Change one and reconsider the other. There is at most ONE write connection per
DB (the single writer thread).

**Why reads get 8 MiB.** It comfortably holds the upper interior levels of the hot b-trees plus a directory's worth of
leaves, which is what the enrichment path needs: point lookups on `(parent_id, name_folded)` and one range scan. A
whole-index working set never fits (a 6.9M-row index runs ~170 k leaf pages) and the OS file cache backs those anyway.
Reads never commit or checkpoint, so nothing here touches the WAL.

**Test coverage**: `sqlite_util::tests` pins the slab (installed before any connection opens, budget numbers, and
`SQLITE_STATUS_PAGECACHE_USED > 0` proving pages really come from it), and
`read_connections_get_a_smaller_page_cache_than_write_connections`, one per store (`tests/open_and_recover.rs` here and
in `importance/store`, `media_index/store`, `agent/store`, `operation_log/store`), asserts the exact KiB each role opens
with.

## Prepared statements on the hot write path

`insert_entry_v2_with_id` is the LIVE reconcile write path, called once per file the watcher sees. The scan path takes
`insert_entries_v2_batch` instead, which already batches and savepoints, so the single-row call is where per-call cost
lands.

**Why `execute` with a literal was wrong.** `rusqlite::Connection::execute` prepares from SQL TEXT on every call, so
SQLite re-ran its parser per inserted row. A prod stack profile (v0.37.0, 2026-08-03) put **1,828 of ~3,398 running
writer samples** in `sqlite3RunParser` → `yy_reduce` → `sqlite3Insert` → `sqlite3GenerateConstraintChecks` →
`sqlite3MPrintf`, against **182** in `sqlite3_step`. The `sqlite3MPrintf` calls materialize constraint-violation message
strings ("UNIQUE constraint failed: entries.parent_id, entries.name_folded" and the NOT NULL equivalents) as P4 operands
in the VDBE program. That is not waste, it is the normal cost of PREPARING an insert against a constrained table; the
defect was paying it once per row instead of once per statement.

**Why the cache capacity is load-bearing, not a tuning knob.** `rusqlite`'s statement cache is an LRU keyed by SQL text
with `STATEMENT_CACHE_DEFAULT_CAPACITY = 16`, and this store alone holds **38** `prepare_cached` sites (`entries.rs` 25,
`dir_stats.rs` 6, `meta.rs` 5, `paths.rs` 1, `connection.rs` 1). A writer working across them would evict and re-compile
statements it was about to reuse, reintroducing exactly the cost `prepare_cached` removes, **with no error and no
failing test to show for it**. `sqlite_util::apply_statement_cache` sets 64 on write connections beside
`apply_page_cache`, so the two role-splits are set in one place. ⚠️ Raise it when the `prepare_cached` count approaches
it; a cache smaller than the working set is worse than none, since it pays the lookup and still re-compiles.

Read connections keep the small default deliberately: they are thread-local and their count tracks tokio's blocking pool
rather than anything semantic (132 were open in a profiled session), so a large per-connection statement cache there
would multiply by a number nothing controls. Same reasoning as `READ_PAGE_CACHE_KIB`.

**Measured**, `tests/insert_throughput_probe.rs` (debug build, 2026-08-03): 34.1 → 30.2 µs per row, **12%**. The probe's
second variant is the more useful number and motivated the writer's implicit batching: the same rows inside ONE
transaction run at **7.2 µs**, so the per-row COMMIT plus WAL frame write is ~76% of the cost. See
`../writer/DETAILS.md` § "Implicit write batching".
