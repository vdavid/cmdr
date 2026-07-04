# Drive indexing module

Background-indexes each volume (local disk, SMB, MTP) into its own SQLite DB with recursive size aggregates, so listings
show directory sizes.

## Module map

Full per-file grouping: DETAILS § "Module structure" or `codegraph_search`. Subdirs: `scanner/` (jwalk scan), `writer/`,
`aggregator/`, [`store/`](store/CLAUDE.md) (reads + SQLite schema; own docs). Key top-level files: `state.rs` +
`manager.rs` (lifecycle), `local_reconcile.rs` / `volume_scanner.rs` (LOCAL / SMB-MTP scan), `reconciler.rs` +
`event_loop.rs` (live), `enrichment.rs` (`ReadPool`), `freshness.rs`. IPC `commands/indexing.rs`; frontend
`src/lib/indexing/`; search `src/search/`.

## Must-knows

All invariants hold PER volume id; DETAILS has the why.

- **`INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`) is the authority** — absent key = disabled (no
  `Disabled` phase). The mutex guards lifecycle ONLY; reads route through the per-volume `ReadPool`, never under it, and
  enrichment skips when `get_read_pool_for` is `None`.
- **Phase transitions go through `events::set_phase_for(app, volume_id, phase, trigger)`, never raw
  `DEBUG_STATS.set_phase`.** It records the global debug-window timeline AND emits the per-volume `index-phase-changed`
  event (the FE step checklist) in one call, so they can't drift. Network (SMB/MTP) emits only `Scanning → Live`, so
  drive the "compute folder sizes" step off aggregation events, not a phase it never sends. DETAILS § "Per-volume
  pipeline phase event".

Writer discipline (one writer thread per DB):

- **`start_indexing` is lock-first**: reserve the registry slot before building `IndexManager` (else two starts race
  writer threads on one DB). **Never hold `INDEX_REGISTRY` across a blocking or re-entrant manager call** (froze the UI
  on real hardware): drop the guard before `shutdown()` / `start_scan`. DETAILS § "Drop the registry guard".
- **Reconciler/event loops hold a READ connection, never a write one** (`SQLITE_BUSY` silently kills live indexing).
  **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter** — don't allocate from `MAX(id)` (uncommitted inserts →
  double-assign).
- **Mid-scan partial aggregation has four easy-to-break rules** (else ships wrong sizes) — DETAILS § "Key decisions".
- **The index is a disposable cache**: a schema mismatch or corruption deletes and rebuilds the DB file (no migrations;
  SQLite schema invariants in [`store/CLAUDE.md`](store/CLAUDE.md)). Gate only `scan_completed_at` writes (absence ⇒ heal
  to rescan).
- **Defer `root` auto-start until FDA is decided** (`should_auto_start_indexing`): scanning from `/` stacks TCC popups.
  FDA gates ONLY `root` — don't route SMB/MTP through it.

SMB/MTP indexing:

- **Gated on a `direct` (smb2) connection; an `os_mount` upgrades first** (`start_indexing_for_smb` refuses with a TYPED
  `SmbIndexGateReason`); MTP has no smb2 gate.
- **Manual rescan routes by TYPED kind:** `force_scan` → `start_volume_scan` for SMB/MTP, `start_scan` for `Local`. ❌
  Never `start_scan` a trait-scanned volume (jwalk over a network mount walks nothing and falsely completes — the "rescan
  does nothing to the NAS" bug). LOCAL `start_scan` reconciles a populated index in place, truncate+jwalks a fresh one.
  DETAILS § "LOCAL full rescan reconciles in place".
- **Never write `scan_completed_at` for an empty root.** A root with zero children returns typed `EmptyRoot`, not `Ok`;
  the local reconcile bails before diffing the root, so an empty `/` can't blank the index. DETAILS § "No completion
  marker on an empty root".
- **Freshness has ONE transition table (`freshness.rs`); don't branch elsewhere.** No journal ⇒ loads **Stale** on
  launch. Manager fires via `apply_freshness_event_on` (no registry re-lock), not `apply_freshness_event`.
- **Live watch runs with NO pane open** (`apply_smb_change` hooks `notify_directory_changed` before the pane
  early-return; don't remove).
- **Deletes resolve against the INDEX**: delete only a known entry; an unknown name/handle is a no-op. Local FSEvents
  `item_removed` stat-verifies.
- **Threads + resources.** One GLOBAL 16 GB memory watchdog (`stop_all_indexing`; idempotent). Wrap ObjC/Cocoa threads
  in `objc2::rc::autoreleasepool` (else multi-GB leaks). Use `tauri::async_runtime::spawn`; `tokio::spawn` panics from
  the sync `setup()` hook.

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
