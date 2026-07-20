# Swap-scan feasibility: build into new tables, then swap

Research note, 2026-07-20. Read-only study of whether the local reconcile rescan can be replaced with "bulk-build into
`entries_new` / `dir_stats_new`, then swap in one transaction". Nothing was compiled or run for this note; every claim
is either a code citation, a read-only SQLite probe of a real index DB, or explicitly marked as not determined.

Verified environment: `rusqlite` 0.40.1 / `libsqlite3-sys` 0.38.1 (`Cargo.lock`), bundled SQLite **3.53.2**
(`#define SQLITE_VERSION "3.53.2"` in the vendored `libsqlite3-sys-0.38.1/sqlite3/sqlite3.h`). Read via
`sqlite3_prepare_v3` (`rusqlite-0.40.1/src/inner_connection.rs:225`).

Real index DB probed read-only (`~/Library/Application Support/com.veszelovszki.cmdr-dev/index-root.db`, `immutable=1`,
2026-07-20): `page_size` 4096, `page_count` 273,854 (= 1.122 GB), `freelist_count` 0, `auto_vacuum` 2 (INCREMENTAL),
`entries` 7,385,259 rows, `dir_stats` 583,711 rows. `sqlite_master` holds exactly: `entries`, `idx_parent_name_folded`,
`idx_inode`, `dir_stats`, `meta`. No views, no triggers. The prod DB is 1,136,955,392 bytes with a 49 MB `-wal`.

## Verdict up front

The swap is feasible, but the plan as stated has one silent-corruption-adjacent trap (index names), one correctness bug
that voids its headline promise (the `scan_completed_at` clear), and one performance own-goal (search reload thrash).
A **separate DB file** rather than shadow tables in the same file avoids three of the six problem areas outright; see
"The file-swap variant" at the end.

## 1. Does the search index survive a table swap underneath it?

**Yes, and it reloads by itself. But it will reload constantly during the scan for no reason.**

- The arena loader is `load_search_index` (`apps/desktop/src-tauri/src/search/index.rs:69`). It runs inside
  `pool.with_conn` (`indexing/enrichment.rs:53`), does one `SELECT COUNT(*) FROM entries` (`search/index.rs:87`) and
  then one `SELECT id, parent_id, name, is_directory, logical_size, modified_at FROM entries` (`search/index.rs:74`),
  iterating all 7.4M rows in a single `stmt.query()` loop (`search/index.rs:96-125`). It uses `prepare`, not
  `prepare_cached` (`search/index.rs:76`), so there is no cached-statement staleness here.
- It does **not** hold a connection between searches in any way that matters: `ReadPool` hands out a thread-local
  connection (`indexing/enrichment.rs:29-71`) that outlives the call, but the statements are re-prepared per use.
- Staleness is keyed off the mutation tracker: `LoadedVolume.generation` (`search/volumes.rs:65`) is stamped from
  `WRITER_GENERATION` (`indexing/writer/mod.rs:90`) at load, `get_loaded` (`search/volumes.rs:212-218`) returns `None`
  on mismatch, and `ensure_volume` (`search/volumes.rs:291`) then does a full blocking reload. `WRITER_GENERATION` is
  bumped by `MutationTracker::bump` (`indexing/writer/mod.rs:139-144`), called from the writer dispatcher on every
  mutating `WriteMessage` (`indexing/writer/mod.rs:1119-1190`), including every `InsertEntriesV2` batch.
- So after the swap, the first search reloads and reads the new table. Good.

**The problem the plan doesn't assume**: `MutationTracker::bump` has no notion of *which* table was written. A shadow
build sends millions of rows through `InsertEntriesV2`, so `WRITER_GENERATION` climbs continuously for ~68 s while the
visible `entries` table hasn't changed at all. Every search issued during the scan therefore invalidates and re-reads
the entire 7.4M-row arena to get **byte-identical data**, and each of those reloads holds a multi-second read snapshot
(which also blocks WAL checkpointing, see § 5). Fix: gate the bump on "wrote to the visible table", and bump exactly
once at the swap.

**Not determined by reading**: what happens to a load that is *mid-iteration* when the swap commits on the writer
connection. My reading of WAL snapshot isolation is that the implicit read transaction opened by `stmt.query()` pins a
snapshot for the whole iteration, so the loader finishes reading the old table and never sees the DDL. I did not verify
this. Experiment that settles it: a Rust test that starts iterating `entries` on a read connection, has a second
connection run `DROP TABLE entries; ALTER TABLE entries_new RENAME TO entries;` mid-iteration, and asserts the
iteration completes with the pre-swap row count and no error.

## 2. Does the live event loop's read connection survive it?

**Almost certainly yes, and in the current scan shape the live loop isn't even running when the swap would land.**

