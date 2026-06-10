# Drive indexing module

Background-indexes local volumes into per-volume SQLite databases, tracking every file and directory with recursive
size aggregates. The headline UX win: directory sizes in file listings.

## Module map

- **Lifecycle / state**: `state.rs` (`IndexPhase` state machine, `INDEXING` mutex, public API), `manager.rs`
  (central coordinator: store, writer thread, scanner, watcher).
- **Write path**: `writer.rs` (single writer thread, owns the write connection), `scanner.rs` (jwalk parallel walk),
  `aggregator.rs` (dir-stats compute), `reconciler.rs` + `event_loop.rs` (replay + live FSEvents).
- **Read path**: `enrichment.rs` (`ReadPool`, listing enrichment), `store.rs` (SQLite schema, queries, collation),
  `verifier.rs` (per-navigation readdir diff), `expected_totals.rs`, `pending_sizes.rs`.
- **Support**: `partial_agg.rs`, `metadata.rs`, `firmlinks.rs`, `watcher.rs`, `memory_watchdog.rs`, `events.rs`.
- IPC in `commands/indexing.rs` (thin wrappers); frontend in `src/lib/indexing/`. Search moved to `src/search/`.

## Must-knows (invariants and guardrails)

- **Single-writer thread.** All writes go through one `std::thread` via a bounded `sync_channel` (20K, backpressure).
  Reads use a `ReadPool` of thread-local WAL connections, NOT the `INDEXING` mutex. Don't move read paths
  (enrichment, verification, IPC dir-stats) back under `INDEXING`; that mutex guards only lifecycle transitions.
  `stop_indexing`/`clear_index` must drop the `INDEXING` guard BEFORE `mgr.shutdown()`'s 5 s drain.
- **`platform_case` collation must be registered on every connection** (`IndexStore::open` + `open_write_connection`).
  It isn't persisted in the DB, so the `sqlite3` CLI fails on any query touching `name`. Use `index-query` instead.
- **`UNIQUE (parent_id, name_folded)` is a data-safety net** against two writers racing on one DB (observed once as a
  1.83 TB ghost size). Don't drop it. The `name_folded` column is load-bearing too: dropping it makes the composite
  index rebuild take ~25 min on macOS (NFD + case-fold per comparison).
- **Scanner uses `INSERT OR IGNORE`, not `INSERT OR REPLACE`.** REPLACE reassigns IDs (orphans children) and is
  catastrophically slow on a populated DB. `start_scan` truncates `entries` + `dir_stats` before every scan.
  `handle_insert_entries_v2` filters the accumulator input by the returned per-row flags, so in-memory state never
  claims bytes that lost the OR-IGNORE.
- **Mid-scan partial aggregation must BORROW the accumulator maps read-only, never consume/mutate them**, and its
  empty-maps no-op must stay SQL-free (a late partial pass legitimately arrives after the final aggregation cleared
  the maps). Violating either silently ships wrong sizes. Don't make the `try_send` blocking, and don't fire partial
  passes outside the full-scan progress loop (that loop's death is what scopes the feature to the scan window).
- **The index is a disposable cache.** Schema-version mismatch drops + rebuilds the DB; any corruption heals by
  rescan. No online migrations, no user-facing errors for DB issues. All buffers are bounded; dropping events and
  rescanning is always safe.
- **An interrupted scan must heal to a fresh rescan**, via two cooperating writes: `start_scan` clears
  `scan_completed_at` before truncating, and the completion handler writes meta only when `!was_cancelled`. Don't gate
  the reconcile/live transition on `was_cancelled`; only the meta writes are gated.
- **`start_indexing` is lock-first**: claim `Disabled -> Initializing` atomically BEFORE building the heavy
  `IndexManager`. Two near-simultaneous starts would otherwise both spawn writer threads racing on one DB.
- **Defer indexer auto-start until FDA is decided** (`should_auto_start_indexing`): scanning from `/` at first launch
  opens TCC-protected dirs and stacks native popups over the in-app FDA modal. Shares the `is_fda_pending` predicate
  with `volumes::list_locations` icon fetches.
- **macOS: writer thread (and any thread calling ObjC/Cocoa) must wrap work in `objc2::rc::autoreleasepool`** or
  autoreleased `NSData`/`NSInvocation` objects leak (multi-GB over hours). Progress events use
  `tauri::async_runtime::spawn`, not `tokio::spawn` (indexing can start from the sync `setup()` hook).
- **Reconciler/event loops hold a READ connection** (`open_read_connection`), never a write connection. Switching to
  write-mode pragmas can `SQLITE_BUSY` and silently kill live indexing for the session.
- **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter.** Don't allocate IDs from `MAX(id)` on a read
  connection (uncommitted inserts in the channel make it stale, causing double-assigned IDs).
- **FSEvents `item_removed` must be stat-verified before deleting** (atomic swaps / coalesced events deliver false
  removals); `handle_removal` upserts instead when the path still exists.
- **Enrichment early-returns for excluded parents** (`scanner::should_exclude`) so network mounts / external drives
  don't log "Parent path not found" on every refresh.
- **Don't `eprintln!`/`println!`; use scoped `log` targets.** Two high-volume per-event lines are at TRACE by default.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
