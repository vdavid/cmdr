# Drive indexing module

Background-indexes volumes into per-volume SQLite DBs with recursive size aggregates, so listings show directory sizes.
Local disk, SMB, and MTP each get their own DB.

## Module map

Per-file roles and the full grouping (lifecycle, SMB/MTP, freshness, support): DETAILS § "Module structure" or
`codegraph_search`. Write path: `scanner/` (jwalk fresh scan), `local_reconcile.rs` (LOCAL rescan), `volume_scanner.rs`
(SMB/MTP), `writer/`, `aggregator/`, `reconciler.rs` + `event_loop.rs`. Reads: `enrichment.rs` (`ReadPool`) + `store/`.
Lifecycle: `state.rs`, `manager.rs`, `freshness.rs`. IPC in `commands/indexing.rs`; frontend in `src/lib/indexing/`;
search in `src/search/`.

## Must-knows

All invariants hold PER volume id; DETAILS has the why.

- **`INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`) is the authority** (absent key = disabled, no
  `Disabled` phase). The mutex guards lifecycle ONLY; reads route through the per-volume `ReadPool`, never under it.
  Enrichment SKIPS when `get_read_pool_for` is `None`.
- **Phase transitions go through `events::set_phase_for(app, volume_id, phase, trigger)`, not raw
  `DEBUG_STATS.set_phase`.** It records the GLOBAL debug-window timeline AND emits the PER-volume `index-phase-changed`
  event (the FE step checklist) in one call, so the two can't drift. `DEBUG_STATS` is an app-wide singleton (can't
  attribute a phase to a drive under concurrent volumes); the event carries `volumeId`. Network (SMB/MTP) emits only
  `Scanning → Live` — drive the "compute folder sizes" step off aggregation events, not a phase network never sends.
  DETAILS § "Per-volume pipeline phase event".

Writer + schema discipline (one writer thread per DB, bounded `sync_channel`):

- **`start_indexing` is lock-first**: reserve the registry slot BEFORE building `IndexManager`, else two starts race
  writer threads on the same DB. **Never hold `INDEX_REGISTRY` across a blocking or re-entrant manager call** (froze
  the UI on real hardware): callers drop the guard before `shutdown()` / `start_scan`. DETAILS § "Drop the registry
  guard".
- **Reconciler/event loops hold a READ connection, never a write one** (`SQLITE_BUSY` silently kills live indexing).
  **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter** — don't allocate from `MAX(id)` (uncommitted inserts →
  double-assign).
- **Schema invariants.** Register `platform_case` collation on every connection (not persisted; else raw `sqlite3` CLI
  fails — use `index-query`). Don't drop `UNIQUE (parent_id, name_folded)` (multi-TB ghost-size hazard) nor
  `name_folded`. Scanner uses `INSERT OR IGNORE`, never `INSERT OR REPLACE` (reassigns IDs, orphans children).
- **Mid-scan partial aggregation has four easy-to-break rules** (else ships wrong sizes) — DETAILS § "Key decisions".
- **The index is a disposable cache**: schema mismatch / corruption → delete the DB file + recreate fresh (reclaims
  disk, no DROP-TABLE freelist); no migrations or user-facing DB errors. Gate only `scan_completed_at` writes
  (absence ⇒ heal to rescan).
- **Defer `root` auto-start until FDA is decided** (`should_auto_start_indexing`): scanning from `/` stacks TCC popups.
  FDA gates ONLY `root` — don't route SMB/MTP through it.

SMB/MTP indexing (read DETAILS before touching this area):

- **Gated on a `direct` (smb2) connection; an `os_mount` upgrades first** (`start_indexing_for_smb` refuses with a
  TYPED `SmbIndexGateReason`); MTP has no smb2 gate.
- **Manual rescan routes by TYPED kind:** `force_scan` → `start_volume_scan` for SMB/MTP, `start_scan` for `Local`. ❌
  Don't call `start_scan` for a trait-scanned volume — jwalk over a network mount walks nothing and falsely completes
  ("Rescan does nothing to the NAS" bug). LOCAL `start_scan` reconciles a populated index
  (`local_reconcile.rs`), truncate+jwalk only a fresh one — predicate `entry_count > 1` NOT `> 0` (the sentinel makes a
  fresh DB count 1). DETAILS § "LOCAL full rescan reconciles in place".
- **Never write `scan_completed_at` for an empty root.** ROOT with ZERO children returns a typed `EmptyRoot`, not `Ok`;
  the local reconcile bails BEFORE diffing the root so an empty `/` can't blank the index. DETAILS § "No completion
  marker on an empty root".
- **Freshness has ONE transition table (`freshness.rs`); don't branch elsewhere.** No journal ⇒ loads **Stale** on
  launch. Manager fires via `apply_freshness_event_on` (no registry re-lock), not `apply_freshness_event`.
- **Live watch runs with NO pane open** (`apply_smb_change` hooks `notify_directory_changed` before the pane
  early-return; don't remove).
- **Deletes resolve against the INDEX**: delete only a known entry; unknown name/handle is a no-op. Local FSEvents
  `item_removed` stat-verifies.
- **Threads + resources.** One GLOBAL 16 GB memory watchdog (`stop_all_indexing`; idempotent). Wrap ObjC/Cocoa threads
  in `objc2::rc::autoreleasepool` (else multi-GB leaks). Use `tauri::async_runtime::spawn`; `tokio::spawn` panics from
  the sync `setup()` hook.

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md). Read before non-trivial work here.