Every long-lived reader I found:

- **The live event loop**: one read connection opened at loop start and held for the loop's life
  (`indexing/event_loop/live.rs:85`, via `open_read_conn_with_retry`, `indexing/event_loop.rs:190`).
- **The replay loop**: same shape (`indexing/event_loop/replay.rs:96`), plus `scan_completion.rs:258`.
- **The local reconcile walk**: one read connection for the whole walk, deliberately in autocommit
  (`indexing/local_reconcile.rs:352`, and the comment at `local_reconcile.rs:337-339`).
- **`ReadPool` thread-locals** (`indexing/enrichment.rs:29-71`): one connection per thread per pool, cached until
  `invalidate()`. `invalidate()` is called only from `stop_indexing` (`indexing/state.rs:287`),
  `remove_instance_and_handles` (`state.rs:942`), `clear_index`, and `fail_index` (`state.rs:1278`) — **never on a
  rescan**. So these connections do span a swap.
- **`IndexStore`'s own `read_conn`** (`indexing/store/connection.rs:104-107`), held for the store's life and used by
  `manager.rs:594` / `manager.rs:663` and `store/connection.rs:235-245`.
- **The per-navigation verifier** (`indexing/verifier.rs`), which reads through `ReadPool` and is gated off while a
  scan runs (`verifier.rs:63-66`).
- **Two whole-index walkers I don't think the plan accounted for**: `media_index`'s enrichment scheduler
  (`media_index/scheduler/enrich.rs:91-128`, `SELECT ... FROM entries WHERE is_directory = 0 ORDER BY parent_id`, run
  through `pool.with_conn` at `media_index/scheduler/mod.rs:193` and `:414` and `media_index/coverage.rs:46`), and
  `importance`'s full recompute (`importance/scheduler/recompute.rs:60,88` via `IndexStore::all_directories` /
  `for_each_file_child`). Both are minutes-long full scans of the same tables on `ReadPool` connections.

Why they survive: all of them read through `prepare_cached` (82 call sites), and rusqlite prepares with
`sqlite3_prepare_v3`, whose documented behavior is to transparently re-prepare a statement when the schema cookie
changes. A statement cached against `entries` that is stepped again after `entries` was dropped and replaced by a
same-named, same-shaped table re-prepares against the new table. **This is SQLite/rusqlite semantics that I read, not a
behavior I verified in this repo.**

Two caveats that matter:

- The replacement table must have the **identical** column set and order, or re-prepared statements silently bind
  different columns. Build `entries_new` from the same DDL string, not a hand-written copy.
- A reader inside an **explicit** transaction would get an unrecoverable `SQLITE_SCHEMA` on its next statement. No
  reader uses one today (grep found `BEGIN IMMEDIATE` only on the writer connection, `indexing/writer/mod.rs:1303`;
  `store/dir_stats.rs:84` and `store/entries.rs:346` deliberately use savepoints), but this becomes a landmine for any
  future reader that adds one, and nothing enforces it.

**The big de-risker**: `start_scan` starts the `DriveWatcher` and *buffers* FSEvents; the live loop only starts in the
completion handler after the scan finishes (`indexing/manager.rs:574-580` flow doc; `scan_completion.rs`). So if the
swap lands before the replay/live handoff, there is no live-loop reader in existence at swap time. Make that an
explicit, asserted invariant rather than an accident.

`SQLITE_BUSY`: the swap needs the write lock, which only the single writer thread holds, and WAL readers don't block
writers. I see no BUSY hazard from the DDL itself. What I could *not* determine by reading is how long the `DROP TABLE`
of a ~1.1 GB table takes with `auto_vacuum = INCREMENTAL` on (it must update pointer-map pages for every freed page)
and how much WAL it generates. Needs measurement.

## 3. What else references the tables by name?

Files with literal `entries` / `dir_stats` SQL (non-test): `indexing/store/{mod,entries,dir_stats,meta}.rs`,
`indexing/writer/{entries,delta,repair,deferred_repair}.rs`, `indexing/aggregator/readers.rs`, `search/index.rs`,
`media_index/scheduler/enrich.rs`, plus `importance/scheduler/recompute.rs` through `IndexStore` helpers. All table
names are **hardcoded SQL literals across ~82 `prepare_cached` sites**. Writing into shadow tables therefore means
threading a table name through the entire store + writer API, or duplicating the write path. That's the single largest
mechanical cost of the in-file variant.

No views, no triggers, no foreign keys (`PRAGMA foreign_keys` is never set anywhere; the schema has no `REFERENCES`).
Confirmed against a real DB's `sqlite_master`, above.

