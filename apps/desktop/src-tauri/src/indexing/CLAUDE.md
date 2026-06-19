# Drive indexing module

Background-indexes volumes into per-volume SQLite DBs, tracking every file and directory with recursive size aggregates
so listings can show directory sizes. Local disk, SMB shares, and MTP storages each get their own index DB.

## Module map

- **Lifecycle / state**: `state.rs` (`IndexPhase` machine, the per-volume `INDEX_REGISTRY`, public API), `manager.rs`
  (per-volume coordinator).
- **Write path**: `writer/` (single writer thread + write connection), `scanner.rs` (jwalk, local only),
  `volume_scanner.rs` (`Volume`-trait scan for SMB/MTP), `aggregator.rs`, `reconciler.rs` + `event_loop.rs` (replay +
  live FSEvents).
- **SMB / MTP / freshness**: `freshness.rs` (the `Freshness` state machine: blue/green/yellow; gray = no instance),
  `smb_index.rs` / `mtp_index.rs` (enable + disconnect→Stale hook), `smb_watch.rs` / `mtp_watch.rs` (live change →
  index-write translation).
- **Read path**: `enrichment.rs` (`ReadPool`, listing enrichment), `store.rs` (schema, queries, collation),
  `verifier.rs`, `expected_totals.rs`, `pending_sizes.rs`.
- **Support**: `partial_agg.rs`, `metadata.rs`, `firmlinks.rs`, `watcher.rs`, `memory_watchdog.rs`, `events.rs`,
  `retention.rs`. Thin IPC in `commands/indexing.rs`; frontend in `src/lib/indexing/`; search in `src/search/`.

## Must-knows (invariants and guardrails)

Every invariant holds PER volume id; DETAILS has the why, flows, and decisions.

- **Per-volume registry is the authority.** `INDEX_REGISTRY: Mutex<HashMap<VolumeId, IndexInstance>>`; an absent key =
  disabled (no `Disabled` phase). Reads route by volume id and use the per-volume `ReadPool` (thread-local WAL conns) —
  NOT the registry mutex, which guards lifecycle only; don't move read paths back under it. Enrichment SKIPS when
  `get_read_pool_for` returns `None`, so an unindexed volume costs zero DB work.
- **One writer thread per DB; never write to a DB directly.** Bounded `sync_channel` (backpressure on full).
  `stop_indexing` / `clear_index` take/remove the instance under the lock, then drop the guard BEFORE the shutdown drain.
- **`start_indexing` is lock-first, per volume**: claim `(absent) -> Initializing` atomically BEFORE building the heavy
  `IndexManager`, else two starts for the SAME volume race writer threads on one DB. Different volumes start freely.
- **`platform_case` collation must be registered on every connection** (not persisted): the `sqlite3` CLI fails on any
  query touching `name`; use `index-query`.
- **Don't drop `UNIQUE (parent_id, name_folded)`** (data-safety net against two writers racing on one DB; seen once as a
  1.83 TB ghost size) **nor `name_folded`** (without it the composite-index rebuild takes ~25 min on macOS).
- **Scanner uses `INSERT OR IGNORE`, not `INSERT OR REPLACE`** (REPLACE reassigns IDs, orphaning children, and is
  catastrophically slow on a populated DB).
- **Mid-scan partial aggregation must BORROW the accumulator maps read-only** (never consume/mutate), and its empty-maps
  no-op must stay SQL-free (a late pass legitimately lands after the final aggregation cleared the maps) — either
  violation silently ships wrong sizes. Keep `try_send` non-blocking; fire passes only from the full-scan progress loop.
- **Reconciler/event loops hold a READ connection** (`open_read_connection`), never a write one: write-mode pragmas can
  `SQLITE_BUSY` and silently kill live indexing.
- **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter.** Don't allocate IDs from `MAX(id)` on a read connection:
  uncommitted channel inserts make it stale and double-assign.
- **The index is a disposable cache**: schema mismatch / corruption drops + rebuilds; no online migrations or user-facing
  DB errors. An interrupted scan heals to a rescan — gate only the `scan_completed_at` meta writes, never the
  reconcile/live transition.
- **Defer `root` auto-start until FDA is decided** (`should_auto_start_indexing`): a first-launch scan from `/` stacks
  TCC popups over the FDA modal. FDA gates ONLY `root`; SMB/MTP starts aren't TCC-protected and must not route through it.
- **SMB indexing is gated on a `direct` (smb2) connection; an `os_mount` is upgraded first.** `start_indexing_for_smb`
  refuses with a TYPED `SmbIndexGateReason` (never a message substring). SMB/MTP scans walk via `volume_scanner` (the
  `Volume` trait), NOT jwalk (`scanner`'s `should_exclude` blocks `/Volumes/`); the walker is cancelable per round trip,
  `timeout`-wrapped, and `autoreleasepool`-drained. MTP enable is FDA-independent with no smb2 gate.
- **Freshness has ONE transition table (`freshness.rs`); don't branch on it elsewhere.** SMB/MTP have no journal, so a
  persisted index loads **Stale** on launch (correct, not a bug); a clean scan ⇒ Fresh. An interrupted/disconnected
  SMB/MTP scan DISCARDS the partial and resets to gray (`reset_to_not_indexed`). Live `CHANGE_NOTIFY` / PTP events keep
  it Fresh; SMB watcher death/overflow and MTP disconnect ⇒ Stale.
- **Live watch → index runs with NO pane open** (`apply_smb_change` hooks into `notify_directory_changed` before the
  pane early-return). Three load-bearing rules, easy to break — read DETAILS § "Live SMB watch → index" before touching
  this: SMB paths must be stripped mount-absolute → mount-relative on BOTH write and read (or an SMB folder shows no
  sizes; MTP is storage-relative already); the index write is sequenced before the FE refresh and enqueued on the
  volume's writer; changes arriving during a scan are buffered and replayed after.
- **Deletes resolve against the INDEX, not a live stat** (SMB/MTP): delete only when the removed name/handle is a known
  index entry; an unknown one is a no-op, a recreate heals via the separate add. (Local FSEvents `item_removed` is the
  exception — a stat is cheap on local disk, so it stat-verifies and upserts when the path still exists.)
- **The memory watchdog is a single GLOBAL budget** (`state::stop_all_indexing` at 16 GB), not per-volume; start is
  idempotent.
- **macOS: the writer thread (and any thread calling ObjC/Cocoa) must wrap work in `objc2::rc::autoreleasepool`** or
  autoreleased `NSData` / `NSInvocation` leak (multi-GB over hours).
- **Spawn indexing tasks with `tauri::async_runtime::spawn`, not `tokio::spawn`** (indexing can start from the sync
  `setup()` hook, where `tokio::spawn` panics).

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
