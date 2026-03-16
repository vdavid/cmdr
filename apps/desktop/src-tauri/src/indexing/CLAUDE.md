# Drive indexing module

Background-indexes local volumes into per-volume SQLite databases, tracking every file and directory with recursive size aggregates. The key UX win: showing directory sizes in file listings.

Full design: `docs/specs/drive-indexing/plan.md`

## Architecture

### Module structure

- **mod.rs** -- Public API (`init()`, `start_indexing()`, `stop_indexing()`, `clear_index()`), `IndexPhase` state machine, `IndexManager` (coordinates all subsystems), `DebugStats` (shared atomic counters for the debug window + phase timeline via `set_phase()`/`close_phase_with_stats()`). `start_scan()` takes a `scan_trigger: &str` parameter describing why the scan was initiated.
- **enrichment.rs** -- `ReadPool` (lock-free thread-local read connections for enrichment and verification), `enrich_entries_with_index()` (called when entries are stored in the listing cache — streaming, watcher update, re-sort — NOT on `get_file_range`; index freshness is handled by `index-dir-updated` → `refreshIndexSizes` → `getDirStatsBatch`). Integer-keyed fast path: resolve parent dir once → batch-fetch child dir stats by ID → match by name. Falls back to individual path resolution for edge cases.
- **event_loop.rs** -- `run_live_event_loop` (real-time FSEvents/inotify processing after scan completes), `run_replay_event_loop` (cold-start journal replay with two-phase approach), `run_background_verification` (post-replay bidirectional readdir diff), `merge_fs_events` (deduplication with flag priority), `process_live_batch`. All bounded-buffer constants live here.
- **events.rs** -- Tauri event payload structs (`IndexScanStartedEvent`, `IndexScanProgressEvent`, `IndexScanCompleteEvent`, `IndexDirUpdatedEvent`, `IndexReplayProgressEvent`), `RescanReason` enum, `emit_rescan_notification()`, IPC response types (`IndexStatusResponse`, `IndexDebugStatusResponse`). Also: `ActivityPhase` enum (Replaying/Scanning/Aggregating/Reconciling/Live/Idle) and `PhaseRecord` for the phase timeline system tracked in `DebugStats`.
- **store.rs** -- SQLite schema v6 (integer-keyed entries with `name_folded` column on macOS, dir_stats by entry_id, meta), platform_case collation, read queries, DB open/migrate. `resolve_component` uses the composite index directly: on macOS queries by `(parent_id, name_folded)`, on Linux/Windows by `(parent_id, name)`. Schema version check: mismatch triggers drop+rebuild. Both path-keyed (backward compat) and integer-keyed APIs.
- **memory_watchdog.rs** -- Background task monitoring resident memory via `mach_task_info` (macOS). Warns at 8 GB, stops indexing at 16 GB, emits `index-memory-warning` event to frontend. No-op stub on non-macOS. Started from `start_indexing()`.
- **writer.rs** -- Single writer thread, owns the write connection, processes `WriteMessage` channel (bounded `sync_channel`, 20K capacity, backpressure via blocking). `WRITER_GENERATION: AtomicU64` (initialized to 1) bumped on every mutation (`InsertEntriesV2`, `UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`, `TruncateData`) for search index staleness detection. Priority: `UpdateDirStats` before `InsertEntries`. `Flush` variant + async `flush()` method let callers wait for all prior writes to commit. Has both integer-keyed variants (`InsertEntriesV2`, `UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`, `PropagateDeltaById`) and path-keyed backward-compat variants. The integer-keyed delete/subtree-delete handlers auto-propagate negative deltas via the `parent_id` chain (same pattern as the path-keyed variants). `propagate_delta_by_id` walks the parent chain using `get_parent_id` lookups. `UpsertEntryV2` initializes a zero-valued `dir_stats` row when inserting a NEW directory, so enrichment always has a row (subsequent `PropagateDeltaById` calls update it incrementally). Maintains `AccumulatorMaps` during `InsertEntriesV2` processing (two HashMaps: direct children stats and child dir relationships + an `entries_inserted` counter), cleared on `TruncateData`. On `ComputeAllAggregates`, passes accumulated maps to `aggregator::compute_all_aggregates_with_maps()` to skip expensive full-table-scan SQL queries. Accepts an optional `AppHandle` at spawn time to emit `index-aggregation-progress` events during aggregation (phase, current, total). Also emits `saving_entries` phase progress during `InsertEntriesV2` processing when the expected total is set via `set_expected_total_entries()` (an `Arc<AtomicU64>` shared between the writer thread and the `IndexWriter` handle). No index drop/recreate dance — the composite indexes (`idx_parent_name_folded` on macOS, `idx_parent_name` on Linux) use binary collation and stay present during scans.
- **scanner.rs** -- jwalk-based parallel directory walker. `scan_volume()` for full scan, `scan_subtree()` for targeted subtree rescans (used by post-replay background verification). Uses `ScanContext` (from store.rs) to assign integer IDs and parent IDs during the walk: maintains a `HashMap<PathBuf, i64>` mapping directory paths to assigned IDs. The scan root is mapped to `ROOT_ID` (1). Sends `InsertEntriesV2(Vec<EntryRow>)` batches to the writer. Platform-specific exclusion filters via `should_exclude` (`pub(super)`) — the single exclusion gate for all code paths (scanner, reconciler, event_loop verification, per-navigation verifier). `default_exclusions()` is `#[cfg(test)]` only. Physical sizes (`st_blocks * 512`).
- **aggregator.rs** -- Dir stats computation. Bottom-up after full scan (O(N) single pass), per-subtree after subtree rescans, incremental delta propagation up ancestor chain for watcher events. Two entry points for full aggregation: `compute_all_aggregates_reported` (loads maps from SQL) and `compute_all_aggregates_with_maps` (accepts pre-built maps from the writer). Both accept an `on_progress: &mut dyn FnMut(AggregationProgress)` callback and delegate to `compute_and_write()` for the shared topological sort + bottom-up computation + batch write. Progress is reported at phase transitions and every ~1% during compute/write loops. `AggregationPhase` enum: `SavingEntries` (flushing writer channel), `LoadingDirectories`, `Sorting`, `Computing`, `Writing`. (The former `RebuildingIndex` phase was removed when the composite `idx_parent_name` index with `platform_case` collation was replaced — now uses binary-collation composite indexes that don't need rebuilding.) `backfill_missing_dir_stats` is a catch-up pass that finds directories without `dir_stats` rows and computes their stats bottom-up; triggered after reconciler replay and cold-start replay via `BackfillMissingDirStats` writer message.
- **watcher.rs** -- Drive-level filesystem watcher. macOS: FSEvents via `cmdr-fsevent-stream` with event IDs and `sinceWhen` replay. Linux: `notify` crate (inotify backend) with recursive watching and synthetic event counter. Other platforms: stub. `supports_event_replay()` lets callers branch on whether journal replay is available.
- **reconciler.rs** -- Buffers FSEvents during scan (capped at 500K events; overflow sets `buffer_overflow` flag forcing full rescan), replays after scan completes using event IDs to skip stale events. Processes live events for file creates/removes/modifies using integer-keyed write messages (`UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`, `PropagateDeltaById`). Resolves filesystem paths to entry IDs via `store::resolve_path()` using a read connection passed by callers. Key functions (`process_fs_event`, `emit_dir_updated`) are `pub(super)` so `mod.rs` can call them directly during cold-start replay. `reconcile_subtree()` handles MustScanSubDirs by diffing filesystem vs DB directory-by-directory instead of delete-then-reinsert, making it safe to interrupt at any point.
- **firmlinks.rs** -- Parses `/usr/share/firmlinks`, builds prefix map, normalizes paths. Converts `/System/Volumes/Data/Users/foo` to `/Users/foo`.
- **verifier.rs** -- Per-navigation background readdir diff. On each directory navigation, `trigger_verification()` (called from `streaming.rs` and `operations.rs` after enrichment) is fully fire-and-forget: it spawns a task that acquires the `INDEXING` lock (never blocking the navigation thread), checks dedup/debounce via static `VerifierState` (in-flight set + recent timestamps), then spawns a second async task that: (1) reads DB children via `ReadPool`, (2) reads disk via `read_dir` (filtering through `scanner::should_exclude`), (3) diffs by normalized name, sending `UpsertEntryV2`/`DeleteEntryById`/`DeleteSubtreeById`/`PropagateDeltaById` corrections to the writer. New directories are flushed then scanned via `scan_subtree` with delta propagation. Debounce: 30s per path, max 2 concurrent verifications. Only runs after initial scan is complete (checks `scanning` flag). `invalidate()` clears state on shutdown/clear.
- **search.rs** -- In-memory search index for whole-drive file search. Lazily loads all entries from the index DB into a `Vec<SearchEntry>` for fast parallel scanning with rayon. Filenames are arena-allocated: all names are concatenated into a single `SearchIndex.names: String` buffer, and each `SearchEntry` stores `name_offset: u32` + `name_len: u16` instead of an owned `String`. During load, `row.get_ref(col).as_str()` borrows directly from SQLite's internal buffer (zero per-row heap allocations), then pushes into the arena. `name_folded` is NOT stored in the search index — instead, the search pattern is NFD-normalized at query time on macOS (APFS filenames are already NFD). `SearchIndex::name(&self, entry)` retrieves a `&str` slice from the arena. `search()` is a pure function: compiles glob/regex patterns, parallel-filters entries, sorts by recency. Global `SEARCH_INDEX` state with `Arc<SearchIndex>`, idle timer (5 min after dialog close), backstop timer (10 min with no activity), and load cancellation via `AtomicBool` checked every 100K rows. `WRITER_GENERATION` in writer.rs tracks mutations; stale indexes are detected on search. IPC commands in `commands/search.rs`: `prepare_search_index` (emits `search-index-ready` event when load completes), `search_files`, `release_search_index`, `translate_search_query` (AI natural language → structured query).

IPC commands in `commands/indexing.rs` -- thin wrappers over `IndexManager` methods.

Frontend in `src/lib/indexing/` -- reactive state, event listeners, scan status overlay.

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
  |
Navigation verification (after enrichment):
  |-- trigger_verification(path) → checks IndexPhase::Running, extracts writer/app/scanning
  |-- verifier::maybe_verify() → dedup/debounce check, spawns async task
  |-- verify_and_correct(): ReadPool → resolve_path + list_children_on → DB snapshot
  |-- read_dir → disk snapshot, diff by normalize_for_comparison
  |-- Corrections: UpsertEntryV2, DeleteEntryById/DeleteSubtreeById, PropagateDeltaById
  |-- New dirs: flush → scan_subtree → flush → propagate deltas → emit_dir_updated
```

### Single-writer architecture

All writes go through a dedicated `std::thread` via a bounded `sync_channel` (20K capacity). When the channel is full, senders block (backpressure). The writer thread owns the write connection and processes messages in order, prioritizing `UpdateDirStats` over `InsertEntries`.

Reads happen on separate WAL connections (any thread). A `ReadPool` provides thread-local read connections for enrichment and verification without contending on the `INDEXING` state-machine mutex.

### SQLite schema (v6: integer-keyed, platform-conditional composite index)

One DB per volume. **Dev and prod use separate directories** (see AGENTS.md § Debugging):
- **Prod**: `~/Library/Application Support/com.veszelovszki.cmdr/index-{volume_id}.db`
- **Dev**: `~/Library/Application Support/com.veszelovszki.cmdr-dev/index-{volume_id}.db`

Three tables:
- `entries` (id INTEGER PK, parent_id, name COLLATE platform_case, [name_folded on macOS], is_directory, is_symlink, size, modified_at). Root sentinel: id=1, parent_id=0, name="".
  - **macOS**: has a `name_folded TEXT NOT NULL` column storing `normalize_for_comparison(name)` (NFD + case fold). Index: `idx_parent_name_folded ON entries (parent_id, name_folded)`.
  - **Linux/Windows**: no `name_folded` column. Index: `idx_parent_name ON entries (parent_id, name)`.
  - The old `idx_parent(parent_id)` from v5 is removed; the composite indexes replace it.
- `dir_stats` (entry_id INTEGER PK, recursive_size, recursive_file_count, recursive_dir_count)
- `meta` (key TEXT PK, value TEXT) WITHOUT ROWID

WAL mode, 16 MB page cache, `auto_vacuum = INCREMENTAL` (free pages reclaimed via `PRAGMA incremental_vacuum` after truncation). Custom `platform_case` collation registered on every connection: case-insensitive + NFD normalization on macOS, binary on Linux. **Opening the DB with the sqlite3 CLI will fail** on queries touching the name column (the collation isn't registered).

History of changes:
- **Schema v3**: Bumped from v2 to force DB rebuild after fixing orphan entry bug. Scanner, writer, aggregator, reconciler, enrichment, and IPC commands all fully migrated to integer keys. Enrichment uses integer-keyed fast path: resolve parent once → batch child dir stats by ID. Reconciler sends integer-keyed messages exclusively. Old path-keyed `WriteMessage` variants and backward-compat shims (`ScannedEntry`, `DirStats`) still exist for post-replay verification — cleanup in milestone 6.
- **Schema v4**: Bumped from v3 to enable `auto_vacuum = INCREMENTAL` (requires DB rebuild since the pragma must be set before table creation).
- **Schema v5**: Replaced composite `UNIQUE INDEX idx_parent_name(parent_id, name)` with simple `INDEX idx_parent(parent_id)`. The composite index with `platform_case` collation was extremely slow to build (~25 min for 5.1M entries). A simple integer index needs no drop/recreate dance during scans.
- **Schema v6**: Added `name_folded` column (macOS only) storing pre-computed `normalize_for_comparison(name)`. Replaced `idx_parent` with platform-conditional composite indexes: `idx_parent_name_folded(parent_id, name_folded)` on macOS, `idx_parent_name(parent_id, name)` on Linux/Windows. `resolve_component` now queries the index directly instead of fetching all children and matching in Rust.

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
- Writer: message processing, priority handling
- mod.rs: end-to-end integration (scan → aggregate → enrich → watcher update → re-enrich), enrichment fast path, fallback, root-level enrichment
- stress_tests.rs: concurrency stress tests — concurrent scan + replay, concurrent batch inserts, concurrent scan + enrichment reads, live event storm + reads, lifecycle transitions under load

## Key decisions

**Single-writer thread, not connection pooling**: SQLite write concurrency is limited by its single-writer design. Instead of fighting it with `BUSY_TIMEOUT` and retries, one dedicated thread owns the write connection. Eliminates contention entirely.

**Index enrichment at read time, not cache time**: `recursive_size` fields are populated on every `get_file_range` call via a batch SQLite read from `dir_stats`. This avoids stale data and keeps enrichment consistent with the latest DB state. The cost is microseconds per page on a WAL connection.

**Enrichment uses integer-keyed batch lookup**: Instead of N individual `resolve_path()` calls (one per directory in the listing), `enrich_entries_with_index` resolves the parent directory once, queries `list_child_dir_ids_and_names(parent_id)` for all child dir IDs, then `get_dir_stats_batch_by_ids()`. Two indexed queries total instead of N. Falls back to individual path resolution for edge cases (for example, mixed-parent entries).

**IPC boundary stays path-based**: Frontend sends filesystem paths, backend resolves path→ID internally via `store::resolve_path()`. No frontend changes needed. IPC dir stats queries (`get_dir_stats`, `get_dir_stats_batch`) use `ReadPool` for lock-free reads, same as enrichment.

**Physical sizes (`st_blocks * 512`)**: More meaningful for disk usage than logical size. May overcount ~10-20% for APFS clones (shared blocks). Volume usage bar uses `statfs()` for true totals.

**MustScanSubDirs uses reconciliation, not delete-then-reinsert**: `reconcile_subtree()` diffs the filesystem against the DB directory-by-directory, only inserting/deleting/updating entries that changed. This is safe to interrupt at any point (no bulk delete phase that could leave the DB empty). For brand-new directories discovered during reconciliation, a `flush_blocking()` + re-resolve cycle ensures their IDs are available before recursing into them. `scanner::scan_subtree` (which uses destructive `DeleteDescendantsById`) is used by post-replay background verification for newly discovered directories.

**In-memory accumulation eliminates aggregation SQL queries**: During a full scan, the writer thread accumulates two HashMaps in `AccumulatorMaps` as `InsertEntriesV2` batches arrive: `direct_stats` (parent_id -> file size/count/dir count) and `child_dirs` (parent_id -> child dir IDs). When `ComputeAllAggregates` fires, these maps are passed to `compute_all_aggregates_with_maps()`, skipping the two expensive full-table-scan SQL queries (`bulk_get_children_stats_by_id` and `bulk_get_child_dir_ids`) that previously dominated aggregation time (~70%). Maps are cleared on `TruncateData` and after aggregation completes. Falls back to SQL queries if maps are empty.

**Pre-computed `name_folded` instead of SQL collation in the index (macOS)**: The old composite index `idx_parent_name(parent_id, name)` with `platform_case` collation took ~25 min to build for 5.1M entries because every B-tree comparison invoked NFD + case fold. The v5 workaround (simple `idx_parent` + match in Rust) required fetching all children per parent. `name_folded` stores the pre-computed `normalize_for_comparison(name)` at insert time, so the composite index uses binary collation and builds in seconds. `resolve_component` gets O(log n) lookups via a single indexed query.

**Subtree aggregation uses scoped queries**: `scoped_get_children_stats_by_id` and `scoped_get_child_dir_ids` in `aggregator.rs` use recursive CTEs scoped to the target subtree, not full-table scans. This keeps subtree aggregation O(subtree_size) regardless of total DB size.

**Bounded buffers prevent OOM**: All buffers have capacity limits. FSEvents channel: 32K batches (bounded `try_send` in cmdr-fsevent-stream; overflow sets atomic flag, triggers rescan). Reconciler buffer: 500K events (overflow triggers full rescan). Writer channel: 20K messages (bounded `sync_channel`, backpressure). Replay `affected_paths`: 50K entries (overflow emits full refresh). Replay `pending_rescans`: 1K entries (overflow triggers full rescan). Replay event count: 1M events max (overflow falls back to full scan). Memory watchdog: warns at 8 GB, stops indexing at 16 GB. The index is a disposable cache, so dropping events and rescanning is always safe.

**Disposable cache pattern**: The index DB is a cache, not a source of truth. Any corruption or error triggers delete+rebuild. No user-facing errors for DB issues.

**cmdr-fsevent-stream fork (macOS only)**: Vendored in `crates/fsevent-stream/` (forked from `fsevent-stream` v0.3.0). Provides direct access to FSEvents event IDs, `sinceWhen` replay, and `MustScanSubDirs` flags. Only used on macOS. On Linux, the `notify` crate (inotify backend) provides recursive directory watching with `RecursiveMode::Recursive`.

**Linux inotify watch limits**: Default `fs.inotify.max_user_watches` is ~8192. The `notify` crate's recursive mode adds one inotify watch per directory. Power users with large directory trees may hit this limit; the workaround is `sysctl fs.inotify.max_user_watches=524288`. The watcher gracefully handles watch errors without crashing.

**APFS firmlinks**: Scan from `/` only, skip `/System/Volumes/Data`. Normalize all paths via firmlink prefix map so DB lookups work regardless of how the user navigated to a path.

**Rescan notification system (`RescanReason` enum)**: Every code path that falls back to a full rescan emits an `index-rescan-notification` event with a `RescanReason` variant and human-readable details. The frontend maps each reason to a user-friendly toast message. Eight reasons: `StaleIndex` (pre-check gap), `JournalGap` (in-loop gap), `ReplayOverflow` (>1M events), `TooManySubdirRescans` (>1K MustScanSubDirs), `WatcherStartFailed`, `ReconcilerBufferOverflow` (>500K buffered events during scan), `IncompletePreviousScan` (has data but no `scan_completed_at`), `WatcherChannelOverflow` (FSEvents channel full, events dropped). The pre-check in `resume_or_scan()` catches stale indexes before starting the FSEvents stream, preventing the cmdr-fsevent-stream channel (32K capacity, `try_send`) from being overwhelmed.

## Gotchas

**INSERT OR REPLACE on a populated DB is catastrophically slow**: The `platform_case` collation (NFD + case fold on macOS) runs for every B-tree comparison during unique index lookups. On an empty DB a full scan takes ~2.5 min; on a populated DB with 5.5M entries the same scan takes ~30 min because each `INSERT OR REPLACE` triggers ~20 collation calls to traverse the B-tree. `start_scan()` truncates `entries` and `dir_stats` via `TruncateData` + `flush_blocking()` before every scan to avoid this. Additionally, without truncation, old rows accumulate as orphaned subtrees (3-4x DB bloat per scan cycle) because `INSERT OR REPLACE` only deduplicates at the root level.

**Cold-start replay enters live mode immediately after flush**: `run_replay_event_loop` (in `event_loop.rs`) doesn't emit `index-dir-updated` during Phase 1 (replay). It collects affected paths, flushes the writer (ensuring all writes are committed), emits a single batched notification, and enters live mode right away (~100ms from startup). Post-replay verification (`verify_affected_dirs`) runs in a background task (`run_background_verification`) concurrently with live events. This is safe because the writer serializes all writes. Any corrections found by verification are emitted as a separate `index-dir-updated` batch.

**FSEvents `item_removed` must be verified against disk**: macOS FSEvents can deliver `item_removed` for paths that still exist (atomic file swaps by editors/git, coalesced events with OR'd flags, `merge_fs_events` discarding `item_created` when `item_removed` is present). `handle_removal()` stats the path before deleting: if the file exists, it delegates to `handle_creation_or_modification()` (upsert) instead. Without this, false removals progressively delete live entries from the DB — especially damaging for directories since `DeleteSubtreeById` is recursive. `handle_creation_or_modification()` already has the inverse pattern: if stat fails, it deletes.

**Events are deduplicated and batched in all modes**: Live events (both `run_live_event_loop` and Phase 3 of `run_replay_event_loop`) use a 1s flush window. Replay events (Phase 1 of `run_replay_event_loop`) use `REPLAY_DEDUP_BATCH_SIZE` (1,000 events). Both collect into a `HashMap<String, FsChangeEvent>` keyed by normalized path and flush via `merge_fs_events`. Flag priority: `must_scan_sub_dirs` always wins, then `removed`, then `created`, then `modified`. `UpdateLastEventId` is sent once per batch. The replay dedup is critical for performance: high-churn files (SQLite journals, browser caches) can generate hundreds of identical FSEvents per second; without dedup, each event triggers a `symlink_metadata()` syscall and a `resolve_path()` component walk.

**Writer-side delete-with-propagation**: Both path-keyed (`DeleteEntry`/`DeleteSubtree`) and integer-keyed (`DeleteEntryById`/`DeleteSubtreeById`) handlers in the writer automatically read old data before deleting and propagate accurate negative deltas. The integer-keyed variants use `propagate_delta_by_id` which walks the `parent_id` chain via `get_parent_id` lookups. This means every deletion -- replay, live, verification -- gets correct dir_stats updates without callers needing to send separate `PropagateDelta` messages.

**Post-replay verification is bidirectional**: `verify_affected_dirs` (in `event_loop.rs`) checks both directions: (1) stale entries in DB but not on disk (sends `DeleteEntry`/`DeleteSubtree`), and (2) missing entries on disk but not in DB (sends `UpsertEntry` + `PropagateDelta` for files, collects directory paths for `scan_subtree`). Both directions filter children through `scanner::should_exclude` to prevent excluded system paths from being inserted as empty stubs. New directories are scanned and their subtree totals propagated up the ancestor chain. Uses a two-phase pattern: Phase 1 uses `ReadPool` (from `enrichment.rs`) for lock-free bulk SQLite reads into a `HashMap`, Phase 2 does all disk I/O without any lock. `run_background_verification`'s dir-stat reads also use `ReadPool`. No `INDEXING` lock is held during verification.

**Schema version mismatch drops the DB**: If `schema_version` in meta doesn't match what the code expects, the entire DB is deleted and rebuilt. No migration path (it's a cache, not user data).

**Verifier debounce is per-path, not global**: Each directory path gets its own 30s cooldown. Navigating to a different directory triggers a fresh verification even if another one just completed. `MAX_CONCURRENT_VERIFICATIONS` (2) prevents overloading the writer channel.

**Dirs created by reconciler/live events must get `dir_stats` immediately**: `UpsertEntryV2` inserts a zero-valued `dir_stats` row when creating a new directory. Without this, directories created after the last full aggregation have no `dir_stats` and show no sizes in the UI. `BackfillMissingDirStats` runs after reconciler replay and cold-start replay as a catch-up pass for any dirs that slipped through (for example, from older code before this fix). The zero-init + backfill combination guarantees every directory always has a `dir_stats` row.

**Scan cancellation leaves partial data**: By design. `scan_completed_at` not set in meta, so next startup detects incomplete scan and runs fresh. No cleanup needed.

**`ReadPool` replaces `INDEXING` lock for all read-only DB access**: Enrichment (`enrich_entries_with_index` in `enrichment.rs`), verification Phase 1 (`verify_affected_dirs` in `event_loop.rs`), background verification dir-stat reads, and IPC dir stats queries (`get_dir_stats`, `get_dir_stats_batch` in `mod.rs`) all use `get_read_pool()` + `pool.with_conn()` — thread-local SQLite connections with no lock contention. The `INDEXING` mutex now guards only lifecycle transitions (start, stop, clear, status). `with_conn` uses `thread_local!` storage. Its signature `fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> T)` ensures the `&Connection` can't escape the closure (the return type `T` is lifetime-independent), so async task migration can't break thread affinity. This is enforced by the type system, not convention.

**Progress events use `tauri::async_runtime::spawn`**: Not `tokio::spawn`, because indexing can start from Tauri's synchronous `setup()` hook where no Tokio runtime context exists.

**`platform_case` collation must be registered on every connection**: The custom collation is not persisted in the DB file. Both `IndexStore::open()` and `open_write_connection()` register it. Forgetting to register before querying causes `no such collation sequence: platform_case` errors. On macOS it uses NFD normalization + case folding (matching APFS). On Linux it's binary (zero overhead).

**Backward-compat shims resolve paths via component walk**: Old path-keyed functions (`get_entry`, `delete_entry`, `upsert_entry`, etc.) internally call `resolve_path()` which walks the tree component-by-component. This means parent directories MUST exist before inserting children. The aggregator's path-keyed `propagate_delta` and `compute_subtree_aggregates` also resolve paths internally. The reconciler no longer uses these shims -- it sends integer-keyed messages directly (milestone 4). Enrichment no longer uses the path-keyed `get_dir_stats_batch` -- it uses integer-keyed batch lookups via `list_child_dir_ids_and_names` + `get_dir_stats_batch_by_ids` (milestone 5). Remaining users of path-keyed shims: `verify_affected_dirs` (post-replay verification). Cleanup in milestone 6.

**Reconciler holds a read connection**: `process_fs_event`, `replay`, and `process_live_event` all require a `&Connection` parameter for path-to-ID resolution. Callers (event loops in `event_loop.rs`) open a read connection via `IndexStore::open_write_connection(writer.db_path())` at loop start and pass it through. This is a WAL-mode connection so it doesn't block the writer.

**ScanContext maps scan root to ROOT_ID**: Both `scan_volume` and `scan_subtree` create a `ScanContext` that maps the scan root directory to `ROOT_ID` (1). This means all top-level entries under any scan root get `parent_id = ROOT_ID` in the DB. For subtree scans, the root is resolved to its existing entry ID (not ROOT_ID), and `DeleteDescendantsById` is sent before the scan starts. The `ScanContext` opens a temporary read connection to the DB to fetch `next_id` via `get_next_id()`.

**Reconciler must delete old subtree on dir-to-file type changes**: When `reconcile_subtree` matches a filesystem entry to a DB entry by name, it must check if `is_directory` changed. If a directory became a file, `DeleteSubtreeById` must be sent before `UpsertEntryV2`. Without this, `INSERT OR REPLACE` keeps the same row ID (same `parent_id + name`), and the old directory's children become logical orphans — entries parented by a file.

**Scanner's `insert_entries_v2_batch` uses plain `INSERT`**: With the old `idx_parent_name` unique index, `INSERT OR REPLACE` would silently delete the old row and insert a new one with a new ID, orphaning all children. That unique index is gone (replaced by `idx_parent_name_folded` on macOS / `idx_parent_name` on Linux), and the only unique constraint is the integer PK (`id`). Since `ScanContext` assigns unique IDs and the table is truncated before full scans (or descendants deleted before subtree scans), PK conflicts shouldn't occur. The batch insert uses plain `INSERT` to reflect this.

**IndexWriter exposes `db_path()`**: The scanner needs the DB path to open a temporary connection for `ScanContext::new()`. This path is stored on the `IndexWriter` handle and accessible via `db_path()`. The temporary connection is short-lived (only used to read `MAX(id)`).

**Verifier tests must avoid `/tmp/` for filesystem roots**: On Linux, `/tmp/` is in `EXCLUDED_PREFIXES`. Tests that create filesystem trees and run `verify_and_correct` must use `test_tempdir()` (creates temp dirs in `CARGO_MANIFEST_DIR`) so `should_exclude` doesn't filter out new entries. The DB temp dir can still use `tempfile::tempdir()` since it's never checked by `should_exclude`.
