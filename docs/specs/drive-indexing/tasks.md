# Drive indexing tasks

All items marked done = plan is fully implemented in great quality.
See also: [plan.md](plan.md), [research.md](research.md), [benchmarks.md](benchmarks.md)

## Milestone 1: Core index infrastructure

- [ ] Create `src-tauri/src/indexing/` module with `mod.rs` public API
- [ ] Add `rusqlite` (bundled) and `jwalk` dependencies to `Cargo.toml`
- [ ] `store.rs`: SQLite schema (entries, dir_stats, meta tables), open/migrate, drop+rebuild on schema mismatch
- [ ] `store.rs`: read queries — `get_dir_stats_batch`, `get_index_status`, `list_entries_by_parent`
- [ ] `writer.rs`: single writer thread with bounded mpsc channel, priority handling (DirStats before InsertEntries)
- [ ] `scanner.rs`: `scan_volume()` full parallel walk with jwalk, physical sizes (`st_blocks * 512`)
- [ ] `scanner.rs`: `scan_subtree()` targeted subtree scan (shared core with full scan)
- [ ] `scanner.rs`: scan exclusions by absolute path prefix (all system paths from plan)
- [ ] `scanner.rs`: scan cancellation via `CancellationToken`, partial data left as-is
- [ ] `aggregator.rs`: bottom-up dir_stats computation after full scan (O(N) single pass)
- [ ] `aggregator.rs`: per-subtree dir_stats computation after micro-scan
- [ ] `micro_scan.rs`: `MicroScanManager` with priority queue, bounded task pool (2-4), dedup, cancellation
- [ ] `micro_scan.rs`: `UserSelected` and `CurrentDir` priorities, cancel `CurrentDir` on navigate-away
- [ ] `micro_scan.rs`: cancel all scans on app shutdown (don't block quit)
- [ ] `firmlinks.rs`: parse `/usr/share/firmlinks`, build prefix map, normalize paths
- [ ] Progress events (`index-scan-started`, `index-scan-progress`, `index-scan-complete`, `index-dir-updated`)
- [ ] IPC commands: `start_drive_index`, `stop_drive_index`, `get_index_status`
- [ ] IPC commands: `get_dir_stats`, `get_dir_stats_batch`, `prioritize_dir`, `cancel_nav_priority`
- [ ] Rust tests: store (schema, reads, writes, migration)
- [ ] Rust tests: aggregator (bottom-up, per-subtree)
- [ ] Rust tests: scanner (with temp dirs, exclusions, cancellation)
- [ ] Rust tests: firmlink normalization
- [ ] Rust tests: micro-scan manager (priority, dedup, cancellation)

## Milestone 2: Dev mode + debug window

- [ ] `CMDR_DRIVE_INDEX=1` env var gating (no scan in dev mode without it; always runs in production)
- [ ] Debug window: "Drive index" section with status display ("Scanning... N / ? entries" or "Ready: N entries, M dirs")
- [ ] Debug window: "Start scan" and "Clear index" buttons
- [ ] Debug window: volume selector, last scan timestamp, duration, last event ID
- [ ] Debug window: live display of index events (listen to all `index-*` events)
- [ ] Manual test: run scan via debug window, verify entries counted, clear and re-scan

## Milestone 3: Frontend display — directory sizes

- [ ] Add `recursiveSize`, `recursiveFileCount`, `recursiveDirCount` to `FileEntry` (Rust struct + TypeScript type)
- [ ] `enrich_with_index_data()` in `get_file_range`: batch `dir_stats` lookup, populate fields on directory entries
- [ ] `index-state.svelte.ts`: Svelte 5 reactive state for index status per volume
- [ ] `index-events.ts`: event listeners for `index-scan-*` and `index-dir-updated`
- [ ] `index-priority.ts`: call `prioritize_dir` on Space/navigate, `cancel_nav_priority` on navigate-away
- [ ] `index-dir-updated` handler: each pane checks if any updated path is a child of its current dir, re-fetches if so
- [ ] FullList.svelte: size column shows spinner + "Scanning..." when no data, formatted size when available
- [ ] FullList.svelte: ⚠️ on stale sizes, tooltip "Might be outdated. Currently scanning..."
- [ ] FullList.svelte: tooltip with "1.23 GB · 4,521 files · 312 folders"
- [ ] BriefList.svelte: tooltip on cursor directory with size info, spinner/⚠️ states
- [ ] SelectionInfo: spinner/⚠️ logic when selected directories are scanning or stale
- [ ] Scan status overlay: top-right corner, spinner + "Scanning..." during full scan, event count during replay
- [ ] Svelte tests for new display states (no data, scanning, stale, fresh, empty dir)

## Milestone 4: FSEvents watcher + reconciliation

- [ ] Add `cmdr-fsevent-stream` git dependency to `Cargo.toml`
- [ ] `watcher.rs`: recursive FSEvents watcher on volume root with file-level events and event IDs
- [ ] `reconciler.rs`: buffer events during scan, replay after scan completes (compare event IDs vs scan progress)
- [ ] `aggregator.rs`: incremental delta propagation up ancestor chain on file add/remove/modify
- [ ] `watcher.rs`: `MustScanSubDirs` handling — queue subtree rescan, max 1 concurrent, dedup by path
- [ ] `writer.rs`: store last processed event ID in meta table on every write batch
- [ ] Frontend: enriched FileEntry updates when dir_stats change for visible directories
- [ ] Rust tests: reconciler (buffered events, event ID ordering, replay correctness)
- [ ] Rust tests: incremental propagation (add file, remove file, modify size, remove dir with subtree)

## Milestone 5: Persistence + cold start

- [ ] On startup: read `last_event_id` from meta, start FSEvents with `sinceWhen`
- [ ] Apply replayed journal events to DB (same code path as live mode)
- [ ] Detect unavailable journal (sinceWhen too old) → fall back to full scan
- [ ] Show existing index data immediately on startup while replay runs
- [ ] Wake from sleep: verify FSEvents stream stays alive, `MustScanSubDirs` handles overflow
- [ ] Store scan metadata (timestamp, duration, entry count) in meta table
- [ ] Detect schema mismatch (different app version) → drop and rebuild
- [ ] Manual test: quit app, make file changes, relaunch, verify sizes update within seconds

## Milestone 6: Settings + user controls

- [ ] Add "Drive indexing" subsection under "Settings > General"
- [ ] Toggle to enable/disable drive indexing (default: enabled)
- [ ] Display current index size (SQLite DB file size on disk), updated live
- [ ] "Clear index" action (drops DB, resets state, restarts scan if enabled)
- [ ] Verify disabling stops all scans and watchers, sizes revert to `<dir>`

## Milestone 7: Volume scanner/watcher traits

- [ ] Define `VolumeScanner` and `VolumeWatcher` traits
- [ ] Add `scanner()` and `watcher()` optional methods to `Volume` trait (default `None`)
- [ ] Implement `VolumeScanner` for `LocalPosixVolume` (wraps jwalk)
- [ ] Implement `VolumeWatcher` for `LocalPosixVolume` (wraps cmdr-fsevent-stream)
- [ ] Refactor indexing module to use traits instead of direct crate calls
- [ ] Verify existing volume types still work (MTP, InMemory, SMB)

## Milestone 8: Polish + checks

- [ ] Run all checks: `./scripts/check.sh` (clippy, rustfmt, eslint, prettier, svelte-check, knip, stylelint)
- [ ] Run Rust tests: `./scripts/check.sh --check rust-tests`
- [ ] Run Svelte tests: `./scripts/check.sh --check svelte-tests`
- [ ] Add new Tauri/DOM-dependent files to `coverage-allowlist.json`
- [ ] Create `src-tauri/src/indexing/CLAUDE.md` with module overview
- [ ] Update `docs/architecture.md` with indexing module entry
