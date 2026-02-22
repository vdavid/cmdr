# Drive indexing module

Background-indexes local volumes into per-volume SQLite databases, tracking every file and directory with recursive size aggregates. The key UX win: showing directory sizes in file listings.

Full design: `docs/specs/drive-indexing/plan.md`

## Architecture

### Module structure

- **mod.rs** -- Public API: `init()`, `start_indexing()`, `stop_indexing()`, `clear_index()`, `enrich_entries_with_index()`. `IndexManager` coordinates all subsystems. Global read-only store for enrichment.
- **store.rs** -- SQLite schema (entries, dir_stats, meta), read queries (`get_dir_stats_batch`, `get_index_status`), DB open/migrate. Schema version check: mismatch triggers drop+rebuild.
- **writer.rs** -- Single writer thread, owns the write connection, processes `WriteMessage` channel (unbounded mpsc). Priority: `UpdateDirStats` before `InsertEntries`. `Flush` variant + async `flush()` method let callers wait for all prior writes to commit.
- **scanner.rs** -- jwalk-based parallel directory walker. `scan_volume()` for full scan, `scan_subtree()` for micro-scans. Exclusion filter for macOS system paths. Physical sizes (`st_blocks * 512`).
- **micro_scan.rs** -- `MicroScanManager`: bounded task pool (default 3 concurrent), priority queue (`UserSelected` > `CurrentDir`), deduplication, cancellation. Skips after full scan completes.
- **aggregator.rs** -- Dir stats computation. Bottom-up after full scan (O(N) single pass), per-subtree after micro-scan, incremental delta propagation up ancestor chain for watcher events.
- **watcher.rs** -- Drive-level FSEvents watcher via `cmdr-fsevent-stream`. File-level events with event IDs. Supports `sinceWhen` for cold-start replay.
- **reconciler.rs** -- Buffers FSEvents during scan, replays after scan completes using event IDs to skip stale events. Processes live events for file creates/removes/modifies. Key functions (`process_fs_event`, `emit_dir_updated`) are `pub(super)` so `mod.rs` can call them directly during cold-start replay.
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
  |   |-- Has existing index + last_event_id? -> sinceWhen replay (FSEvents journal)
  |   |-- Otherwise -> fresh full scan
  |
Full scan:
  |-- Start DriveWatcher (sinceWhen=0, buffers events)
  |-- jwalk parallel walk -> batched InsertEntries -> writer thread -> SQLite
  |-- On complete: replay buffered events (reconciler), compute all aggregates, switch to live mode
  |
Live mode:
  |-- FSEvents -> reconciler -> UpsertEntry/DeleteEntry/DeleteSubtree -> writer (auto-propagates deltas) -> SQLite
  |-- Affected paths batched in HashSet, flushed to frontend every 300 ms via index-dir-updated event
  |
Enrichment (every get_file_range call):
  |-- enrich_entries_with_index() -> batch SELECT from dir_stats -> populate FileEntry fields
```

### Single-writer architecture

All writes go through a dedicated `std::thread` via an unbounded mpsc channel. The writer thread owns the write connection and processes messages in order, prioritizing `UpdateDirStats` over `InsertEntries` for responsive micro-scan results.

Reads happen on separate WAL connections (any thread). The global read-only store (`GLOBAL_INDEX_STORE`) provides enrichment without passing `AppHandle` through the listing pipeline.

### SQLite schema

One DB per volume: `~/Library/Application Support/com.veszelovszki.cmdr/index-{volume_id}.db`

Three tables: `entries` (path, parent_path, name, is_directory, is_symlink, size, modified_at), `dir_stats` (recursive_size, recursive_file_count, recursive_dir_count), `meta` (key-value for schema_version, last_event_id, scan metadata). All `WITHOUT ROWID`, WAL mode, 64 MB page cache.

## How to test

Run Rust tests:
```sh
cd apps/desktop/src-tauri && cargo nextest run indexing
```

Key test files are alongside each module (test functions within `#[cfg(test)]` blocks). Tests use temp dirs and real SQLite to verify:
- Store: schema creation, reads, writes, batch operations, schema mismatch handling
- Aggregator: bottom-up computation, subtree-only computation, delta propagation
- Scanner: full scan with temp dir trees, exclusion filtering, cancellation
- Firmlinks: path normalization, edge cases
- Micro-scan manager: priority ordering, deduplication, cancellation
- Writer: message processing, priority handling