**The index-name trap (this is the sharp one).** `ALTER TABLE ... RENAME TO` renames the table; its indexes keep their
own names, and SQLite has no `ALTER INDEX ... RENAME`. Index names are per-database-schema, so `entries_new` cannot use
the name `idx_parent_name_folded` while the old `entries` still owns it. After the swap you're left with a table named
`entries` whose unique index is named `idx_parent_name_folded_new`. Then on the next launch, `create_tables`
(`indexing/store/mod.rs:554`) runs `CREATE UNIQUE INDEX IF NOT EXISTS idx_parent_name_folded ON entries (...)`
(`store/mod.rs:469`), finds no index by that name, and **builds a second full copy of a 7.4M-row unique index at
startup** — silently, on every launch until someone notices. Same for `idx_inode` (`store/mod.rs:470`). Options:

- (a) Create `entries_new` without indexes and build them after the swap: costs a full 7.4M-row index build at the end
  of every scan and, worse, drops the `UNIQUE (parent_id, name_folded)` safety net during the bulk build — the net that
  `store/CLAUDE.md` calls out as the guard against the observed 1.83 TB ghost-size double-insert.
- (b) Make `create_tables` schema-aware (inspect `sqlite_master`, rename by drop+recreate). Still a full index rebuild
  once.
- (c) Use a separate DB file, where the names live in a different schema namespace and never collide. See the end.

