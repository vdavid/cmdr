# Drive indexing module

Background-indexes each volume (local, SMB, MTP) into its own SQLite DB with recursive size aggregates.

## Module map

Full per-file grouping: DETAILS § "Module structure". Subdirs: `scanner/`, `writer/`, `aggregator/`,
[`store/`](store/CLAUDE.md). Key top-level: `state.rs` + `manager.rs` (lifecycle), `local_reconcile.rs` /
`volume_scanner.rs` (LOCAL / SMB-MTP scan), `reconciler.rs` + `event_loop.rs` (live), `enrichment.rs`, `freshness.rs`.
IPC `commands/indexing.rs`, frontend `src/lib/indexing/`, search `src/search/`.

## Must-knows

All invariants hold PER volume id; full depth in [DETAILS.md](DETAILS.md).

- **`INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`) is the authority**: absent key = disabled. Guards
  lifecycle ONLY; reads route through the per-volume `ReadPool`, never under it.
- **Phase transitions go through `events::set_phase_for(...)`**, never raw `DEBUG_STATS.set_phase`.

Writer discipline (one writer thread per DB):

- **`start_indexing` is lock-first**: reserve the registry slot before building `IndexManager` (else two starts race).
  **Never hold `INDEX_REGISTRY` across a blocking/re-entrant manager call** (froze the UI once).
- **Reconciler/event loops hold a READ connection, never a write one** (`SQLITE_BUSY` kills live indexing). **`IndexWriter`
  owns the shared `Arc<AtomicI64>` ID counter**; never allocate from `MAX(id)` (uncommitted inserts → double-assign). A
  drifted counter self-heals: an upsert insert hitting PK 1555 resyncs from the DB and retries once (else the entry is
  dropped forever). Never extend that to UNIQUE 2067 — a name conflict retried under a new id IS the duplicate row.
  Live file upserts throttle 60 s (`reconciler/throttle.rs`): ≤1 write/window, `pending` never evictable.
- **`MustScanSubDirs` is depth-split** (`reconciler/rescan_route.rs`): a SHALLOW anchor (`depth ≤ 2`, root-scale) routes
  via `route_must_scan_sub_dirs` to the VISIBLE scanner (`start_scan`, 45 s cooldown) with NO hourglass hold — holding it
  for a ~20-min reconcile-of-`/` is the stuck-hourglass bug. A DEEP anchor keeps the per-subtree-throttled reconcile
  drain (`rescan_throttle.rs`, 60 s). Depth: DETAILS § "Depth-split MustScanSubDirs routing".
- **The watcher→loop channel is UNBOUNDED** (`mpsc::unbounded_channel`): a slow drain must never backpressure FSEvents
  into dropping events (that used to force a full scan). Don't re-bound it. Memory is capped by
  `classify_ingestion_pressure` instead: warn at 20K, deliberate full scan past `INGESTION_HARD_CAP` (5M). DETAILS §
  "Unbounded ingestion buffer".
- **The index is a disposable cache**: a schema mismatch or corruption deletes and rebuilds the DB (no migrations). Gate
  only `scan_completed_at`.
- **A fatal storage error STOPS + FAILS the index, never retries** (an incident logged 12,700 warnings in 8 min): the
  writer trips `IndexFailureSignal` on the first fatal SQLite error → `Failed` + `Freshness::Failed`. Recovery is rebuild;
  BUSY/LOCKED stay retried.
- **Defer `root` auto-start** (`should_auto_start_indexing`): scanning `/` stacks TCC popups; FDA gates ONLY `root`.

**The `dir_stats` delta-adjusted ledger, three hard rules** (DETAILS § "The dir_stats ledger"):

- **Never clamp the arithmetic**: a negative delta is drift; escalate to `repair_dir_stats_upward`, never `.max(0)`
  (floored a real 1.21 GB to "0 bytes").
- **Structural rewrites repair ancestors ON THE WRITER** (subtree scan, backfill), never off-writer read-then-credit.
- **Full-aggregate senders declare `source: Maps|Sql`** (`Maps` only for a fresh scan); never clear the accumulator in
  the subtree handler.

SMB/MTP + external-drive indexing:

- **Gated on a `direct` (smb2) connection; an `os_mount` upgrades first** (typed `SmbIndexGateReason`).
- **Reconnect/upgrade AUTO-RESUMES, gated on PERSISTED state**: resume ONLY when a scan completed AND `user_disabled`
  isn't set (the disable command writes it, NOT `stop_indexing`).
- **Manual rescan routes by TYPED kind** (`force_scan`): SMB/MTP → `start_volume_scan`, local → `start_scan`; never
  `start_scan` a trait-scanned volume (false-completes).
- **Never write `scan_completed_at` for an empty root** (typed `EmptyRoot`) or an unlistable one (`RootUnlistable`):
  abort, no completion.
- **The local walker gives up on a subtree after 32 consecutive failed reads** (dead mount): descendants left
  honest-stale (`listed_epoch=0`), never completed or zeroed.
- **`should_exclude(path, ExclusionScope)` derives scope from the volume KIND, never `is_volume_root`** (else
  `MountRooted` false-completes).
- **The LOCAL scan/reconcile/live pipeline is mount-relative via `IndexPathSpace`**: strip the mount root ONLY at
  `resolve_abs`; path sets + FE emit stay ABSOLUTE.
- **Live watch runs with NO pane open** (`apply_smb_change` hooks before the pane early-return; don't remove).
- **Freshness has ONE transition table (`freshness.rs`); don't branch elsewhere.** No journal ⇒ loads **Stale**.
- **Deletes resolve against the INDEX** (unknown = no-op); local `item_removed` stat-verifies. **FAT/exFAT
  (`LocalExternal`) store `inode: None`** (`trust_inode`): a reused derived inode false-matches the rename pre-pass and
  corrupts `dir_stats`; don't restore it.
- **FSKit panic (2026-07-15): stop a `LocalExternal` index BEFORE its volume unmounts.** An open FSEvents watcher at
  unmount can wedge FSKit → kernel panic (eject stops it first). Test with synthetic disk images ONLY.
- **A GLOBAL 16 GB memory watchdog stops ALL indexing.** Scans spawn via `tauri::async_runtime::spawn` (`tokio::spawn`
  panics in `setup()`).

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
