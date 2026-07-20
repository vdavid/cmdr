# Drive indexing module

Background-indexes each volume (local, SMB, MTP) into its own SQLite DB with recursive size aggregates.

## Module map

Subdirs `scanner/`, `writer/`, `aggregator/`, [`store/`](store/CLAUDE.md); top-level `state.rs` + `manager.rs`
(lifecycle), `local_reconcile.rs` / `volume_scanner.rs` (LOCAL / SMB-MTP scan), `reconciler.rs` + `event_loop.rs`
(live), `enrichment.rs`, `freshness.rs`; IPC `commands/indexing.rs`, FE `src/lib/indexing/`, search `src/search/`.
Per-file grouping: DETAILS § "Module structure".

## Must-knows

All invariants hold PER volume id.

- **`INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`) is the authority**: absent key = disabled. Guards
  lifecycle ONLY; reads route through the per-volume `ReadPool`, never under the lock.
- **Phase transitions go through `events::set_phase_for(...)`**, never `DEBUG_STATS.set_phase`.
- **The index is a disposable cache**: schema mismatch or corruption deletes and rebuilds the DB (no migrations); gate
  only `scan_completed_at`.
- **Defer `root` auto-start** (`should_auto_start_indexing`): scanning `/` stacks TCC popups; FDA gates ONLY `root`.
- **A GLOBAL 16 GB memory watchdog stops ALL indexing.** Scans spawn via `tauri::async_runtime::spawn` (`tokio::spawn`
  panics in `setup()`).

Writer discipline (one thread per DB):

- **`start_indexing` is lock-first**: reserve the registry slot before building `IndexManager` (else two starts race).
  **Never hold `INDEX_REGISTRY` across a blocking/re-entrant manager call** (froze the UI once).
- **Reconciler/event loops hold a READ connection, never a write one** (`SQLITE_BUSY` kills live indexing).
- **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter**; never allocate from `MAX(id)`. A drifted counter
  self-heals (PK 1555 → resync + one retry); never extend that to UNIQUE 2067: a retried name conflict IS the duplicate
  row. Live file upserts throttle 60 s (`reconciler/throttle.rs`); `pending` is never evictable.
- **A fatal storage error STOPS + FAILS the index, never retries** (one incident logged 12,700 warnings in 8 min);
  recovery is rebuild; BUSY/LOCKED stay retried.
- **`MustScanSubDirs` is depth-split** (`reconciler/rescan_route.rs`): SHALLOW (`depth ≤ 2`) → VISIBLE scanner
  (`start_scan`, 45 s cooldown), no hourglass hold; DEEP → per-subtree-throttled reconcile drain.
- **The watcher→loop channel is UNBOUNDED**: backpressure dropped FSEvents and forced full scans. Don't re-bound it;
  `classify_ingestion_pressure` caps memory.

**The `dir_stats` ledger, three hard rules** (DETAILS § "The dir_stats ledger"):

- **Never clamp the arithmetic**: a negative delta is drift; escalate to `repair_dir_stats_upward`, never `.max(0)`
  (floored a real 1.21 GB to "0 bytes").
- **A failed `dir_stats` read or write is drift, not a no-op**: never warn-and-continue, never treat a read `Err` as "no
  row". Queue the id (`writer/deferred_repair.rs`).
- **Structural rewrites repair ancestors ON THE WRITER**, never off-writer read-then-credit; full-aggregate senders
  declare `source: Maps|Sql` (`Maps` only for a fresh scan), and never clear the accumulator in the subtree handler.

Coverage epochs and verification cost:

- **Never write `listed_epoch = 0` for a directory we DID list but chose to skip.** `0` absorbs up the whole chain: one
  skipped dir renders `~` incomplete. Post-replay verification declines oversized dirs, epoch untouched.
- **`verify_affected_dirs` guards BOTH phases**: a `LIMIT`-probe before the DB snapshot, and a `read_dir` ITERATION cap
  (not an upsert cap: known children are skipped first).

SMB/MTP + external-drive indexing:

- **Gated on a `direct` (smb2) connection; an `os_mount` upgrades first** (typed `SmbIndexGateReason`). Reconnect
  AUTO-RESUMES only when a scan completed AND `user_disabled` isn't set. **Manual rescan routes by TYPED kind**
  (`force_scan`): SMB/MTP → `start_volume_scan`, local → `start_scan`; never `start_scan` a trait-scanned volume (it
  false-completes).
- **Never write `scan_completed_at` for an empty root** (`EmptyRoot`) or an unlistable one (`RootUnlistable`).
- **The local walker gives up on a subtree after 32 consecutive failed reads**; descendants stay honest-stale
  (`listed_epoch=0`).
- **`should_exclude(path, &ExclusionScope)` derives scope from the volume KIND, never `is_volume_root`.** The scope
  carries the volume ROOT, so `<volume root>/{proc,sys,dev}` is skipped on EVERY volume (a phone's `proc` tree was 35%
  of one reconcile walk) while a deeper `~/projects/x/proc` stays indexed: key on root POSITION, never on the name.
  A File Provider domain root (`com.apple.file-provider-domain-id` xattr) counts as a volume root; that detection is an
  OPTIMIZATION, never a guarantee, so never let it stand in for the cost-budget backstop.
- **The LOCAL pipeline is mount-relative via `IndexPathSpace`**: strip the mount root ONLY at `resolve_abs`; path sets +
  FE emit stay ABSOLUTE.
- **Live watch runs with NO pane open** (`apply_smb_change` hooks before the pane early-return). **Freshness has ONE
  transition table (`freshness.rs`)**; no journal ⇒ loads **Stale**. **Deletes resolve against the INDEX** (unknown =
  no-op).
- **FAT/exFAT (`LocalExternal`) store `inode: None`** (`trust_inode`): a reused derived inode false-matches the rename
  pre-pass and corrupts `dir_stats`.
- **FSKit panic (2026-07-15): stop a `LocalExternal` index BEFORE its volume unmounts**; test with synthetic disk images
  ONLY.

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md); read it before non-trivial work here.
