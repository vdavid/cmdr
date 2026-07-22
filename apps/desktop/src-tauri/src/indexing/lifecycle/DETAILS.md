# Indexing lifecycle details

Read this before any non-trivial work in `indexing/lifecycle/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in [CLAUDE.md](CLAUDE.md).

This area was generalized from one hardwired volume to a registry keyed by `VolumeId`, so multiple volumes index
concurrently without corrupting each other. Every invariant below holds independently per key.

## Module structure

- **state.rs** (+ `state/tests.rs`) — the lifecycle/registry CORE: the `IndexPhase` enum, the per-volume
  `INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`), `IndexVolumeKind`, the phase transitions, the registry
  helpers, and the `IndexManager` + `ReadPool` bootstrap. Public lifecycle API (all take a `volume_id`):
  `start_indexing()` → `start_indexing_for(app, "root", "/")`, `stop_indexing`, `clear_index`, `force_scan`, `stop_scan`,
  `is_active`, `trigger_verification`, plus `init()`, `should_auto_start_indexing()`, and `stop_all_indexing` (the memory
  watchdog's target). The path→volume routing and the read-only query surface moved OUT to `../paths` and `../read`.
- **manager.rs** — `IndexManager`, the central per-volume coordinator, plus the LOCAL scan path and the shared dispatch.
  Owns the SQLite store (reads), the writer thread (writes), the scanner handle, and the FSEvents watcher. `resume_or_scan`
  / `force_rescan` dispatch by TYPED `IndexVolumeKind`: a trait-scanned (SMB/MTP) volume routes to `network_scan.rs`,
  `Local`/`LocalExternal` to `start_scan` here. `start_scan` dispatches the guarded-walker scanner (fresh) or the
  reconcile-in-place path (populated), and spawns the shared `ScanProgressReporter` (owned by `../events`).
- **network_scan.rs** — the SMB/MTP `Volume`-trait scan path, split out as a sibling `impl IndexManager` block. Holds
  `resume_or_scan_network` (a completed prior scan loads Stale and does NOT auto-rescan; a never-completed one scans) and
  `start_volume_scan` (the scan/rescan entry plus its bespoke completion handler). Mirrors `start_scan` but walks via the
  `../network_scanner` trait BFS, starts NO `DriveWatcher` (the live-watch layer owns that), and fires freshness through
  the manager's own `freshness` `Arc` (no registry re-lock).
- **scan_completion.rs** — the post-scan handler: the vanished-volume abort and the LOCAL failure→Stale arm (below).
- **freshness.rs** — the `Fresh`/`Stale`/`Scanning`/`Failed` transition table (`Freshness::on`) + `initial_freshness_on_launch`.
- **failure.rs** — `IndexFailureSignal`, the one-shot per-volume fatal-storage-error signal.
- **lifecycle_bus.rs** — the neutral scan-completed / registration / dirs-changed pub/sub.

## The per-volume registry

```
INDEX_REGISTRY: Mutex<HashMap<VolumeId, IndexInstance { phase, kind, read_pool, pending_sizes, freshness }>>
```

`IndexInstance` bundles everything one volume's index owns: its lifecycle `phase` (`IndexPhase`), its `ReadPool`
(lock-free enrichment/verification reads), its `PendingSizes` (the "size updating" hourglass), and its freshness `Arc`.
The registry is the single authority for WHICH volumes are indexed and for each volume's lifecycle. Every invariant the
single-volume design held now holds per volume id, keyed independently so two volumes can't corrupt each other:
single-writer-per-DB, lock-first reservation, drop-guard-before-drain, reads-via-`ReadPool`-never-under-the-lifecycle-lock.

**Disabled is the absence of a key.** There is no `IndexPhase::Disabled`. An `IndexInstance` only ever exists in
`Initializing` / `Running` / `ShuttingDown` / `Failed`; a stopped or never-started volume has no entry. `get_status`/
`is_active` treat an absent key as disabled, and `stop_indexing`/`clear_index` `remove()` the instance after the drain.
This is why IPC `get_index_status` for a stopped volume returns the same "not initialized" response a never-started one
does.

**Why a registry of bundled instances** (vs. three parallel `HashMap`s or a `DashMap`): bundling `{phase, read_pool,
pending_sizes, freshness}` in one struct keyed by volume id means a volume's lifecycle phase and its read handles are
taken/dropped together — no window where the phase says "Running" but the pool is gone. One `Mutex<HashMap>` (not
`DashMap`) keeps the lock-discipline reasoning identical to the old single-`Mutex` model: the lock guards lifecycle
transitions only, never reads.

**Root's read-path handles are special-cased to module globals.** Root's `ReadPool` lives in the `READ_POOL` global and
its `PendingSizes` in `PENDING_SIZES`; the root `IndexInstance` holds the SAME `Arc`s (one allocation, no drift). Two
reasons: the search module (local-disk-only) reads `get_read_pool()` on its hot path and shouldn't take the registry
lock, and the indexing tests install `READ_POOL`/`PENDING_SIZES` directly. Non-root volumes' handles live only in their
instance; `get_read_pool_for(vid)` / `get_pending_sizes_for(vid)` route root→global, non-root→`state::get_instance_*`.

The read-routing "skip if no index registered" gate (enrichment early-returns when `get_read_pool_for(vid)` is `None`)
lives with enrichment in [`../read`](../read/CLAUDE.md); the path→volume resolution (`volume_id_for_local_path`) lives in
[`../paths`](../paths/CLAUDE.md). Both consume the registry but aren't owned here.

## The `IndexPhase` machine (and where the pipeline-phase EVENT lives)

`IndexPhase` (state.rs) is the LIFECYCLE state: `Initializing { store }` → `Running` → `ShuttingDown` (transient) →
absent, plus the terminal `Failed { reason, db_path }`. This is distinct from the pipeline-phase (`ActivityPhase`:
Replaying/Scanning/Aggregating/Reconciling/Live/Idle) that drives the FE step checklist — that lives in
[`../events`](../events/CLAUDE.md) as the `index-phase-changed` event. Fire every pipeline-phase transition through
`events::set_phase_for(app, volume_id, phase, trigger)` (it does the global debug ring AND the per-volume emit in one
call so they can't drift); the lifecycle-phase transitions here are the `IndexPhase` swaps under the registry lock.

## Capability axes (`IndexVolumeKind`)

`IndexVolumeKind` has four variants (`Local`, `LocalExternal`, `Smb`, `Mtp`) and four orthogonal capability methods — the
canonical per-kind table lives on the enum's doc comment, so branch on the axis, not the variant:

- `uses_local_scanner()` — the guarded walker + FSEvents pipeline (`Local`, `LocalExternal`) vs the `Volume`-trait
  scanner. Exact complement of `is_trait_scanned()` (`Smb`, `Mtp`); a partition test in `state.rs` pins that they never
  drift, so a fifth variant must pick a side.
- `has_event_journal()` — self-heals watch continuity via FSEvents replay on launch. Only `Local` (the boot disk). Feeds
  `initial_freshness_on_launch`; a non-journaled kind loads Stale. This — NOT `last_event_id.is_some()` — gates journal
  replay: the shared local event loop persists `last_event_id` for any local-scanner volume, so a completed
  `LocalExternal` index carries one despite having no journal to replay.
- `mount_rooted()` — the index `ROOT_ID` is the mount (`/Volumes/X`), not `/`. True for every kind but `Local`.
- `feeds_search()` — the single volume whose writes back the in-memory search index. Only `Local`.

## Lock discipline (the load-bearing decisions)

**Lock-first `start_indexing`.** `start_indexing_for(app, volume_id, root)` opens a temporary `IndexStore` plus the
volume's `ReadPool`/`PendingSizes`, then atomically claims the `(absent) → Initializing(store)` transition via
`try_reserve_initializing_phase(volume_id, store, pool, pending)` BEFORE constructing the heavy `IndexManager` (which
spawns the writer thread). The reservation rejects when the volume already has ANY instance, so a second start for the
SAME volume no-ops; different volumes reserve independently. Without the lock-first claim, two near-simultaneous calls
for one volume can both spawn writer threads — each with its own `Arc<AtomicI64>` ID counter and `AccumulatorMaps` —
racing on the same DB (one of the mechanisms behind a historical ghost-size bug; the other, two writers racing, is closed
by this guard, with `UNIQUE (parent_id, name_folded)` as the safety net). The reservation also installs the volume's
read-path handles so enrichment works during `Initializing`.

**Drop the registry guard before the shutdown drain.** `stop_indexing(vid)` and `clear_index(vid)` swap the volume's
phase to `ShuttingDown` under the registry lock (taking the `IndexManager` out by value), then RELEASE the lock before
`mgr.shutdown()`. `shutdown()` blocks up to 5 s draining the live-event task. Holding `INDEX_REGISTRY` across that drain
would stall every concurrent `get_status`/`is_active`/`trigger_verification` caller — for ANY volume — for the whole
window and park a tokio worker, violating "reads never contend on the lifecycle lock." Dropping the guard mid-shutdown is
safe because the live loop reads via `ReadPool` and never reacquires the registry lock; concurrent callers observe the
published `ShuttingDown` phase (reported as not-initialized). After the drain, both re-lock only to `remove()` the
instance. Don't fold the drain back under a single held guard.

**Drop the registry guard before the blocking scan-start, too.** `force_scan(vid)` and the journal-gap fallback task
take the `Running` manager OUT of the registry under the lock (swapping in a transient `ShuttingDown`), RELEASE the
guard, run `mgr.force_rescan(...)` / `mgr.start_scan(...)`, then re-lock only to restore the manager as `Running`.
`start_scan`'s prelude does blocking I/O (`block_in_place(flush_blocking())` plus a `get_space_info_for_path` query) AND
fires the scan-start freshness transition. Held under the global registry lock, that prelude froze every concurrent
registry user, and the freshness firing re-locked the registry → an outright self-deadlock that froze the whole UI on
real hardware (QA). The fix is two-pronged and both halves are load-bearing: (1) the freshness firing goes through the
manager's own freshness `Arc` (no registry re-lock); (2) `force_scan`/fallback drop the guard before the blocking
prelude. Regression-guarded by `state::tests::scan_start_freshness_firing_does_not_relock_the_registry` (a
watchdog-timeout test: fire scan-start while holding the registry lock; pre-fix it deadlocks and the watchdog trips).

**A manual rescan routes by the TYPED volume kind.** `state::force_scan(vid)` calls `mgr.force_rescan(...)`, NOT
`mgr.start_scan(...)`. `force_rescan` dispatches on `rescan_scanner_for_kind(self.kind)`: a trait-scanned kind (SMB/MTP)
runs `start_volume_scan` (the trait walk from the share/storage root), a local-scanner kind runs `start_scan`. Pre-fix,
`force_scan` called `start_scan` unconditionally, so "Rescan now" on a NAS ran the LOCAL scanner over the SMB mount —
walked nothing in ~2 ms, wrote `volume_path=/` (a local-scanner-only marker), and falsely marked the index complete with
`total_entries=0`. `rescan_scanner_for_kind` is a separate pure function (unit-testable without an `AppHandle`),
regression-locked by `manager::tests::force_rescan_routes_smb_and_mtp_to_the_trait_scanner_not_the_local_walker`.
Classify by the typed `kind`, never a volume-id substring.

## Freshness (`freshness.rs`) — the state machine and the seam

Local disk gets freshness free from FSEvents' journal (replay from `last_event_id` → Fresh on launch). SMB/MTP/external
have NO journal — events arrive only while connected and watching, and any gap loses them irrecoverably — so freshness
is binary: continuously-watched-since-scan ⇒ Fresh, any break ⇒ Stale. UI colors: **gray** = no registered instance
(the "disabled = no key" model, NOT a `Freshness` variant); **blue** = `Scanning`; **green** = `Fresh`; **yellow** =
`Stale`; **red** = `Failed`.

`Freshness::on(event)` is the single, total transition table (pure, exhaustively tested in `freshness::tests`). It lives
on the `IndexInstance` as `Arc<Mutex<Option<Freshness>>>` so scan-transition tasks and the watcher layer can flip it
without the registry lock. Two entry points thread an event through it, and which one a caller uses is a LOCK-DISCIPLINE
decision:

- `state::apply_freshness_event_on(freshness_arc, vid, event)` — the real transition + FE emit. Operates on the `Arc`
  DIRECTLY, NEVER locks `INDEX_REGISTRY`. `IndexManager` holds a clone of its volume's freshness `Arc` and fires ALL its
  scan transitions through this (including from spawned completion tasks via a cloned handle). That's what lets a caller
  holding the registry lock across `start_scan` fire scan-start WITHOUT re-entering the non-recursive registry mutex.
- `state::apply_freshness_event(vid, event)` — looks the instance's freshness `Arc` up UNDER the registry lock, clones
  it, drops the lock, then delegates. For EXTERNAL callers that only have a volume id and are NOT under the registry
  lock: the live-watch layer (`../transports`) firing `WatcherDied` / `OverflowUnrecoverable`.

Load-bearing rules:

- **Load-as-Stale on launch.** `initial_freshness_on_launch(scan_completed_at_present, journaled)`: a completed-but-
  non-journaled index (SMB/MTP/external) loads **Stale**, a journaled one (local) loads **Fresh**, no-completed-scan
  loads `None` (gray → fresh scan). Seeded at reservation from the volume `kind`. This is correct and honest, not a bug:
  we weren't watching while off.
- **Scan transitions.** `ScanStarted` ⇒ Scanning; a CLEAN `ScanCompleted` ⇒ Fresh (gated on `!was_cancelled`); a FAILED
  LOCAL scan/reconcile ⇒ `ScanFailed` ⇒ Stale.
- **Failed LOCAL scan ⇒ Stale, never a stuck spinner** (`scan_completion.rs`). `start_scan`'s completion handler fires
  `ScanFailed` (through the cloned freshness handle, no registry re-lock) on both failure arms: `Ok(Err(_))` (a typed
  `ScanError` like `EmptyRoot`, or a `catch_unwind`-converted `Panicked`) and `Err(_)` (thread-join panic). `ScanStarted`
  already moved the badge to Scanning, so without this a failed scan strands it on a perpetual blue spinner until
  relaunch. The prior index is NOT blanked; it gets the honest Stale "rescan available" badge and heals on rescan.
- **Interrupted SMB/MTP scan: disconnect ⇒ keep an honest partial + Stale; user-cancel ⇒ heal-to-rescan (gray).** The
  completion handler in `start_volume_scan` splits on `match result` (NOT a freshness-enum change — one transition table,
  the handler just chooses WHICH event to apply):
  - **Disconnect** (the typed `DeviceDisconnected`, or the consecutive-failure backstop — both classified by
    `VolumeScanError::is_terminal_disconnect`, by TYPED variant, never a substring): KEEP the instance + DB, leave
    `scan_completed_at` UNwritten, `bump_current_epoch_for` (the continuity break that makes the kept rows stale), apply
    `WatcherDied` ⇒ Stale, and `discard_buffered_changes`. The network scanner already ran its partial-preserving write
    sequence (flush + `MarkDirsListed` + `ComputeAllAggregates`, NO `scan_completed_at`) before returning the typed
    error, so scanned subtrees roll up exact-but-stale and unscanned ones stay `0` (`—`/`≥`). Net: a navigable honest
    partial, Stale, not gray, not a lie. This is the fix for the reported prod bug (the old code churned every still-
    queued dir into a silently-empty row, then wrote `scan_completed_at` and rendered "complete + Fresh"). It persists
    across relaunch because `resume_or_scan_network` sees no `scan_completed_at` and RECONCILES (not truncates) the
    existing rows.
  - **User cancel** (`Ok(summary)` with `was_cancelled`): the partial is discardable — `discard_buffered_changes` +
    `state::reset_to_not_indexed` ⇒ gray, healing to a clean fresh scan on the next enable. (Timeout / writer-send /
    non-disconnect root-fatal also take this discard path.)
- **The watcher-driven transitions** (`WatcherDied`, `OverflowUnrecoverable`) fire from the transport live-watch layer
  ([`../transports`](../transports/CLAUDE.md)); this area owns only the transition table they feed.

## The Failed state (fatal storage failure) — stop loudly, don't retry forever

A real incident: the local index DB began returning `SQLITE_IOERR` on every read and write mid-scan. The writer thread
and the reconciler each just `log::warn!`-and-continued and retried FOREVER: 12,700+ identical warnings over 8 minutes,
~190% CPU, a frozen webview, and "Find files" stuck at 0%. The fix makes a dead index DB fail loudly, stop cleanly, and
show an honest state.

**Classification is typed, never on the message string** (`no-string-matching`). `store::IndexStoreError::sqlite_code()`
extracts `(rusqlite::ErrorCode, extended_code)`; `is_fatal_storage_error()` is `true` for the storage-death classes
(`SQLITE_IOERR*`, `SQLITE_CORRUPT`, `SQLITE_CANTOPEN`, `SQLITE_FULL`, `SQLITE_READONLY`, `SQLITE_NOTADB`). Transient
contention (`SQLITE_BUSY`/`SQLITE_LOCKED`) is deliberately NOT fatal (the busy handler backs those off). The detector
lives in the writer ([`../writer`](../writer/CLAUDE.md)); this area owns the LIFECYCLE representation of the trip.

**Detection lives in the writer, the signal is `failure.rs::IndexFailureSignal`.** A one-shot per-volume
`Arc<IndexFailureSignal>` created in `IndexWriter::spawn_for`, cloned into the writer thread and exposed via
`IndexWriter::failure_signal()`. `note(&err, ctx)` classifies + trips once: a non-fatal error logs at warn as before
(returns `false`); the FIRST fatal error CAS-trips the signal, records the reason, logs ONCE at error level, and wakes
the supervisor (later fatal errors suppressed — that's what stops the 12,700-line flood). `writer_loop` checks
`is_tripped()` after each message and returns.

**The representation choice (why `Failed` lives in BOTH the lifecycle phase and freshness).** A dead index must be
DISTINCT from "absent = disabled" so the badge is honest, yet its writer/watcher must be torn down. So:

- `IndexPhase::Failed { reason: IndexFailure, db_path }`: the instance STAYS registered (discoverable for the badge +
  recovery) but carries no live manager. `get_status`/`get_debug_status` treat it like disabled; `is_active` is `false`;
  `get_instance_read_pool`/`get_instance_pending_sizes` return `None`, so reads SKIP cleanly (no per-navigation flood on
  a dead DB). The stored `db_path` lets `clear_index` reclaim the file.
- `Freshness::Failed` (red): drives the badge through the SAME `index-freshness-changed` event the other colors use. It
  is TERMINAL in `Freshness::on` (only `ScanStarted` leaves it), so a concurrent scan-completion unwinding as the index
  is torn down can't downgrade a dead index back to Stale/Fresh.

**The supervisor (`state::spawn_failure_supervisor` → `fail_index`).** Spawned once when the volume becomes `Running`
(the signal is one-shot and `notified()` resolves even if the trip already happened, so a failure in the
Initializing→Running window is never missed). On the trip it runs `fail_index`: uninstall + invalidate the read-path
handles, take the manager OUT of the registry under the lock (publishing a transient `ShuttingDown`), DROP the lock,
`mgr.shutdown()`, re-lock and install `IndexPhase::Failed`, then fire `set_phase_for(Failed)` + `apply_freshness_event(
StorageFailed)`. Same drop-the-guard-before-the-drain discipline as `stop_indexing`. A no-op if the volume isn't
`Running`.

**Recovery is rebuild-from-scratch** (the index is a disposable cache). A `Failed` volume can't resume in place — its
manager/writer are gone and the instance still holds the key, so a plain `start_indexing` would no-op. The
`enable_drive_index` funnel checks `indexing::is_failed` and, if so, `clear_index`es the dead instance + DB FIRST, then
falls through to a fresh start. `clear_index`/`stop_indexing` each grew a `Failed` arm (remove the instance, no drain;
`clear` also deletes the DB via the stored `db_path`). The FE maps `Failed` → `['rescan', 'forget']`.

**Scope / known limit.** The writer is the authoritative detector. `run_live_event_loop` polls
`writer.failure_signal().is_tripped()` at each flush tick and breaks, and the supervisor tears the watcher down, so the
reconciler's failing-resolve churn is bounded to at most one batch after the trip. A pure read-only flood (the event
loop's `resolve_path` failing fatally while the writer never writes) is still not independently detected — in practice
live processing always writes, so the writer trips.

## Vanished-volume scan abort (`scan_completion.rs`)

A drive yanked mid-scan makes the local scan's ROOT unlistable: the fresh guarded-walker scan detects `dirs_read == 0`
on a volume-root scan, and the reconcile walk hits `reader.read(root) == None` — both return the typed
`ScanError::RootUnlistable`, distinct from `ScanError::EmptyRoot` (a readable-but-empty root, e.g. a blank USB stick,
which legitimately completes). The completion handler fires `ScanFailed` ⇒ Stale for every failure and, ONLY for
`RootUnlistable`, additionally goes `Idle` and emits `IndexScanAbortedEvent { volume_id }` — clearing the frontend's
stuck "scanning" row, mirroring the network path's disconnect arm. No `scan_completed_at` is written, so the index heals
to a rescan on remount. `scan_failure_is_vanished_volume` is the pure distinguisher; an empty root or a walk panic does
NOT abort. (The wedge-safe unmount/eject ORDERING that stops the index before the FS goes away lives in
[`../transports`](../transports/CLAUDE.md).)

## The neutral lifecycle bus (`lifecycle_bus.rs`) — single source

A minimal in-process pub/sub so a backend subsystem (the importance scheduler; later the media-ML enrichment scheduler)
learns when a volume finished scanning, WITHOUT `indexing/` depending on it (the one-way `consumer → indexing`
direction). This is the single canonical home for the mechanism; consumer docs point here.

- **Published from the neutral chokepoint.** `apply_freshness_event_on` calls `lifecycle_bus::publish_scan_completed(vid)`
  on a `ScanCompleted`, alongside the FE `.emit`. Both the LOCAL and network completion paths funnel through this seam.
  It publishes on the EVENT, not on a freshness CHANGE: a Fresh→Fresh rescan completion still means new data to rescore.
- **`tokio::sync::watch`, NOT `broadcast`.** A `broadcast` doesn't replay a value sent before a receiver subscribes, so a
  `ScanCompleted` fired during `setup()` before the scheduler subscribes would be lost. A `watch` retains the last value.
  The publish uses `send_replace` (not `send`), which updates the retained value even with zero receivers.
- **Senders live in a module map, not `IndexInstance`.** A `watch::Sender` per volume id in a process-global `BUS`,
  created lazily. Keeping it OUT of the instance is deliberate: the sender must outlive the instance so a subscriber that
  took its receiver keeps seeing the last state after the volume unmounts. `ScanState` carries a monotonic `generation`
  so a consumer can coalesce a repeat.
- **The startup sweep is the bus's companion, not part of it.** A volume already Fresh at launch never re-fires
  `ScanCompleted`, so `state::ready_volumes_with_kind()` snapshots the volumes that are `Fresh` right now (with each
  volume's typed `IndexVolumeKind`) for the scheduler to enqueue once at startup.
- **A registration `broadcast`** (`publish_volume_registered` / `subscribe_registrations`) carries late-registering
  volumes (a share mounted AFTER startup), published from `start_indexing_for` right after a volume wins its
  `Initializing` reservation, carrying the id AND its typed kind. A lagged receiver only misses a registration the next
  `ScanCompleted` still covers, so a miss self-heals.
- **A `dir-changed` channel** (`publish_dirs_changed` / `subscribe_dirs_changed`, a per-volume `watch<DirsChanged>` in a
  separate `DIR_BUS` map) carries live listing changes from the live event loop and the per-navigation verifier — the
  importance scheduler's incremental-recompute trigger. Being a `watch`, a burst can drop an intermediate batch;
  accepted, because the next full recompute heals it.

See `importance/DETAILS.md` for how the scheduler combines the sweep + the bus with per-volume coalescing and per-kind
policy.

## The IPC surface (resolved here, commands elsewhere)

The per-drive freshness UX drives any drive through three thin `commands/indexing.rs` commands: `enable_drive_index`,
`disable_drive_index`, `rescan_drive_index`. For root they map to `start_indexing`/`stop_indexing`/`force_scan`; SMB/MTP/
local-external routing lives in [`../transports`](../transports/CLAUDE.md). `enable`/`rescan` return
`EnableIndexingOutcome` (`{ status: "started" }` or, for SMB, `{ status: "refused", reason: SmbIndexGateReason }`). The
per-volume status IPC (`get_volume_index_status(path)` for the active-drive badge, `get_volume_index_status_by_id` for
the dropdown rows) builds `VolumeIndexStatus { volume_id, enabled, freshness, scan_completed_at, scan_duration_ms,
coalesced_signals_since_sweep, next_sweep_due_at }`: freshness from the registry, the scan facts from the persisted
`meta`. `enabled: false` + `freshness: None` is gray. The path→volume resolution feeding these lives in
[`../paths`](../paths/CLAUDE.md).

## FDA-deferred root auto-start

At first launch on macOS, recursively scanning from `/` opens iCloud Drive, Photos, and other TCC-protected directories,
which makes macOS stack native permission popups on top of the in-app FDA modal (we hit 5-10 once).
`should_auto_start_indexing(indexing_enabled, fda_choice, os_fda_granted)` gates the launch-time start via
`crate::fda_gate::is_fda_pending`: skip when `fda_choice == NotAskedYet` AND `os_fda_granted == false`. Once the user
picks Allow (restart) or Deny (same session, via `start_indexing_after_fda_decision`), the indexer starts.
`os_fda_granted == true` overrides `NotAskedYet`. FDA gates ONLY `root` — SMB/MTP/external paths aren't TCC-protected, so
`start_indexing_for_smb` and the MTP/local-external enables never route through this gate. After Deny the indexer runs
in degraded mode (one TCC prompt per protected folder, the contract the user opted into). Launch-time NSWorkspace icon
fetches in `volumes::list_locations` share the same `is_fda_pending` predicate so the two gate sites can't drift.

## Testing

The registry / phase / freshness state-machine tests and their serialize-on-a-dedicated-mutex discipline live in
[`../tests`](../tests/CLAUDE.md) (`integration_tests.rs`, the stress suites) and colocated `state/tests.rs` +
`freshness.rs`/`failure.rs`/`manager.rs` unit tests. Key regression anchors named above:
`scan_start_freshness_firing_does_not_relock_the_registry`,
`force_rescan_routes_smb_and_mtp_to_the_trait_scanner_not_the_local_walker`,
`forget_stale_index_transitions_to_gray_and_deletes_db`, and the two disconnect-storm tests.
