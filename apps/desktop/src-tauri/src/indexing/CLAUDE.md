# Drive indexing module

Background-indexes local volumes into per-volume SQLite databases, tracking every file and directory with recursive size aggregates. The key UX win: showing directory sizes in file listings.

Full design: `docs/specs/drive-indexing/plan.md`

## Architecture

### Module structure

- **mod.rs** -- Public API: `init()`, `start_indexing()`, `stop_indexing()`, `clear_index()`, `enrich_entries_with_index()`. `IndexManager` coordinates all subsystems, owns a `PathResolver` (LRU-cached path→ID mapping) for IPC commands. `ReadPool` provides lock-free thread-local read connections for enrichment and verification. Enrichment uses an integer-keyed fast path: resolve parent dir once → batch-fetch child dir stats by ID → match by name. Falls back to individual path resolution for edge cases.
- **store.rs** -- SQLite schema v2 (integer-keyed entries, dir_stats by entry_id, meta), platform_case collation, read queries, DB open/migrate. Schema version check: mismatch triggers drop+rebuild. Both path-keyed (backward compat) and integer-keyed APIs.
- **path_resolver.rs** -- `PathResolver`: resolves filesystem paths to integer entry IDs via component-by-component walk with full-path LRU cache (50K entries). Case-aware `CacheKey` on macOS (NFD + case fold). Prefix-based invalidation for deletes/renames.
- **memory_watchdog.rs** -- Background task monitoring resident memory via `mach_task_info` (macOS). Warns at 8 GB, stops indexing at 16 GB, emits `index-memory-warning` event to frontend. No-op stub on non-macOS. Started from `start_indexing()`.
- **writer.rs** -- Single writer thread, owns the write connection, processes `WriteMessage` channel (bounded `sync_channel`, 20K capacity, backpressure via blocking). Priority: `UpdateDirStats` before `InsertEntries`. `Flush` variant + async `flush()` method let callers wait for all prior writes to commit. Has both integer-keyed variants (`InsertEntriesV2`, `UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`, `PropagateDeltaById`) and path-keyed backward-compat variants. The integer-keyed delete/subtree-delete handlers auto-propagate negative deltas via the `parent_id` chain (same pattern as the path-keyed variants). `propagate_delta_by_id` walks the parent chain using `get_parent_id` lookups. Maintains `AccumulatorMaps` during `InsertEntriesV2` processing (two HashMaps: direct children stats and child dir relationships + an `entries_inserted` counter), cleared on `TruncateData`. On `ComputeAllAggregates`, passes accumulated maps to `aggregator::compute_all_aggregates_with_maps()` to skip expensive full-table-scan SQL queries. Accepts an optional `AppHandle` at spawn time to emit `index-aggregation-progress` events during aggregation (phase, current, total). Also emits `saving_entries` phase progress during `InsertEntriesV2` processing when the expected total is set via `set_expected_total_entries()` (an `Arc<AtomicU64>` shared between the writer thread and the `IndexWriter` handle).
- **scanner.rs** -- jwalk-based parallel directory walker. `scan_volume()` for full scan, `scan_subtree()` for micro-scans. Uses `ScanContext` (from store.rs) to assign integer IDs and parent IDs during the walk: maintains a `HashMap<PathBuf, i64>` mapping directory paths to assigned IDs. The scan root is mapped to `ROOT_ID` (1). Sends `InsertEntriesV2(Vec<EntryRow>)` batches to the writer. Platform-specific exclusion filters (macOS system paths, Linux virtual filesystems). Physical sizes (`st_blocks * 512`).
- **micro_scan.rs** -- `MicroScanManager`: bounded task pool (default 3 concurrent), priority queue (`UserSelected` > `CurrentDir`), deduplication, cancellation. Skips after full scan completes.
- **aggregator.rs** -- Dir stats computation. Bottom-up after full scan (O(N) single pass), per-subtree after micro-scan, incremental delta propagation up ancestor chain for watcher events. Two entry points for full aggregation: `compute_all_aggregates_reported` (loads maps from SQL) and `compute_all_aggregates_with_maps` (accepts pre-built maps from the writer). Both accept an `on_progress: &mut dyn FnMut(AggregationProgress)` callback and delegate to `compute_and_write()` for the shared topological sort + bottom-up computation + batch write. Progress is reported at phase transitions and every ~1% during compute/write loops. `AggregationPhase` enum: `SavingEntries` (flushing writer channel), `LoadingDirectories`, `Sorting`, `Computing`, `Writing`.
- **watcher.rs** -- Drive-level filesystem watcher. macOS: FSEvents via `cmdr-fsevent-stream` with event IDs and `sinceWhen` replay. Linux: `notify` crate (inotify backend) with recursive watching and synthetic event counter. Other platforms: stub. `supports_event_replay()` lets callers branch on whether journal replay is available.
- **reconciler.rs** -- Buffers FSEvents during scan (capped at 500K events; overflow sets `buffer_overflow` flag forcing full rescan), replays after scan completes using event IDs to skip stale events. Processes live events for file creates/removes/modifies using integer-keyed write messages (`UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`, `PropagateDeltaById`). Resolves filesystem paths to entry IDs via `store::resolve_path()` using a read connection passed by callers. Key functions (`process_fs_event`, `emit_dir_updated`) are `pub(super)` so `mod.rs` can call them directly during cold-start replay.
- **firmlinks.rs** -- Parses `/usr/share/firmlinks`, builds prefix map, normalizes paths. Converts `/System/Volumes/Data/Users/foo` to `/Users/foo`.
- **verifier.rs** -- Placeholder for per-navigation background readdir diff (future milestone).

