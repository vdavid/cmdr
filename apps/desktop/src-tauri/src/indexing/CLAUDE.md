# Drive indexing module

Background-indexes each volume (local, SMB, MTP) into its own SQLite DB with recursive size aggregates.

## Module map

Subdirs `scanner/`, `writer/`, `aggregator/`, [`store/`](store/CLAUDE.md); top-level `state.rs` + `manager.rs`
(lifecycle), `local_reconcile.rs` / `volume_scanner.rs` (LOCAL / SMB-MTP scan) + `scan_pace.rs` (yield to nav),
`reconciler.rs` + `event_loop.rs` (live), `enrichment.rs`, `freshness.rs`; IPC `commands/indexing.rs`, FE
`src/lib/indexing/`, search `src/search/`. Per-file grouping: DETAILS § "Module structure".

## Must-knows

All invariants hold PER volume id.

- **`INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`) is the authority**: absent key = disabled. Guards
  lifecycle ONLY; reads route through the per-volume `ReadPool`, never under the lock.
- **Phase transitions go through `events::set_phase_for(...)`**, never `DEBUG_STATS.set_phase`.
- **The index is a disposable cache**: schema mismatch or corruption deletes and rebuilds the DB (no migrations); gate
  on `scan_completed_at`.
- **Defer `root` auto-start** (`should_auto_start_indexing`): scanning `/` stacks TCC popups; FDA gates ONLY `root`.
- **A GLOBAL 16 GB memory watchdog stops ALL indexing.** Scans spawn via `tauri::async_runtime::spawn` (`tokio::spawn`
  panics in `setup()`).

Writer discipline (one thread per DB):

- **`start_indexing` is lock-first**: reserve the registry slot before building `IndexManager` (else two starts race).
  **Never hold `INDEX_REGISTRY` across a blocking/re-entrant manager call** (froze the UI once).
- **Reconciler/event loops hold a READ connection, never a write one** (`SQLITE_BUSY` kills live indexing).
- **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter**; never allocate from `MAX(id)`. A drifted counter
  self-heals (PK 1555 → resync + one retry); never extend to UNIQUE 2067: a retried name conflict IS the duplicate row.
  Live upserts throttle 60 s (`reconciler/throttle.rs`); `pending` is never evictable.
- **A fatal storage error STOPS + FAILS the index, never retries** (one incident logged 12,700 warnings in 8 min);
  recovery is rebuild. BUSY/LOCKED stay retried.
- **`MustScanSubDirs` is depth-split** (`reconciler/rescan_route.rs`): SHALLOW (`depth ≤ 2`) → VISIBLE scanner
  (`start_scan`), no hourglass hold; DEEP → throttled reconcile drain.
- **A shallow anchor sweeps at most ONCE A DAY, on the BOOT DISK ONLY** (`SHALLOW_RESCAN_MIN_INTERVAL`); a
  mount-rooted volume keeps the short `EXTERNAL_SHALLOW_RESCAN_MIN_INTERVAL`. Don't unify them: the storm was on `/`,
  and the per-navigation verifier is root-scoped, so an external drive has NO cover between sweeps.
- **Coalesced anchors are counted, not forgotten** (`SweepRecord.coalesced_since_sweep`, since the last COMPLETED
  sweep, never lifetime), surfaced on `VolumeIndexStatus`. The badge stays green by design: once-a-day sweeping is
  intended, not a fault.
- **The window is WALL-CLOCK and persisted**, seeded from `max(meta.shallow_sweep_at, meta.scan_completed_at)`. A
  TRIGGERED sweep stamps `shallow_sweep_at` immediately: `start_scan` DELETES `scan_completed_at` first, so keying off
  completion alone makes an interrupted sweep look never-swept and rescan every launch.
- **The watcher→loop channel is UNBOUNDED**: backpressure dropped FSEvents and forced full scans. Don't re-bound it;
  `classify_ingestion_pressure` caps memory.

**The `dir_stats` ledger, four hard rules** (DETAILS § "The dir_stats ledger"):

- **Never clamp the arithmetic**: a negative delta is drift; escalate to `repair_dir_stats_upward`, never `.max(0)`
  (floored a real 1.21 GB to "0 bytes").