## Key decisions

**Single-writer thread, not connection pooling**: SQLite write concurrency is limited by its single-writer design. Instead of fighting it with `BUSY_TIMEOUT` and retries, one dedicated thread owns the write connection. Eliminates contention entirely.

**Index enrichment at read time, not cache time**: `recursive_size` fields are populated on every `get_file_range` call via a batch SQLite read from `dir_stats`. This avoids stale cache entries when micro-scans complete. The cost is microseconds per page on a WAL connection.

**Physical sizes (`st_blocks * 512`)**: More meaningful for disk usage than logical size. May overcount ~10-20% for APFS clones (shared blocks). Volume usage bar uses `statfs()` for true totals.

**Dev mode gating**: `CMDR_DRIVE_INDEX=1` env var required in dev mode (debug builds). Production auto-starts by default. This prevents accidental full-disk scans during development.

**Disposable cache pattern**: The index DB is a cache, not a source of truth. Any corruption or error triggers delete+rebuild. No user-facing errors for DB issues.

**cmdr-fsevent-stream fork**: Our fork of `fsevent-stream` (v0.3.0) provides direct access to FSEvents event IDs, `sinceWhen` replay, and `MustScanSubDirs` flags. The existing `notify` crate stays for per-directory file watchers (different use case).

**APFS firmlinks**: Scan from `/` only, skip `/System/Volumes/Data`. Normalize all paths via firmlink prefix map so DB lookups work regardless of how the user navigated to a path.

## Gotchas

**Cold-start replay enters live mode immediately after flush**: The `run_replay_event_loop` doesn't emit `index-dir-updated` during Phase 1 (replay). It collects affected paths, flushes the writer (ensuring all writes are committed), emits a single batched notification, re-enables micro-scans, and enters live mode right away (~100ms from startup). Post-replay verification (`verify_affected_dirs`) runs in a background task (`run_background_verification`) concurrently with live events. This is safe because the writer serializes all writes. Any corrections found by verification are emitted as a separate `index-dir-updated` batch.

**Live events are batched with a 300 ms window**: Both `run_live_event_loop` and the Phase 3 live loop in `run_replay_event_loop` use `tokio::select!` with a 300 ms `tokio::time::interval` to collect affected paths in a `HashSet` and emit a single `index-dir-updated` per flush. This prevents UI flicker from rapid per-event notifications (FSEvents can fire hundreds of events per second during bulk operations). `process_live_event` collects paths into the caller's `HashSet` instead of emitting directly.

**Writer-side delete-with-propagation**: `DeleteEntry` and `DeleteSubtree` handlers in the writer automatically read old data before deleting and propagate accurate negative deltas. This means every deletion -- replay, live, verification -- gets correct dir_stats updates without callers needing to send separate `PropagateDelta` messages. `delete_subtree` and `propagate_delta` have no internal transactions, so they're safe inside the replay's `BEGIN IMMEDIATE` transaction.

**Post-replay verification is bidirectional**: `verify_affected_dirs` checks both directions: (1) stale entries in DB but not on disk (sends `DeleteEntry`/`DeleteSubtree`), and (2) missing entries on disk but not in DB (sends `UpsertEntry` + `PropagateDelta` for files, collects directory paths for `scan_subtree`). New directories are scanned and their subtree totals propagated up the ancestor chain. The `GLOBAL_INDEX_STORE` mutex guard is scoped to avoid holding it across `.await` points (the guard is not `Send`).

**Schema version mismatch drops the DB**: If `schema_version` in meta doesn't match what the code expects, the entire DB is deleted and rebuilt. No migration path (it's a cache, not user data).

**`verifier.rs` is a placeholder**: Per-navigation readdir diff is a future milestone. Currently just a TODO comment.

**Scan cancellation leaves partial data**: By design. `scan_completed_at` not set in meta, so next startup detects incomplete scan and runs fresh. No cleanup needed.

**Global read-only store uses `std::sync::Mutex`**: Not `RwLock`, because `rusqlite::Connection` is `Send` but not `Sync`. The mutex is held briefly for each batch read.

**Progress events use `tauri::async_runtime::spawn`**: Not `tokio::spawn`, because indexing can start from Tauri's synchronous `setup()` hook where no Tokio runtime context exists.
