# Drive indexing module

Background-indexes each volume (local, SMB, MTP) into its own SQLite DB with recursive size aggregates.

## Module map

Full per-file grouping: DETAILS § "Module structure". Subdirs: `scanner/`, `writer/`, `aggregator/`,
[`store/`](store/CLAUDE.md). Key top-level: `state.rs` + `manager.rs` (lifecycle), `local_reconcile.rs` /
`volume_scanner.rs` (LOCAL / SMB-MTP scan), `reconciler.rs` + `event_loop.rs` (live), `enrichment.rs`, `freshness.rs`.
IPC `commands/indexing.rs`; frontend `src/lib/indexing/`; search `src/search/`.

## Must-knows

All invariants hold PER volume id (DETAILS).

- **`INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`) is the authority** — absent key = disabled (no
  `Disabled` phase). It guards lifecycle ONLY; reads route through the per-volume `ReadPool`, never under it.
- **Phase transitions go through `events::set_phase_for(...)`, never raw `DEBUG_STATS.set_phase`.**

Writer discipline (one writer thread per DB):

- **`start_indexing` is lock-first**: reserve the registry slot before building `IndexManager` (else two starts race).
  **Never hold `INDEX_REGISTRY` across a blocking/re-entrant manager call** (froze the UI once): drop the guard before
  `shutdown()` / `start_scan`. DETAILS § "Drop the registry guard".
- **Reconciler/event loops hold a READ connection, never a write one** (`SQLITE_BUSY` silently kills live indexing).
  **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter** — never allocate from `MAX(id)` (uncommitted inserts →
  double-assign).
- **Live file upserts are throttled 60 s** (`reconciler/throttle.rs`): ≤1 write/window; a `pending` key is NEVER
  evictable. DETAILS § "Live per-file write throttle".
- **The index is a disposable cache**: a schema mismatch or corruption deletes and rebuilds the DB (no migrations). Gate
  only `scan_completed_at`.
- **Defer `root` auto-start** (`should_auto_start_indexing`): scanning `/` stacks TCC popups; FDA gates ONLY `root`.

`dir_stats` is a delta-adjusted ledger (DETAILS § "The dir_stats ledger"):

- **Never clamp `dir_stats` arithmetic** — a negative delta is drift evidence; escalate to `repair_dir_stats_upward`,
  never `.max(0)` (which floored a real 1.21 GB to "0 bytes").
- **Structural rewrites repair ancestors ON THE WRITER** (subtree scan, backfill); never off-writer read-then-credit.
- **Full-aggregate senders declare `source: Maps|Sql`** (`Maps` only for a fresh scan; reconcile/heal `Sql`); never
  clear the accumulator in the subtree handler.

SMB/MTP indexing:

- **Gated on a `direct` (smb2) connection; an `os_mount` upgrades first** (`start_indexing_for_smb` refuses with a TYPED
  `SmbIndexGateReason`).
- **Manual rescan routes by TYPED kind** (`force_scan`): SMB/MTP → `start_volume_scan`, local → `start_scan`; never
  `start_scan` a trait-scanned volume (walks nothing, false-completes).
- **Never write `scan_completed_at` for an empty root** (typed `EmptyRoot`); a yanked drive's unlistable root → typed
  `RootUnlistable`: abort, no completion. DETAILS § "No completion marker on an empty root".
- **`should_exclude(path, ExclusionScope)` derives scope from the volume kind, NEVER `is_volume_root`** (else
  `MountRooted` false-completes).
- **The LOCAL scan/reconcile/live pipeline is mount-relative via `IndexPathSpace`.** Strip the mount root ONLY at the
  `resolve_abs` argument; keep path sets + the FE emit ABSOLUTE.
- **Live watch runs with NO pane open** (`apply_smb_change` hooks before the pane early-return; don't remove).
- **Freshness has ONE transition table (`freshness.rs`); don't branch elsewhere.** No journal ⇒ loads **Stale**.
- **`resume_or_scan` gates journal replay on `has_event_journal()`, NOT `last_event_id.is_some()`** (a `LocalExternal`
  index persists an event id but has no journal). DETAILS § "Capability axes".
- **Deletes resolve against the INDEX** (unknown = no-op); local `item_removed` stat-verifies.
- **FAT/exFAT (`LocalExternal`) store `inode: None`** (`IndexPathSpace::trust_inode`): a reused derived inode
  false-matches the rename pre-pass, corrupting `dir_stats`; don't restore it.
- **Threads + resources.** Wrap ObjC/Cocoa threads in `objc2::rc::autoreleasepool`; use `tauri::async_runtime::spawn`
  (`tokio::spawn` panics in `setup()`). GLOBAL 16 GB memory watchdog stops indexing (`stop_all_indexing`).

- **The 2026-07-15 FSKit panic governs external drives.** Stop a `LocalExternal` index BEFORE its volume unmounts — an
  open FSEvents watcher at unmount can wedge FSKit and kernel-panic (eject stops it before `diskutil`). Tests: synthetic
  images ONLY. DETAILS §§ "Unmount/eject lifecycle", "Testing external drives".

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