- **A failed `dir_stats` read or write is drift, not a no-op**: never warn-and-continue, never treat a read `Err` as "no
  row". Queue the id (`writer/deferred_repair.rs`).
- **Structural rewrites repair ancestors ON THE WRITER**, never off-writer read-then-credit; full-aggregate senders
  declare `source: Maps|Sql` (`Maps` only for a fresh scan), never clearing the accumulator in the subtree handler.
- **Suppressing propagation is a DEBT, only ever inside `BulkReconcileGuard`**: it marks the ledger unpaid durably
  (`MarkLedgerUnpaid`) and pays on exit (`PayLedgerIfUnpaid`), so a walk that never reaches its terminal aggregate heals
  here or next launch. Bare `SetDeltaPropagation(false)` left 249 dirs claiming exact sizes.

Coverage epochs and verification cost:

- **Never write `listed_epoch = 0` for a directory we DID list but chose to skip.** `0` absorbs up the whole chain: one
  skipped dir renders `~` incomplete. Post-replay verification declines oversized dirs, epoch untouched.
- **The reconcile walk stops descending into a subtree whose reads are >5% SLOW** (≥10 of them, >5 s wasted, over a
  20 ms + 100 µs/entry allowance; anchors depth 5; `local_reconcile/cost_budget.rs`). ❌ Never a TOTAL: it scales with
  subtree size, so total-based rules refused a big healthy repo. A skip means "never listed": ❌ never hand the diff an
  empty listing (it reaps the subtree, stripping bytes from every ancestor) nor stamp an epoch.
- **`verify_affected_dirs` guards BOTH phases**: a `LIMIT`-probe before the DB snapshot, and a `read_dir` ITERATION cap
  (not an upsert cap: known children are skipped first).

SMB/MTP + external-drive indexing:

- **Gated on a `direct` (smb2) connection; an `os_mount` upgrades first** (typed `SmbIndexGateReason`). Reconnect
  AUTO-RESUMES only when a scan completed AND `user_disabled` isn't set. **Manual rescan routes by TYPED kind**
  (`force_scan`): SMB/MTP → `start_volume_scan`, local → `start_scan`; never `start_scan` a trait-scanned volume (it
  false-completes).
- **Never write `scan_completed_at` for an empty root** (`EmptyRoot`) or an unlistable one (`RootUnlistable`).
- **The local walker abandons a read that STOPPED PRODUCING (15 s, via `ReadProgress`), never a merely long one.**
  ❌ Never re-cap total duration: it can't tell a 200,000-entry dir from a dead mount (dropped 661,411 rows once).
  Subtree give-up after 32 consecutive failed reads; descendants stay honest-stale (`listed_epoch=0`).
- **`should_exclude(path, &ExclusionScope)` derives scope from the volume KIND, never `is_volume_root`.** The scope
  carries the volume ROOT, so `<volume root>/{proc,sys,dev}` is skipped on EVERY volume (a phone's `proc` was 35% of one
  reconcile walk) while `~/projects/x/proc` stays indexed. BOTH root POSITION and all three as siblings required — else
  a Dropbox `dev` silently vanishes. A File Provider domain root counts as one, but that's an OPTIMIZATION, never a
  substitute for the cost-budget backstop.
- **The walk's listing budget is PACED, not constant** (`scan_pace.rs`): browsing a share drops it 64 → 1, per volume,
  so nav isn't queued behind the scan. ❌ Never let it reach 0 — one-at-a-time is what makes progress structural.
- **The LOCAL pipeline is mount-relative via `IndexPathSpace`**: strip the mount root ONLY at `resolve_abs`; path sets +
  FE emit stay ABSOLUTE.
- **Live watch runs with NO pane open** (`apply_smb_change` hooks before the pane early-return). **Freshness has ONE
  transition table (`freshness.rs`)**; no journal ⇒ **Stale**. **Deletes resolve against the INDEX** (unknown = no-op).
- **FAT/exFAT (`LocalExternal`) store `inode: None`** (`trust_inode`): a reused derived inode false-matches the rename
  pre-pass and corrupts `dir_stats`.
- **FSKit panic (2026-07-15): stop a `LocalExternal` index BEFORE its volume unmounts**; test with synthetic disk
  images ONLY.

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md); read it before non-trivial work here.
