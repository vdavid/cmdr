# Drive indexing module

Background-indexes local volumes into per-volume SQLite databases, tracking every file and directory with recursive
size aggregates. UX win: directory sizes in file listings.

## Module map

- **Lifecycle / state**: `state.rs` (`IndexPhase` machine, `INDEXING` mutex, public API), `manager.rs` (coordinator:
  store, writer thread, scanner, watcher).
- **Write path**: `writer.rs` (single writer thread, owns the write connection), `scanner.rs` (jwalk walk),
  `aggregator.rs` (dir-stats), `reconciler.rs` + `event_loop.rs` (replay + live FSEvents).
- **Read path**: `enrichment.rs` (`ReadPool`, listing enrichment), `store.rs` (schema, queries, collation),
  `verifier.rs` (per-navigation readdir diff), `expected_totals.rs`, `pending_sizes.rs`.
- **Support**: `partial_agg.rs`, `metadata.rs`, `firmlinks.rs`, `watcher.rs`, `memory_watchdog.rs`, `events.rs`. IPC in
  `commands/indexing.rs` (thin); frontend in `src/lib/indexing/`. Search is in `src/search/`.

## Must-knows (invariants and guardrails)

- **Single-writer thread.** All writes go through one `std::thread` via a bounded `sync_channel` (20K, backpressure).
  Reads use a `ReadPool` of thread-local WAL connections, NOT the `INDEXING` mutex (which guards only lifecycle
  transitions); don't move read paths (enrichment, verification, IPC dir-stats) back under it. `stop_indexing` /
  `clear_index` must drop the `INDEXING` guard BEFORE `mgr.shutdown()`'s 5 s drain.
- **`platform_case` collation must be registered on every connection** (`IndexStore::open` + `open_write_connection`).
  It isn't persisted, so the `sqlite3` CLI fails on any query touching `name`; use `index-query`.
- **`UNIQUE (parent_id, name_folded)` is a data-safety net** against two writers racing on one DB (observed once as a
  1.83 TB ghost size). Don't drop it. `name_folded` is load-bearing too: dropping it makes the composite-index rebuild
  take ~25 min on macOS (NFD + case-fold per comparison).
- **Scanner uses `INSERT OR IGNORE`, not `INSERT OR REPLACE`.** REPLACE reassigns IDs (orphans children) and is
  catastrophically slow on a populated DB. `start_scan` truncates `entries` + `dir_stats` before every scan.
  `handle_insert_entries_v2` filters accumulator input by the per-row flags, so in-memory state never claims bytes that
  lost the OR-IGNORE.
- **Mid-scan partial aggregation must BORROW the accumulator maps read-only, never consume/mutate them**, and its
  empty-maps no-op must stay SQL-free (a late partial pass legitimately arrives after the final aggregation cleared the
  maps). Violating either silently ships wrong sizes. Don't make `try_send` blocking, and fire partial passes only
  inside the full-scan progress loop (its death scopes them to the scan window).
- **The index is a disposable cache.** Schema-version mismatch drops + rebuilds; corruption heals by rescan. No online
  migrations, no user-facing DB errors. All buffers are bounded, so dropping events and rescanning is always safe.
- **An interrupted scan must heal to a fresh rescan**, via two cooperating writes: `start_scan` clears
  `scan_completed_at` before truncating, and the completion handler writes meta only when `!was_cancelled`. Gate only
  the meta writes on it, never the reconcile/live transition.
- **`start_indexing` is lock-first**: claim `Disabled -> Initializing` atomically BEFORE building the heavy
  `IndexManager`, else two near-simultaneous starts both spawn writer threads racing on one DB.
- **Reconciler/event loops hold a READ connection** (`open_read_connection`), never a write one. Write-mode pragmas can
  `SQLITE_BUSY` and silently kill live indexing for the session.
- **Defer indexer auto-start until FDA is decided** (`should_auto_start_indexing`): scanning from `/` at first launch
  opens TCC-protected dirs and stacks native popups over the in-app FDA modal. Shares `is_fda_pending` with
  `volumes::list_locations` icon fetches.
- **macOS: the writer thread (and any thread calling ObjC/Cocoa) must wrap work in `objc2::rc::autoreleasepool`** or
  autoreleased `NSData` / `NSInvocation` objects leak (multi-GB over hours). Progress events use
  `tauri::async_runtime::spawn`, not `tokio::spawn` (indexing can start from the sync `setup()`).
- **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter.** Don't allocate IDs from `MAX(id)` on a read connection:
  uncommitted inserts in the channel make it stale, double-assigning IDs.
- **FSEvents `item_removed` must be stat-verified before deleting** (atomic swaps / coalesced events deliver false
  removals); `handle_removal` upserts instead when the path still exists.

Architecture, flows, decisions, and lower-severity gotchas (enrichment log-noise, per-event TRACE logging):
[DETAILS.md](DETAILS.md). Read it whole before structural changes here.
