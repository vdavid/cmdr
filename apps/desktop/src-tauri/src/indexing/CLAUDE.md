# Drive indexing module

Background-indexes local volumes into per-volume SQLite databases, tracking every file and directory with recursive
size aggregates so listings can show directory sizes.

## Module map

- **Lifecycle / state**: `state.rs` (`IndexPhase` machine, `INDEXING` mutex, public API), `manager.rs` (coordinator).
- **Write path**: `writer.rs` (single writer thread + write connection), `scanner.rs` (jwalk walk), `aggregator.rs`
  (dir-stats), `reconciler.rs` + `event_loop.rs` (replay + live FSEvents).
- **Read path**: `enrichment.rs` (`ReadPool`, listing enrichment), `store.rs` (schema, queries, collation),
  `verifier.rs` (per-navigation readdir diff), `expected_totals.rs`, `pending_sizes.rs`.
- **Support**: `partial_agg.rs`, `metadata.rs`, `firmlinks.rs`, `watcher.rs`, `memory_watchdog.rs`, `events.rs`. Thin
  IPC in `commands/indexing.rs`; frontend in `src/lib/indexing/`; search in `src/search/`.

## Must-knows (invariants and guardrails)

- **Single-writer thread.** All writes go through one writer thread via a bounded `sync_channel` (backpressure on full).
  Reads use a `ReadPool` of thread-local WAL connections, NOT the `INDEXING` mutex (which guards only lifecycle
  transitions); don't move read paths (enrichment, verification, IPC dir-stats) back under it. `stop_indexing` and
  `clear_index` must drop the guard BEFORE `mgr.shutdown()`'s 5 s drain.
- **`platform_case` collation must be registered on every connection.** It isn't persisted, so the `sqlite3` CLI fails
  on any query touching `name`; use `index-query`.
- **Don't drop `UNIQUE (parent_id, name_folded)`**: it's the data-safety net against two writers racing on one DB
  (seen once as a 1.83 TB ghost size). Don't drop `name_folded` either; without it the composite-index rebuild takes
  ~25 min on macOS.
- **Scanner uses `INSERT OR IGNORE`, not `INSERT OR REPLACE`.** REPLACE reassigns IDs (orphaning children) and is
  catastrophically slow on a populated DB. `start_scan` truncates `entries` + `dir_stats` before every scan, and the
  accumulator counts only rows that landed (per-row OR-IGNORE flags), never bytes a row lost.
- **Mid-scan partial aggregation must BORROW the accumulator maps read-only, never consume/mutate them**, and its
  empty-maps no-op must stay SQL-free (a late partial pass legitimately lands after the final aggregation cleared the
  maps). Either violation silently ships wrong sizes. Keep `try_send` non-blocking, and fire partial passes only from
  the full-scan progress loop (its death scopes them to the scan window).
- **The index is a disposable cache.** Schema-version mismatch drops + rebuilds; corruption heals by rescan; no online
  migrations or user-facing DB errors. Bounded buffers make dropping events then rescanning safe.
- **An interrupted scan must heal to a fresh rescan**, via two cooperating writes: `start_scan` clears
  `scan_completed_at` before truncating, and the completion handler writes meta only when `!was_cancelled`. Gate only
  the meta writes on it, never the reconcile/live transition.
- **`start_indexing` is lock-first**: claim `Disabled -> Initializing` atomically BEFORE building the heavy
  `IndexManager`, else two near-simultaneous starts spawn writer threads racing on one DB.
- **Reconciler/event loops hold a READ connection** (`open_read_connection`), never a write one: write-mode pragmas can
  `SQLITE_BUSY` and silently kill live indexing for the session.
- **Defer indexer auto-start until FDA is decided** (`should_auto_start_indexing`): a first-launch scan from `/` opens
  TCC-protected dirs and stacks native popups over the FDA modal.
- **macOS: the writer thread (and any thread calling ObjC/Cocoa) must wrap work in `objc2::rc::autoreleasepool`** or
  autoreleased `NSData` / `NSInvocation` objects leak (multi-GB over hours).
- **Spawn indexing tasks with `tauri::async_runtime::spawn`, not `tokio::spawn`**: indexing can start from the sync
  `setup()` hook, where `tokio::spawn` panics (no runtime).
- **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter.** Don't allocate IDs from `MAX(id)` on a read connection:
  uncommitted channel inserts make it stale and double-assign IDs.
- **FSEvents `item_removed` must be stat-verified before deleting** (atomic swaps and coalesced events deliver false
  removals); upsert instead when the path exists.

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md). Read it whole before structural changes.