IPC commands in `commands/indexing.rs` -- thin wrappers over `IndexManager` methods.

Frontend in `src/lib/indexing/` -- reactive state, event listeners, priority triggers, scan status overlay.

### Data flow

```
App startup
  |-- init(): register IndexManagerState in Tauri
  |-- start_indexing(): create IndexManager, open SQLite, spawn writer thread
  |-- resume_or_scan():
  |   |-- macOS: Has existing index + last_event_id?
  |   |   |-- Pre-check: event gap > 1M? -> emit index-rescan-notification (StaleIndex), full scan
  |   |   |-- Otherwise -> sinceWhen replay (FSEvents journal)
  |   |-- Linux: Always full rescan (no event journal; existing DB used for instant enrichment)
  |   |-- Incomplete previous scan (has data but no scan_completed_at)? -> notify + fresh scan
  |   |-- Otherwise -> fresh full scan
  |
Full scan (start_scan):
  |-- Truncate entries + dir_stats (TruncateData + flush_blocking)
  |-- Start DriveWatcher (sinceWhen=0, buffers events)
  |-- ScanContext initialized: root -> ROOT_ID, next_id from DB
  |-- jwalk parallel walk -> ScanContext assigns IDs -> batched InsertEntriesV2 -> writer -> SQLite
  |-- On complete: replay buffered events (reconciler), compute all aggregates, switch to live mode
  |
Live mode:
  |-- macOS: FSEvents -> reconciler (resolve_path -> entry IDs) -> UpsertEntryV2/DeleteEntryById/DeleteSubtreeById -> writer -> SQLite
  |-- Linux: inotify (via notify crate) -> same pipeline
  |-- Reconciler and event loops hold a read connection for integer-keyed path resolution
  |-- Events deduplicated by normalized path in HashMap, flushed every 1s via index-dir-updated event
  |
Enrichment (every get_file_range call):
  |-- enrich_entries_with_index() -> resolve parent dir → id (one tree walk)
  |-- list_child_dir_ids_and_names(parent_id) → (id, name) pairs
  |-- get_dir_stats_batch_by_ids(child_ids) → batch stats
  |-- Match by normalized name → populate FileEntry.recursive_size/file_count/dir_count
  |-- Fallback: individual path resolution if fast path fails (mixed-parent edge case)
```

### Single-writer architecture

