# Drive indexing module

Background-indexes local volumes into per-volume SQLite databases, tracking every file and directory with recursive
size aggregates so listings can show directory sizes.

## Module map

- **Lifecycle / state**: `state.rs` (`IndexPhase` machine, the per-volume `INDEX_REGISTRY`, public API), `manager.rs`
  (per-volume coordinator).
- **Write path**: `writer/` (single writer thread + write connection; `mod.rs` = protocol + loop, `entries.rs` /
  `delta.rs` / `aggregation.rs` / `maintenance.rs` = handlers), `scanner.rs` (jwalk walk, local only),
  `volume_scanner.rs` (`Volume`-trait recursive scan for SMB/network), `aggregator.rs` (dir-stats), `reconciler.rs` +
  `event_loop.rs` (replay + live FSEvents).
- **SMB / freshness**: `freshness.rs` (the `Freshness` state machine: blue/green/yellow; gray = no instance),
  `smb_index.rs` (the direct-smb2 gate + per-volume SMB enable).
- **Read path**: `enrichment.rs` (`ReadPool`, listing enrichment), `store.rs` (schema, queries, collation),
  `verifier.rs` (per-navigation readdir diff), `expected_totals.rs`, `pending_sizes.rs`.
- **Support**: `partial_agg.rs`, `metadata.rs`, `firmlinks.rs`, `watcher.rs`, `memory_watchdog.rs`, `events.rs`. Thin
  IPC in `commands/indexing.rs`; frontend in `src/lib/indexing/`; search in `src/search/`.

## Must-knows (invariants and guardrails)

- **Per-volume registry.** `INDEX_REGISTRY: Mutex<HashMap<VolumeId, IndexInstance{phase, read_pool, pending_sizes}>>` is
  the authority for which volumes are indexed; an absent key = disabled (no `Disabled` phase). Every invariant below
  holds PER volume id; in M1 only `root` is registered, so behavior is byte-identical to single-volume. Reads route by
  volume id (`get_read_pool_for`); enrichment SKIPS when it returns `None` ("no index registered", replacing the old
  `should_exclude` early-return, so non-root listings skip with zero DB work). Root's pool/tracker stay in the
  `READ_POOL`/`PENDING_SIZES` globals (same `Arc` the root instance holds). DETAILS § registry.
- **Single-writer thread, per DB; reads off the lock.** Writes go through one writer thread (bounded `sync_channel`,
  backpressure on full). Reads use the per-volume `ReadPool` (thread-local WAL conns), NOT the registry mutex (lifecycle
  only); don't move read paths back under it. `stop_indexing` / `clear_index` take/remove the instance under the lock,
  then drop the guard BEFORE `mgr.shutdown()`'s 5 s drain.
- **`platform_case` collation must be registered on every connection.** It isn't persisted, so the `sqlite3` CLI fails
  on any query touching `name`; use `index-query`.
- **Don't drop `UNIQUE (parent_id, name_folded)`** (data-safety net against two writers racing on one DB; seen once as a
  1.83 TB ghost size) **nor `name_folded`** (without it the composite-index rebuild takes ~25 min on macOS).
- **Scanner uses `INSERT OR IGNORE`, not `INSERT OR REPLACE`.** REPLACE reassigns IDs (orphaning children) and is
  catastrophically slow on a populated DB. `start_scan` truncates `entries` + `dir_stats` before every scan; the
  accumulator counts only rows that landed (per-row OR-IGNORE flags), never bytes a row lost.
- **Mid-scan partial aggregation must BORROW the accumulator maps read-only** (never consume/mutate), and its empty-maps
  no-op must stay SQL-free (a late pass legitimately lands after the final aggregation cleared the maps) — either
  violation silently ships wrong sizes. Keep `try_send` non-blocking; fire passes only from the full-scan progress loop.
- **The index is a disposable cache.** Schema-version mismatch drops + rebuilds; corruption heals by rescan; no online
  migrations or user-facing DB errors. Bounded buffers make dropping events then rescanning safe.
