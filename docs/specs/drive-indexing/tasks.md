# Drive indexing tasks

All items marked done = plan is fully implemented in great quality.
See also: [plan.md](plan.md), [research.md](research.md), [benchmarks.md](benchmarks.md)

## Milestone 1: Core index infrastructure

- [x] Create `src-tauri/src/indexing/` module with `mod.rs` public API
- [x] Add `rusqlite` (bundled) and `jwalk` dependencies to `Cargo.toml`
- [x] `store.rs`: SQLite schema (entries, dir_stats, meta tables), open/migrate, drop+rebuild on schema mismatch
- [x] `store.rs`: read queries — `get_dir_stats_batch`, `get_index_status`, `list_entries_by_parent`
- [x] `writer.rs`: single writer thread with bounded mpsc channel, priority handling (DirStats before InsertEntries)
- [x] `scanner.rs`: `scan_volume()` full parallel walk with jwalk, physical sizes (`st_blocks * 512`)
- [x] `scanner.rs`: `scan_subtree()` targeted subtree scan (shared core with full scan)
- [x] `scanner.rs`: scan exclusions by absolute path prefix (all system paths from plan)
- [x] `scanner.rs`: scan cancellation via `CancellationToken`, partial data left as-is
- [x] `aggregator.rs`: bottom-up dir_stats computation after full scan (O(N) single pass)
- [x] `aggregator.rs`: per-subtree dir_stats computation after micro-scan
- [x] `micro_scan.rs`: `MicroScanManager` with priority queue, bounded task pool (2-4), dedup, cancellation
- [x] `micro_scan.rs`: `UserSelected` and `CurrentDir` priorities, cancel `CurrentDir` on navigate-away
- [x] `micro_scan.rs`: cancel all scans on app shutdown (don't block quit)
- [x] `firmlinks.rs`: parse `/usr/share/firmlinks`, build prefix map, normalize paths
- [x] Progress events (`index-scan-started`, `index-scan-progress`, `index-scan-complete`, `index-dir-updated`)
- [x] IPC commands: `start_drive_index`, `stop_drive_index`, `get_index_status`
- [x] IPC commands: `get_dir_stats`, `get_dir_stats_batch`, `prioritize_dir`, `cancel_nav_priority`
- [x] Rust tests: store (schema, reads, writes, migration)
- [x] Rust tests: aggregator (bottom-up, per-subtree)
- [x] Rust tests: scanner (with temp dirs, exclusions, cancellation)
- [x] Rust tests: firmlink normalization
- [x] Rust tests: micro-scan manager (priority, dedup, cancellation)

## Milestone 2: Dev mode + debug window

- [x] ~~`CMDR_DRIVE_INDEX=1` env var gating~~ Removed: indexing now auto-starts in dev mode too
- [x] Debug window: "Drive index" section with status display ("Scanning... N / ? entries" or "Ready: N entries, M dirs")
- [x] Debug window: "Start scan" and "Clear index" buttons
- [x] Debug window: volume selector, last scan timestamp, duration, last event ID
- [x] Debug window: live display of index events (listen to all `index-*` events)
- [ ] Manual test: run scan via debug window, verify entries counted, clear and re-scan

## Milestone 3: Frontend display — directory sizes

- [x] Add `recursiveSize`, `recursiveFileCount`, `recursiveDirCount` to `FileEntry` (Rust + TypeScript)
- [x] `enrich_with_index_data()` in `get_file_range`: batch `dir_stats` lookup, populate fields on directory entries
- [x] `index-state.svelte.ts`: Svelte 5 reactive state for index status per volume (+ initial status query on mount)
- [x] `index-events.ts`: event listeners for `index-scan-*` and `index-dir-updated`
- [x] `index-priority.ts`: call `prioritize_dir` on Space/navigate, `cancel_nav_priority` on navigate-away
- [x] `index-dir-updated` handler: each pane checks if any updated path is a child of its current dir, re-fetches if so
- [x] FullList.svelte: size column shows spinner + "Scanning..." when no data, formatted size when available
- [x] FullList.svelte: ⚠️ on stale sizes, tooltip "Might be outdated. Currently scanning..."
- [x] FullList.svelte: tooltip with "1.23 GB · 4,521 files · 312 folders"
- [x] BriefList.svelte: tooltip on cursor directory with size info, spinner/⚠️ states
- [x] SelectionInfo: spinner/⚠️ logic when selected directories are scanning or stale
- [x] Scan status overlay: top-right corner, spinner + "Scanning..." during full scan, event count during replay
- [x] Svelte tests for new display states (no data, scanning, stale, fresh, empty dir)

## Milestone 4: FSEvents watcher + reconciliation

- [x] Add `cmdr-fsevent-stream` git dependency to `Cargo.toml`
- [x] `watcher.rs`: recursive FSEvents watcher on volume root with file-level events and event IDs
- [x] `reconciler.rs`: buffer events during scan, replay after scan completes (compare event IDs vs scan progress)
- [x] `aggregator.rs`: incremental delta propagation up ancestor chain on file add/remove/modify
- [x] `watcher.rs`: `MustScanSubDirs` handling — queue subtree rescan, max 1 concurrent, dedup by path
- [x] `writer.rs`: store last processed event ID in meta table on every write batch
- [x] Frontend: enriched FileEntry updates when dir_stats change for visible directories
- [x] Rust tests: reconciler (buffered events, event ID ordering, replay correctness)
- [x] Rust tests: incremental propagation (add file, remove file, modify size, remove dir with subtree)

## Milestone 5: Persistence + cold start

- [x] On startup: read `last_event_id` from meta, start FSEvents with `sinceWhen`
- [x] Apply replayed journal events to DB (same code path as live mode)
- [x] Detect unavailable journal (sinceWhen too old) → fall back to full scan
- [x] Show existing index data immediately on startup while replay runs
- [x] Wake from sleep: verify FSEvents stream stays alive, `MustScanSubDirs` handles overflow
- [x] Store scan metadata (timestamp, duration, entry count) in meta table
- [x] Detect schema mismatch (different app version) → drop and rebuild
- [ ] Manual test: quit app, make file changes, relaunch, verify sizes update within seconds

## Milestone 6: Settings + user controls

- [x] Add "Drive indexing" subsection under "Settings > General"
- [x] Toggle to enable/disable drive indexing (default: enabled)
- [x] Display current index size (SQLite DB file size on disk), updated live
- [x] "Clear index" action (drops DB, resets state, restarts scan if enabled)
- [ ] Verify disabling stops all scans and watchers, sizes revert to `<dir>`

## Milestone 7: Volume scanner/watcher traits

- [x] Define `VolumeScanner` and `VolumeWatcher` traits
- [x] Add `scanner()` and `watcher()` optional methods to `Volume` trait (default `None`)
- [x] Implement `VolumeScanner` for `LocalPosixVolume` (wraps jwalk)
- [x] Implement `VolumeWatcher` for `LocalPosixVolume` (wraps cmdr-fsevent-stream)
- [ ] Refactor indexing module to use traits instead of direct crate calls (deferred: threading `Box<dyn VolumeScanner>` through `MicroScanManager`, `EventReconciler`, and `IndexManager` is medium-complexity for no immediate gain -- only `LocalPosixVolume` supports scanning. The traits exist and work; the indirection can be added when a second scannable volume type arrives.)
- [x] Verify existing volume types still work (MTP, InMemory, SMB)

## Milestone 8: Polish + checks

- [x] Run all checks: `./scripts/check.sh` (clippy, rustfmt, eslint, prettier, svelte-check, knip, stylelint)
- [x] Run Rust tests: `./scripts/check.sh --check rust-tests`
- [x] Run Svelte tests: `./scripts/check.sh --check svelte-tests`
- [x] Add new Tauri/DOM-dependent files to `coverage-allowlist.json`
- [x] Create `src-tauri/src/indexing/CLAUDE.md` with module overview
- [x] Update `docs/architecture.md` with indexing module entry