All writes go through a dedicated `std::thread` via a bounded `sync_channel` (20K capacity). When the channel is full, senders block (backpressure). The writer thread owns the write connection and processes messages in order, prioritizing `UpdateDirStats` over `InsertEntries` for responsive micro-scan results.

Reads happen on separate WAL connections (any thread). A `ReadPool` provides thread-local read connections for enrichment and verification without contending on the `INDEXING` state-machine mutex.

### SQLite schema (v4: integer-keyed, incremental vacuum)

One DB per volume. **Dev and prod use separate directories** (see AGENTS.md § Debugging):
- **Prod**: `~/Library/Application Support/com.veszelovszki.cmdr/index-{volume_id}.db`
- **Dev**: `~/Library/Application Support/com.veszelovszki.cmdr-dev/index-{volume_id}.db`

Three tables:
- `entries` (id INTEGER PK, parent_id, name COLLATE platform_case, is_directory, is_symlink, size, modified_at) with unique index `idx_parent_name(parent_id, name)`. Root sentinel: id=1, parent_id=0, name="".
- `dir_stats` (entry_id INTEGER PK, recursive_size, recursive_file_count, recursive_dir_count)
- `meta` (key TEXT PK, value TEXT) WITHOUT ROWID

WAL mode, 16 MB page cache, `auto_vacuum = INCREMENTAL` (free pages reclaimed via `PRAGMA incremental_vacuum` after truncation). Custom `platform_case` collation registered on every connection: case-insensitive + NFD normalization on macOS, binary on Linux. **Opening the DB with the sqlite3 CLI will fail** on queries touching the name column (the collation isn't registered).

History of changes:
- **Schema v3**: Bumped from v2 to force DB rebuild after fixing orphan entry bug. Scanner, writer, aggregator, reconciler, enrichment, and IPC commands all fully migrated to integer keys. `IndexManager` owns a `PathResolver` for LRU-cached path→ID resolution in IPC commands (`get_dir_stats`, `get_dir_stats_batch`). Enrichment uses integer-keyed fast path: resolve parent once → batch child dir stats by ID. Reconciler sends integer-keyed messages exclusively. Old path-keyed `WriteMessage` variants and backward-compat shims (`ScannedEntry`, `DirStats`) still exist for post-replay verification — cleanup in milestone 6.
- **Schema v4**: Bumped from v3 to enable `auto_vacuum = INCREMENTAL` (requires DB rebuild since the pragma must be set before table creation).

## How to test

Run Rust tests:
```sh
cd apps/desktop/src-tauri && cargo nextest run indexing
```

Key test files are alongside each module (test functions within `#[cfg(test)]` blocks). Tests use temp dirs and real SQLite to verify:
- Store: schema creation, reads, writes, batch operations, schema mismatch handling, `list_child_dir_ids_and_names`
- Aggregator: bottom-up computation, subtree-only computation, delta propagation
- Scanner: full scan with temp dir trees, exclusion filtering, cancellation
- Firmlinks: path normalization, edge cases
- Micro-scan manager: priority ordering, deduplication, cancellation
- Writer: message processing, priority handling
- Path resolver: cache hit/miss, prefix invalidation, case-insensitive lookups (macOS)
- mod.rs: end-to-end integration (scan → aggregate → enrich → watcher update → re-enrich), enrichment fast path, fallback, root-level enrichment, PathResolver for dir stats

## Key decisions

**Single-writer thread, not connection pooling**: SQLite write concurrency is limited by its single-writer design. Instead of fighting it with `BUSY_TIMEOUT` and retries, one dedicated thread owns the write connection. Eliminates contention entirely.

**Index enrichment at read time, not cache time**: `recursive_size` fields are populated on every `get_file_range` call via a batch SQLite read from `dir_stats`. This avoids stale cache entries when micro-scans complete. The cost is microseconds per page on a WAL connection.

**Enrichment uses integer-keyed batch lookup**: Instead of N individual `resolve_path()` calls (one per directory in the listing), `enrich_entries_with_index` resolves the parent directory once, queries `list_child_dir_ids_and_names(parent_id)` for all child dir IDs, then `get_dir_stats_batch_by_ids()`. Two indexed queries total instead of N. Falls back to individual path resolution for edge cases (for example, mixed-parent entries).

**IPC boundary stays path-based**: Frontend sends filesystem paths, backend resolves path→ID internally via `PathResolver`. No frontend changes needed. `IndexManager.get_dir_stats()` and `get_dir_stats_batch()` use the `PathResolver`'s LRU cache for efficient resolution.

**Physical sizes (`st_blocks * 512`)**: More meaningful for disk usage than logical size. May overcount ~10-20% for APFS clones (shared blocks). Volume usage bar uses `statfs()` for true totals.

**Subtree rescans delete descendants first**: `scan_subtree` sends `DeleteDescendantsById(root_id)` to the writer before inserting fresh entries. This prevents orphaned entries that previously caused DB bloat (4x) and missing dir_stats. The root entry is preserved (its existing ID is reused by `ScanContext`). The delete and subsequent inserts are serialized through the single writer channel, so no race conditions. `ComputeSubtreeAggregates` runs after the scan to recompute stats.

**In-memory accumulation eliminates aggregation SQL queries**: During a full scan, the writer thread accumulates two HashMaps in `AccumulatorMaps` as `InsertEntriesV2` batches arrive: `direct_stats` (parent_id -> file size/count/dir count) and `child_dirs` (parent_id -> child dir IDs). When `ComputeAllAggregates` fires, these maps are passed to `compute_all_aggregates_with_maps()`, skipping the two expensive full-table-scan SQL queries (`bulk_get_children_stats_by_id` and `bulk_get_child_dir_ids`) that previously dominated aggregation time (~70%). Maps are cleared on `TruncateData` and after aggregation completes. Falls back to SQL queries if maps are empty.

**Subtree aggregation uses scoped queries**: `scoped_get_children_stats_by_id` and `scoped_get_child_dir_ids` in `aggregator.rs` use recursive CTEs scoped to the target subtree, not full-table scans. This keeps subtree aggregation O(subtree_size) regardless of total DB size.

**Bounded buffers prevent OOM**: All buffers have capacity limits. FSEvents channel: 32K batches (bounded `try_send` in cmdr-fsevent-stream; overflow sets atomic flag, triggers rescan). Reconciler buffer: 500K events (overflow triggers full rescan). Writer channel: 20K messages (bounded `sync_channel`, backpressure). Replay `affected_paths`: 50K entries (overflow emits full refresh). Replay `pending_rescans`: 1K entries (overflow triggers full rescan). Replay event count: 1M events max (overflow falls back to full scan). Memory watchdog: warns at 8 GB, stops indexing at 16 GB. The index is a disposable cache, so dropping events and rescanning is always safe.

**Disposable cache pattern**: The index DB is a cache, not a source of truth. Any corruption or error triggers delete+rebuild. No user-facing errors for DB issues.

**cmdr-fsevent-stream fork (macOS only)**: Vendored in `crates/fsevent-stream/` (forked from `fsevent-stream` v0.3.0). Provides direct access to FSEvents event IDs, `sinceWhen` replay, and `MustScanSubDirs` flags. Only used on macOS. On Linux, the `notify` crate (inotify backend) provides recursive directory watching with `RecursiveMode::Recursive`.

**Linux inotify watch limits**: Default `fs.inotify.max_user_watches` is ~8192. The `notify` crate's recursive mode adds one inotify watch per directory. Power users with large directory trees may hit this limit; the workaround is `sysctl fs.inotify.max_user_watches=524288`. The watcher gracefully handles watch errors without crashing.

**APFS firmlinks**: Scan from `/` only, skip `/System/Volumes/Data`. Normalize all paths via firmlink prefix map so DB lookups work regardless of how the user navigated to a path.

**Rescan notification system (`RescanReason` enum)**: Every code path that falls back to a full rescan emits an `index-rescan-notification` event with a `RescanReason` variant and human-readable details. The frontend maps each reason to a user-friendly toast message. Eight reasons: `StaleIndex` (pre-check gap), `JournalGap` (in-loop gap), `ReplayOverflow` (>1M events), `TooManySubdirRescans` (>1K MustScanSubDirs), `WatcherStartFailed`, `ReconcilerBufferOverflow` (>500K buffered events during scan), `IncompletePreviousScan` (has data but no `scan_completed_at`), `WatcherChannelOverflow` (FSEvents channel full, events dropped). The pre-check in `resume_or_scan()` catches stale indexes before starting the FSEvents stream, preventing the cmdr-fsevent-stream channel (32K capacity, `try_send`) from being overwhelmed.

## Gotchas

**INSERT OR REPLACE on a populated DB is catastrophically slow**: The `platform_case` collation (NFD + case fold on macOS) runs for every B-tree comparison during unique index lookups. On an empty DB a full scan takes ~2.5 min; on a populated DB with 5.5M entries the same scan takes ~30 min because each `INSERT OR REPLACE` triggers ~20 collation calls to traverse the B-tree. `start_scan()` truncates `entries` and `dir_stats` via `TruncateData` + `flush_blocking()` before every scan to avoid this. Additionally, without truncation, old rows accumulate as orphaned subtrees (3-4x DB bloat per scan cycle) because `INSERT OR REPLACE` only deduplicates at the root level.

**Cold-start replay enters live mode immediately after flush**: The `run_replay_event_loop` doesn't emit `index-dir-updated` during Phase 1 (replay). It collects affected paths, flushes the writer (ensuring all writes are committed), emits a single batched notification, re-enables micro-scans, and enters live mode right away (~100ms from startup). Post-replay verification (`verify_affected_dirs`) runs in a background task (`run_background_verification`) concurrently with live events. This is safe because the writer serializes all writes. Any corrections found by verification are emitted as a separate `index-dir-updated` batch.

**Live events are deduplicated and batched with a 1s window**: Both `run_live_event_loop` and the Phase 3 live loop in `run_replay_event_loop` collect incoming events into a `HashMap<String, FsChangeEvent>` keyed by normalized path. On each 1s flush tick, only the deduplicated set is processed through `process_live_event`. `merge_fs_events` keeps the most significant flags when events collide: `must_scan_sub_dirs` always wins, then `removed`, then `created`, then `modified`. `UpdateLastEventId` is sent once per batch (in `process_live_batch`) instead of per-event, reducing writer channel pressure during event storms.

**Writer-side delete-with-propagation**: Both path-keyed (`DeleteEntry`/`DeleteSubtree`) and integer-keyed (`DeleteEntryById`/`DeleteSubtreeById`) handlers in the writer automatically read old data before deleting and propagate accurate negative deltas. The integer-keyed variants use `propagate_delta_by_id` which walks the `parent_id` chain via `get_parent_id` lookups. This means every deletion -- replay, live, verification -- gets correct dir_stats updates without callers needing to send separate `PropagateDelta` messages.

**Post-replay verification is bidirectional**: `verify_affected_dirs` checks both directions: (1) stale entries in DB but not on disk (sends `DeleteEntry`/`DeleteSubtree`), and (2) missing entries on disk but not in DB (sends `UpsertEntry` + `PropagateDelta` for files, collects directory paths for `scan_subtree`). New directories are scanned and their subtree totals propagated up the ancestor chain. Uses a two-phase pattern: Phase 1 uses `ReadPool` for lock-free bulk SQLite reads into a `HashMap`, Phase 2 does all disk I/O without any lock. `run_background_verification`'s dir-stat reads also use `ReadPool`. No `INDEXING` lock is held during verification.

**Schema version mismatch drops the DB**: If `schema_version` in meta doesn't match what the code expects, the entire DB is deleted and rebuilt. No migration path (it's a cache, not user data).