- **An interrupted scan heals to a fresh rescan** via two cooperating writes: `start_scan` clears `scan_completed_at`
  before truncating; the completion handler writes meta only when `!was_cancelled`. Gate only the meta writes, never the
  reconcile/live transition.
- **`start_indexing` is lock-first, per volume**: claim `(absent) -> Initializing` for that volume id atomically BEFORE
  building the heavy `IndexManager`, else two starts for the SAME volume race writer threads on one DB. Different volumes
  start freely.
- **Reconciler/event loops hold a READ connection** (`open_read_connection`), never a write one: write-mode pragmas can
  `SQLITE_BUSY` and silently kill live indexing.
- **Defer `root` auto-start until FDA is decided** (`should_auto_start_indexing`): a first-launch scan from `/` stacks
  TCC popups over the FDA modal. FDA gates ONLY `root` — SMB/MTP starts (`start_indexing_for_smb`) must not route
  through it (not TCC-protected).
- **SMB indexing is gated on a `direct` (smb2) connection; an `os_mount` is upgraded first.** `start_indexing_for_smb`
  refuses with a TYPED `SmbIndexGateReason` (never a message substring) when the upgrade fails / needs creds / is
  disconnected. SMB scans walk via `volume_scanner` (the `Volume` trait), NOT jwalk (`scanner`'s `should_exclude` blocks
  `/Volumes/`). The walker is cancelable per round trip, `timeout`-wrapped, and `autoreleasepool`-drained per listing.
- **Freshness (per-volume, in `freshness.rs`) has ONE transition table; don't branch on it elsewhere.** SMB/MTP have no
  journal, so a persisted index loads **Stale** on launch (correct, not a bug) and a clean scan ⇒ Fresh. An
  interrupted/disconnected SMB scan DISCARDS the partial and resets to gray (`reset_to_not_indexed`) — don't keep a
  half-snapshot live. Live SMB `CHANGE_NOTIFY` keeps it Fresh; watcher death / overflow ⇒ Stale (next bullet).
- **SMB watch→index (`smb_watch.rs`, see DETAILS § "Live SMB watch → index").** `apply_smb_change` is hooked into
  `notify_directory_changed` BEFORE the pane early-return, so it runs with no pane open. Three things to get right: (1)
  the SMB index's `ROOT_ID` is the MOUNT ROOT, so translate events to MOUNT-RELATIVE paths (`index_relative_path`) before
  `resolve_path` — a mount-absolute path resolves to nothing; (2) sequence the index write BEFORE the FE refresh (the
  writer's `EmitDirUpdated` fires `index-dir-updated` only after the write commits), and enqueue on the volume's writer,
  never write directly; (3) changes during a scan are BUFFERED (`scanning` flips true before the truncate) and replayed
  after — don't apply them against the rebuilding index. Watcher death / overflow call
  `smb_index::on_smb_watcher_died` / `on_smb_overflow` ⇒ Stale.
- **The memory watchdog is a single GLOBAL budget, not per-volume.** It stops EVERY volume's index at 16 GB
  (`state::stop_all_indexing`); scans run in parallel (the wire, not RAM, is the bottleneck). Start is idempotent (one
  watchdog task across all volumes).
- **macOS: the writer thread (and any thread calling ObjC/Cocoa) must wrap work in `objc2::rc::autoreleasepool`** or
  autoreleased `NSData` / `NSInvocation` leak (multi-GB over hours).
- **Spawn indexing tasks with `tauri::async_runtime::spawn`, not `tokio::spawn`** (indexing can start from the sync
  `setup()` hook, where `tokio::spawn` panics).
- **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter.** Don't allocate IDs from `MAX(id)` on a read connection:
  uncommitted channel inserts make it stale and double-assign.
- **FSEvents `item_removed` must be stat-verified before deleting** (atomic swaps and coalesced events deliver false
  removals); upsert instead when the path exists.

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
