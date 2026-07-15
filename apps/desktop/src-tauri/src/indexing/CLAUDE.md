# Drive indexing module

Background-indexes each volume (local, SMB, MTP) into its own SQLite DB with recursive size aggregates, so listings show
directory sizes.

## Module map

Full per-file grouping: DETAILS § "Module structure". Subdirs: `scanner/`, `writer/`, `aggregator/`,
[`store/`](store/CLAUDE.md) (reads + schema). Key top-level files: `state.rs` +
`manager.rs` (lifecycle), `local_reconcile.rs` / `volume_scanner.rs` (LOCAL / SMB-MTP scan), `reconciler.rs` +
`event_loop.rs` (live), `enrichment.rs` (`ReadPool`), `freshness.rs`. IPC `commands/indexing.rs`; frontend
`src/lib/indexing/`; search `src/search/`.

## Must-knows

All invariants hold PER volume id (why: DETAILS).

- **`INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`) is the authority** — absent key = disabled (no
  `Disabled` phase). The mutex guards lifecycle ONLY; reads route through the per-volume `ReadPool` (never under it);
  enrichment skips when `get_read_pool_for` is `None`.
- **Phase transitions go through `events::set_phase_for(app, volume_id, phase, trigger)`, never raw
  `DEBUG_STATS.set_phase`.** It records the global timeline AND emits the per-volume `index-phase-changed`, so they
  can't drift. Network (SMB/MTP) emits only `Scanning → Live`. DETAILS § "Per-volume pipeline phase event".

Writer discipline (one writer thread per DB):

- **`start_indexing` is lock-first**: reserve the registry slot before building `IndexManager` (else two starts race on
  one DB). **Never hold `INDEX_REGISTRY` across a blocking or re-entrant manager call** (froze the UI
  on real hardware): drop the guard before `shutdown()` / `start_scan`. DETAILS § "Drop the registry guard".
- **Reconciler/event loops hold a READ connection, never a write one** (`SQLITE_BUSY` silently kills live indexing).
  **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter** — don't allocate from `MAX(id)` (uncommitted inserts →
  double-assign).
- **Live file upserts are throttled 60 s** (leading + trailing, not debounce; `reconciler/throttle.rs`): a file
  rewritten in place writes ≤1/window. Invisible for files (pane sizes are live `lstat`), so no schema/marker; a
  `pending` key is NEVER evictable. Bypass thresholds, trailing-flush, UNthrottled paths: DETAILS § "Live per-file write
  throttle".
- **Mid-scan partial aggregation has four easy-to-break rules** — DETAILS § "Key decisions".
- **The index is a disposable cache**: a schema mismatch or corruption deletes and rebuilds the DB (no migrations;
  schema in [`store/CLAUDE.md`](store/CLAUDE.md)). Gate only `scan_completed_at` writes (absence ⇒ heal to rescan).
- **Defer `root` auto-start** (`should_auto_start_indexing`): scanning from `/` stacks TCC popups. FDA gates ONLY
  `root`; don't route SMB/MTP through it.

SMB/MTP indexing:

- **Gated on a `direct` (smb2) connection; an `os_mount` upgrades first** (`start_indexing_for_smb` refuses with a TYPED
  `SmbIndexGateReason`); MTP has no gate.
- **Manual rescan routes by TYPED kind** (`force_scan`): SMB/MTP → `start_volume_scan`, `Local` → `start_scan`. ❌
  Never `start_scan` a trait-scanned volume — it walks nothing over a network mount and falsely completes (the "rescan
  does nothing to the NAS" bug). LOCAL `start_scan` reconciles a populated index in place, truncate-walks a fresh one;
  both hang-tolerant. DETAILS § "LOCAL full rescan reconciles in place".
- **Never write `scan_completed_at` for an empty root** (an empty `/` must not blank the index; the reconcile returns
  typed `EmptyRoot`, not `Ok`). DETAILS § "No completion marker on an empty root".
- **Freshness has ONE transition table (`freshness.rs`); don't branch elsewhere.** No journal ⇒ loads **Stale** on
  launch. The manager fires via `apply_freshness_event_on` (no registry re-lock).
- **Live watch runs with NO pane open** (`apply_smb_change` hooks `notify_directory_changed` before the pane early-
  return; don't remove).
- **Deletes resolve against the INDEX**: delete only a known entry (unknown name/handle = no-op); local `item_removed`
  stat-verifies.
- **Threads + resources.** One GLOBAL 16 GB memory watchdog (`stop_all_indexing`; idempotent). Wrap ObjC/Cocoa threads
  in `objc2::rc::autoreleasepool` (else multi-GB leaks). Use `tauri::async_runtime::spawn`; `tokio::spawn` panics from
  the sync `setup()` hook.

- **External-drive tests: synthetic disk images ONLY, `hdiutil` calls timeout-guarded** (a real-card unmount once
  kernel-panicked the machine). `indexing::external_drive_fixture`; DETAILS § "Testing external drives".

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