**`verifier.rs` is a placeholder**: Per-navigation readdir diff is a future milestone. Currently just a TODO comment.

**Scan cancellation leaves partial data**: By design. `scan_completed_at` not set in meta, so next startup detects incomplete scan and runs fresh. No cleanup needed.

**`ReadPool` replaces `INDEXING` lock for all read-only DB access**: Enrichment (`enrich_entries_with_index`), verification Phase 1 (`verify_affected_dirs`), and background verification dir-stat reads all use `get_read_pool()` + `pool.with_conn()` — thread-local SQLite connections with no lock contention. The `INDEXING` mutex now guards only lifecycle transitions and IPC commands that need `PathResolver`. `with_conn` uses `thread_local!` storage, so callers must not have `.await` points between obtaining the pool and completing the closure (async task migration would break thread affinity).

**Progress events use `tauri::async_runtime::spawn`**: Not `tokio::spawn`, because indexing can start from Tauri's synchronous `setup()` hook where no Tokio runtime context exists.

**`platform_case` collation must be registered on every connection**: The custom collation is not persisted in the DB file. Both `IndexStore::open()` and `open_write_connection()` register it. Forgetting to register before querying causes `no such collation sequence: platform_case` errors. On macOS it uses NFD normalization + case folding (matching APFS). On Linux it's binary (zero overhead). The `PathResolver`'s `CacheKey` uses the same normalization via `store::normalize_for_comparison()`.