**`meta` survives naturally** (a swap touches only `entries` / `dir_stats`), but what it carries is not swap-safe:
`current_epoch` is bumped at scan start (`manager.rs`, Step 0a'), the calibration keys describe the last *completed*
scan, and `scan_completed_at` is deleted at scan start — see § 6.

**The id space breaks at the swap.** A shadow build allocates fresh ids from 2 (the writer owns the shared
`Arc<AtomicI64>`; `TruncateData` resets it, `indexing/writer/entries.rs:750-780`). At the swap, every entry id held
anywhere else becomes a pointer into a different row:

- The deferred-repair queue holds ids and is explicitly cleared on `TruncateData` for exactly this reason
  (`indexing/writer/deferred_repair.rs:97-100`). A swap must clear it too.
- Any in-flight verifier correction started just before `scanning` flipped can land after the swap with pre-swap ids
  (`UpsertEntryV2` / `DeleteEntryById`), now naming unrelated rows.
- The shared id counter would be used by both the shadow walker and any live write into the old table. The shadow build
  needs its **own** counter, installed as the live one only at the swap.

## 4. Is `ALTER TABLE ... RENAME` in a transaction safe here?

**Yes, with two caveats.**

- DDL is transactional in SQLite, so `DROP TABLE entries; ALTER TABLE entries_new RENAME TO entries;` in one
  transaction is atomic and crash-safe under WAL. Other connections never observe the intermediate state.
- `legacy_alter_table` is never set (the only pragmas applied are `busy_timeout`, `synchronous`, `cache_size`,
  `auto_vacuum`, `journal_mode`, `wal_autocheckpoint`, `journal_size_limit` — `indexing/store/mod.rs:498-551`), so
  SQLite 3.53.2's modern rename applies: it rewrites references to the renamed table in triggers and views. There are
  none, and no foreign keys, so it has nothing to rewrite. No version-specific hazard I can identify.
- `auto_vacuum = INCREMENTAL` doesn't interfere with the DDL; it only means the `DROP` produces a large freelist plus
  pointer-map churn.

**Caveat 1 (durability):** with `synchronous = NORMAL` + WAL, the swap's commit is not fsynced; it becomes durable at
the next checkpoint. A power loss right after the swap commits can roll back to the pre-swap state, leaving the old
tables *and* a populated `entries_new` behind. Not corruption, but the recovery path must be idempotent, not
"assume the swap either happened or never started".

**Caveat 2 (cost):** dropping a ~1.1 GB table inside the swap transaction is a large write. Duration and WAL volume are
**not determined by reading**; measure before committing to the design.

## 5. Disk cost

Measured, not estimated: the index is 273,854 pages × 4096 B = **1.122 GB** for 7,385,259 `entries` + 583,711
`dir_stats` rows, freelist 0.

- A shadow copy roughly **doubles the main file** to ~2.25 GB at peak. SQLite grows the file for the new tables and
  cannot shrink it until after the `DROP`.
- Add WAL. `journal_size_limit` is 64 MiB (`store/mod.rs:547`) but that only trims the file *after* a checkpoint, and
  checkpoints are blocked while any reader holds an older snapshot — exactly what a 7.4M-row search load, an importance
  recompute, or a `media_index` walk does. Peak disk budget should be ~2.5-3 GB, not 2.25.
- **`auto_vacuum = INCREMENTAL` does not reclaim anything on its own.** It only makes reclaim possible via
  `PRAGMA incremental_vacuum`. Two drivers exist: the uncapped inline call after truncate
  (`indexing/writer/entries.rs:764-767`) and the 30 s `IncrementalVacuum` writer tick, which is **capped**
  (`indexing/writer/maintenance.rs:143-165`: skip below 1,000 free pages, 2,000/tick up to 20,000, 20,000/tick above).
  A ~274,000-page freelist at 20,000 pages per 30 s tick is ~14 ticks ≈ **7 minutes** before the file is back to its
  pre-swap size. An uncapped inline vacuum right after the swap would reclaim in one pass but holds the write lock for
  the whole ~274,000-page reclaim. Wall-clock for either is **not determined by reading**.

So: the file stays large for minutes after the swap by default, and the user sees ~2.3 GB during the scan.

## 6. What breaks "an interrupted scan leaves the old index intact"?

Six things, in rough order of severity.

1. **`scan_completed_at` is cleared at scan start** (`indexing/manager.rs`, Step 0a, the `DeleteMeta("scan_completed_at")`
   send). If a swap scan is interrupted, `entries` is fully intact but the completion marker is gone, so the next scan
   evaluates `local_rescan_reconciles(entry_count, prior_scan_completed = false)`
   (`indexing/manager.rs:168-170`) as **false** and takes the truncate path, blanking the index. The plan's headline
   promise fails on the very next run. A swap scan must **not** clear that marker (nothing about the visible tables is
   partial) and should use its own `swap_scan_in_progress` meta key instead.
2. **Orphan `entries_new` / `dir_stats_new` survive forever.** Nothing enumerates `sqlite_master`; `create_tables` is
   `IF NOT EXISTS` (`store/mod.rs:554`) and the schema-version gate is a `meta` read (`store/connection.rs:82-102`). A
   leftover shadow table costs ~1.1 GB invisibly, and the next scan's `CREATE TABLE entries_new` finds a populated
   table and would append to garbage. Needs an explicit `DROP TABLE IF EXISTS entries_new, dir_stats_new` at open, plus
   after every swap and every scan abort.
3. **A lost swap under `synchronous = NORMAL`** (§ 4, caveat 1) leaves old tables plus a full shadow table; recovery
   must be idempotent.
4. **The fatal-storage-error and memory-watchdog paths** (`fail_index`, `indexing/state.rs:1269+`; the global 16 GB
   watchdog) stop indexing mid-scan with no table-cleanup hook today.
5. **Stale ids escaping the swap** (§ 3): deferred repairs and in-flight verifier corrections.
6. The corruption / schema-mismatch path deletes the whole DB file (`store/connection.rs:44-51`) and is unaffected.
   Bumping `SCHEMA_VERSION` for the new shape also wipes everything by design — that's fine, but it means a swap-scan
   rollout costs one full rebuild for every existing user.

## The file-swap variant (recommendation)

Building into a separate `index-root.new.db` rather than shadow tables in the same file removes four of the problems
above:

- No table-name threading through ~82 SQL sites: the DDL and every statement stay byte-identical.
- No index-name collision (§ 3): separate schema namespace.
- No giant `DROP` and no 274,000-page freelist (§ 5): deleting the old file reclaims all 1.12 GB instantly, no
  incremental-vacuum drain, no 7-minute tail.
- Cleanup on interruption is `remove_file`, and detecting a leftover is a `path.exists()` instead of a `sqlite_master`
  query.
- Existing readers keep the old inode alive until they close it (POSIX unlink semantics), which is exactly the
  "index stays readable throughout" property; `ReadPool::invalidate()` (`indexing/enrichment.rs:43`) already exists to
  flip them onto the new file, and `state.rs` already calls it on three lifecycle paths.

Costs: `meta` must be carried across explicitly (it's tiny), the writer holds two connections during the scan, the swap
is a file rename rather than an atomic DB transaction (so the "which file is current" decision needs its own durable
marker), and the search `SEARCH_INDICES` cache plus the `IndexStore`'s own long-lived `read_conn`
(`store/connection.rs:104-107`) must be pointed at the new file.

## Open questions that need an experiment, not more reading

1. Does a mid-iteration reader survive the swap unharmed? (§ 1; test described there.)
2. Do `prepare_cached` statements on a long-lived reader actually re-prepare across the swap in this codebase, or does
   any of them surface an error? (§ 2; a two-connection integration test.)
3. How long does `DROP TABLE` of ~1.1 GB take with `auto_vacuum = INCREMENTAL`, and how much WAL does it write? (§ 4.)
4. How long does the post-swap reclaim take, capped (~7 min projected) versus uncapped inline? (§ 5.)
5. Does the bulk parallel walker actually keep its 68 s throughput when the DB file already holds 1.12 GB of live data
   (page-cache pressure, more file growth, checkpoint interference)? The 68 s figure was measured against an empty
   file.