**Backward-compat shims resolve paths via component walk**: Old path-keyed functions (`get_entry`, `delete_entry`, `upsert_entry`, etc.) internally call `resolve_path()` which walks the tree component-by-component. This means parent directories MUST exist before inserting children. The aggregator's path-keyed `propagate_delta` and `compute_subtree_aggregates` also resolve paths internally. The reconciler no longer uses these shims -- it sends integer-keyed messages directly (milestone 4). Enrichment no longer uses the path-keyed `get_dir_stats_batch` -- it uses integer-keyed batch lookups via `list_child_dir_ids_and_names` + `get_dir_stats_batch_by_ids` (milestone 5). Remaining users of path-keyed shims: `verify_affected_dirs` (post-replay verification). Cleanup in milestone 6.

**Reconciler holds a read connection**: `process_fs_event`, `replay`, and `process_live_event` all require a `&Connection` parameter for path-to-ID resolution. Callers (event loops in mod.rs) open a read connection via `IndexStore::open_write_connection(writer.db_path())` at loop start and pass it through. This is a WAL-mode connection so it doesn't block the writer. The `IndexManager` also owns a `PathResolver` with LRU cache, used by IPC commands (`get_dir_stats`, `get_dir_stats_batch`) for cached resolution. The event loops don't use the `PathResolver` yet because they run in separate async tasks -- could be migrated in a future optimization pass.

**ScanContext maps scan root to ROOT_ID**: Both `scan_volume` and `scan_subtree` create a `ScanContext` that maps the scan root directory to `ROOT_ID` (1). This means all top-level entries under any scan root get `parent_id = ROOT_ID` in the DB. For subtree scans, the root is resolved to its existing entry ID (not ROOT_ID), and `DeleteDescendantsById` is sent before the scan starts. The `ScanContext` opens a temporary read connection to the DB to fetch `next_id` via `get_next_id()`.

**Never use `INSERT OR REPLACE` on entries without deleting descendants first**: `INSERT OR REPLACE` on the `idx_parent_name` unique index silently deletes the old row and inserts a new one with a new ID. This orphans all children (their `parent_id` points to the deleted old ID) and orphans the old `dir_stats` row. The scanner's `insert_entries_v2_batch` still uses `INSERT OR REPLACE` as a safety net, but it's always preceded by `DeleteDescendantsById` for subtree scans, so no conflicts should occur in practice.

**IndexWriter exposes `db_path()`**: The scanner needs the DB path to open a temporary connection for `ScanContext::new()`. This path is stored on the `IndexWriter` handle and accessible via `db_path()`. The temporary connection is short-lived (only used to read `MAX(id)`).
