# Indexing lifecycle details

Read this before any non-trivial work in `indexing/lifecycle/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in `CLAUDE.md`.

This area was generalized from one hardwired volume to a registry keyed by `VolumeId`, so multiple volumes index
concurrently without corrupting each other. Every invariant below holds independently per key.

## Module structure

- **state.rs** (+ the `state/` submodules) — the lifecycle/registry CORE: the `IndexPhase` enum, the per-volume
  `INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`), and the thin registry helpers that hand a `Running`
  manager's writer, cover context, or branch-coverage calls out. Everything that ACTS on the registry sits one file
  down, one job each, and `state.rs` re-exports all of it, so `state::<anything>` remains the only path any caller uses
  (`state/` is private):
  - `auto_start.rs` — the two pure launch-policy predicates (settings, plus the FDA gate).
  - `reservation.rs` — `try_reserve_initializing_phase` (the lock-first `(absent) -> Initializing` check-and-set that
    carries the one-writer-per-DB contract), `is_initializing_phase`, and the test-only reservation helper.
  - `startup.rs` — `start_indexing_for`, the choke point every transport funnels through, plus `Activation`,
    `start_indexing`, and the three per-transport `*_inner` entry points. `walk_database.rs` holds the two things only a
    `WriterOnly` start does: evict an index whose coverage this build refuses, and stamp/seed the empty one that
    replaces it.
  - `teardown.rs` — `stop_indexing`, `clear_index`, `clear_every_index`, `reset_to_not_indexed`,
    `disable_drive_index_persist_intent`, `remove_instance_and_handles`, and `stop_all_indexing`, all sharing the
    withdraw-then-publish-`ShuttingDown`-then-drop-the-guard-then-drain ordering.
  - `scan_control.rs` — `force_scan`, `stop_scan`, `trigger_verification`.
  - `queries.rs` — the read-only surface: `is_active`, `is_failed`, `index_failure`, `awaits_its_first_scan`,
    `ready_volumes_with_kind`, `all_registered_volume_ids`, `volume_kind`, `registered_mtp_volume_ids_for_device`.
  - `freshness_bridge.rs` — the registry ↔ `freshness.rs` wiring (`apply_freshness_event` vs `..._on`, which is LOCK
    DISCIPLINE, not style) plus `bump_current_epoch_for` / `get_freshness`.
  - `supervisor.rs` — `spawn_failure_supervisor` + `fail_index`, the `Failed`-phase transition (the signal itself is
    `failure.rs`).
  - `tests.rs` — the registry-level lifecycle tests.

  A volume's IDENTITY (`VolumeId`, `ROOT_VOLUME_ID`, `IndexVolumeKind`) is deliberately NOT here: it's the leaf
  `../volume.rs`, so path routing, the transports, and the bus can name a volume without importing the registry. Nothing
  below `lifecycle` imports `lifecycle::state` — a volume's read handles and its stop token are pushed DOWN to the code
  that needs them (both sections below), which is what keeps that statable. Public lifecycle API (all take a
  `volume_id`): `start_indexing()` → `start_indexing_for(app, "root", "/")`, `stop_indexing`, `clear_index`,
  `force_scan`, `stop_scan`, `is_active`, `trigger_verification`, plus `init()`, `should_auto_start_indexing()`, and
  `stop_all_indexing` (the memory watchdog's target). The path→volume routing and the read-only query surface moved OUT
  to `../paths` and `../read`.

- **manager.rs** — `IndexManager`, the central per-volume coordinator, plus the LOCAL scan path and the shared dispatch.
  Owns the SQLite store (reads), the writer thread (writes), the scanner handle, and the FSEvents watcher.
  `resume_or_scan` / `force_rescan` dispatch by TYPED `IndexVolumeKind`: a trait-scanned (SMB/MTP) volume routes to
  `network_scan.rs`, `Local`/`LocalExternal` to `start_scan` here. `start_scan` dispatches the guarded-walker scanner
  (fresh) or the reconcile-in-place path (populated), and spawns the shared `ScanProgressReporter` (owned by
  `../events`). Both scan-start funnels (here and `network_scan.rs`) classify the run with
  `events::ScanRunKind::classify` right after deciding reconcile-vs-truncate: the kind rides `index-scan-started` (what
  the FE states) and picks the calibration bucket this run reads its ETA seed from and writes its timing back into
  (`../store/DETAILS.md` § "Scan calibration is stored PER WALK KIND"). The stashed `ScanCalibration` also surfaces the
  kind on `get_status`, so a mid-scan window reload recovers it.
- **network_scan.rs** — the SMB/MTP `Volume`-trait scan path, split out as a sibling `impl IndexManager` block. Holds
  `resume_or_scan_network` (a completed prior scan loads Stale and does NOT auto-rescan; a never-completed one scans)
  and `start_volume_scan` (the scan/rescan entry plus its bespoke completion handler). Mirrors `start_scan` but walks
  via the `../network_scanner` trait BFS, starts NO `DriveWatcher` (the live-watch layer owns that), and fires freshness
  through the manager's own `freshness` `Arc` (no registry re-lock). `start_volume_scan` takes a `NetworkScanMode`:
  `Auto` picks reconcile-vs-truncate by what the DB holds, `Rebuild` forces the truncate. `resume_or_scan_network`
  carries the two jobs an index that loads Stale would otherwise never get: the one-shot `dir_stats` ledger heal (below,
  gated on its `meta` marker and needing a `ComputeAllAggregates` to follow), and the one-time `Rebuild` for an index
  built under an older NAS system-dir exclusion list. The rebuild wins when both apply: its own aggregate consumes the
  heal latch. If the rebuild can't start (share unmounted, scan already running) we log and keep serving the existing
  index; nothing stamps the DB until a rebuild actually truncates, so the next load re-arms. Triggers, name list, and
  the stamp are canonical in `../network_scanner/DETAILS.md` § "NAS snapshot/system dirs aren't recursed".
- **cover.rs** (+ `cover/bootstrap.rs`, `cover/live.rs`, and four test files split by harness: `cover/tests.rs` over a
  temp tree with an index, `cover/cold_drive_tests.rs` over a drive with none through the public handle,
  `cover/network_tests.rs` over the `Volume` trait, `cover/bench.rs` for the `#[ignore]`d primitive measurement) — the
  SEARCH-driven walk, the write half of the coverage concept whose read half is `../read/coverage.rs`. `Index::cover`
  resolves a `CoverContext` (the volume, its writer, its path space, its kind) here, spawns one Utility-QoS thread, and
  walks each frontier root the coverage answer named — through the local guarded walker or the volume's own `Volume`,
  whichever the kind calls for (`Ground`). `bootstrap` is everything that has to exist first: an index on a volume that
  has none, and an `entries` row for a path the index has never seen. `live` is which roots are already being walked.
  Detail below.
- **scan_completion.rs** — the post-scan handler: the vanished-volume abort and the LOCAL failure→Stale arm (below).
- **freshness.rs** — the `Fresh`/`Stale`/`Scanning`/`Failed` transition table (`Freshness::on`) +
  `initial_freshness_on_launch`.
- **failure.rs** — `IndexFailureSignal`, the one-shot per-volume fatal-storage-error signal.
- **lifecycle_bus.rs** — the neutral scan-completed / registration / dirs-changed pub/sub.

## What a launch does with the index it finds (the routing table)

`manager/launch_route.rs` is one pure function over the facts a launch reads off a volume's own database, and
`resume_or_scan` does what it says. It is separate from the side effects on purpose: **a wrong cell costs either a
wasted full rescan or a silently stale index, and both are invisible until somebody reports something strange.** The
unit tests beside it ARE the table.

An SMB share and an MTP phone never reach it: `resume_or_scan` routes `is_trait_scanned()` to `resume_or_scan_network`
first. For everything the local guarded walker reads, in the order the function asks:

- **journal replayable, gap too wide** ⇒ `start_scan("stale index: journal gap too large")`. Unchanged, and reachable
  ONLY on a volume that completed a scan, since replaying at all requires one — so ❌ no volume being covered in phases
  can take it. Its phased counterpart is `ensure_branch_watch`'s conditional epoch bump: a resume that can't replay the
  gap keeps the rows and says they're stale (`a_relaunch_with_no_replayable_journal_bumps_the_epoch`).
- **journal replayable** ⇒ `start_replay`. An install that already finished its first index loses nothing.
- **`scan_completed_at` set** ⇒ `start_scan("rescan of existing index")`, which reconciles in place. ⚠️ This is the path
  a `LocalExternal` drive takes at EVERY mount (`has_event_journal()` is `Local`-only) and the Linux boot disk's normal
  launch. ❌ The phased answers must never swallow it, or a finished external drive is treated as one nobody indexed.
- **phased first index switched off** ⇒ `start_scan("incomplete previous scan")`. The escape hatch's own row: with no
  phase machine to resume into, a phased partial takes today's truncating rebuild. Self-healing, and what the person who
  flipped it asked for. ❌ It never costs a COMPLETED volume its replay: the switch restores the BUILD path, not a
  rescan of everything already indexed. Shape and who flips it: `phases/DETAILS.md` § "The escape hatch".
- **rows, and no record of which ground they cover** ⇒ truncate, then the phase machine. This is the discriminator, and
  it is **the persisted branch set** (`branches::any_persisted`): `start_scan` clears the set before a whole-volume
  walk, so a first BULK scan somebody interrupted has none while a phased (or search-walked) volume does. Resuming into
  rows nothing accounts for would leave that ground unwatched and un-epoch-bumped, rendering last session's sizes as
  CURRENT with nothing having verified them.
- **anything else** ⇒ the phase machine, adding to what is there. A fresh install, a phased partial, a volume a search
  walked.

Two more truncate decisions sit one level below and are ❌ NOT part of this table: `start_scan`'s own
reconcile-vs-truncate rule (`local_rescan_reconciles`), and `register_a_phased_start`'s exclusion-policy rebuild (an
index whose rows were written under a policy this build doesn't apply counts as covering nothing).

### Every other way a full walk starts

`IndexManager::cover_or_scan` is the ONE door for "walk this volume whole", and it asks the same first question the
table does: no completed scan ⇒ the phase machine, otherwise `start_scan`. ❌ Don't add a caller that reaches past it
into `start_scan`. Four reach it, and each was its own way to blank a half-built index:

- **"Rescan now"** and **"Turn on indexing for this drive"**, both through `state::force_scan`. The enable arrives via
  `Index::start_volume` → `awaits_its_first_scan`; ❌ don't re-key that predicate to fix this (it's shared, and both
  shapes it serves have rows, so a row-count key would make the button a silent no-op on exactly them).
- **The FDA-deny launch** (`start_indexing_after_fda_decision` → `start_volume`), which is the same call.
- **`manager::perform_registry_rescan`**: a coalesced shallow `MustScanSubDirs`, a replay that couldn't roll forward, an
  ingestion backlog.

During the phased window a rescan RESTARTS the machine: the queue is recomputed from the host's current answers plus a
coverage query per root, so it picks up folders the user has come to care about since, and covered ground stays covered.
A machine that already has work is left alone (`AlreadyScanning`, which `force_scan` reports as `Started` — the walk the
caller asked for is in flight).

⚠️ **`perform_registry_rescan` stops the watcher and the live loop on its way in**, because the full scan it used to
reach started fresh ones. The machine starts a watcher too, but only from a walk's `begin_branch_coverage` — so
`cover_or_scan`'s phased arm calls `ensure_branch_watch(false)` itself, or a volume whose frontier is already empty
takes stock, completes, and spends the rest of the session with nothing watching ground it serves as covered.

⚠️ **The machine is started from OUTSIDE the registry-held window**, by `state::start_pending_phases`, at all three
sites. Both rescan doors hold the manager out under a transient `ShuttingDown` for the whole scan-start prelude, and
`cover_context_for` hands a context out only from a `Running` manager — start the machine in there and every one of its
first walks reports "did not run". At launch the ordering has a second reason (`state/startup.rs`).

⚠️ **And the standing-up itself runs off the lock too**, in two short `with_running_manager` windows with
`PhaseStart::run` between them. The start reads scan calibration off SQLite, asks the host for its open listings, and
spawns the reporter and the driver, while that driver's first act is to come back through the same lock for a write
context — so under the guard its first walk would wait on the whole prelude. Same discipline as `force_scan` below, and
the reason `with_running_manager`'s "non-blocking work only" holds. `PendingPhases` is what keeps the volume reading as
busy across the gap, and a manager that went away in it hands the machine back to be stopped rather than leaving it
walking with nothing holding it (`manager/phased.rs`).

⚠️ **A master off→on only brings back drives `drives_to_resume` names**, and its per-drive intent is
`persisted_scan_completed` — which a drive part way through its FIRST index doesn't have. So an external drive covered
in phases is forgotten by a master-switch cycle, exactly as one interrupted mid-bulk-scan always was. The boot disk is
`is_root` and always resumes. Closing it needs a persisted "the user enabled this drive" marker that doesn't exist yet;
❌ don't use the branch set as a proxy, since a drive somebody only SEARCHED has one too and auto-indexing it is exactly
what the veto forbids.

## The per-volume registry

```
INDEX_REGISTRY: Mutex<HashMap<VolumeId, IndexInstance { phase, kind, signals { freshness, events, cancel } }>>
```

`IndexInstance` holds what a volume's LIFECYCLE owns: its `phase` (`IndexPhase`), its `kind`, and the `VolumeSignals`
bundle it shares with its `IndexManager` (freshness `Arc`, event sink, stop token). The registry is the single authority
for WHICH volumes are indexed and for each volume's lifecycle. Every invariant the single-volume design held now holds
per volume id, keyed independently so two volumes can't corrupt each other: single-writer-per-DB, lock-first
reservation, drop-guard-before-drain, reads-via-`ReadPool`-never-under-the-lifecycle-lock.

**Disabled is the absence of a key.** There is no `IndexPhase::Disabled`. An `IndexInstance` only ever exists in
`Initializing` / `Running` / `ShuttingDown` / `Failed`; a stopped or never-started volume has no entry. `get_status`/
`is_active` treat an absent key as disabled, and `stop_indexing`/`clear_index` `remove()` the instance after the drain.
This is why IPC `get_index_status` for a stopped volume returns the same "not initialized" response a never-started one
does.

**Why one bundled instance** (vs. parallel `HashMap`s or a `DashMap`): keeping `{phase, kind, signals}` in one struct
keyed by volume id means a volume's phase and the handles its manager fires through are taken and dropped together. One
`Mutex<HashMap>` (not `DashMap`) keeps the lock-discipline reasoning identical to the old single-`Mutex` model: the lock
guards lifecycle transitions only, never reads.

The read-routing "skip if no index registered" gate (enrichment early-returns when `get_read_pool_for(vid)` is `None`)
lives with enrichment in `../read/CLAUDE.md`; the path→volume resolution (`volume_id_for_local_path`) lives in
`../paths/CLAUDE.md`. Neither consumes the registry.

## Where a volume's read handles live

A volume's `ReadPool` (lock-free enrichment/verification reads) and `PendingSizes` (the "size updating" hourglass) live
in volume-keyed tables in `../read/handles.rs`, NOT in its `IndexInstance`. This module builds both while it reserves
the volume's slot, PUSHES them in under the registry lock, and withdraws them on every teardown path (`stop_indexing`,
`clear_index`, `fail_index`, `remove_instance_and_handles`). `get_read_pool_for(vid)` / `get_pending_sizes_for(vid)`
answer from those tables alone and never touch `INDEX_REGISTRY`.

**Why push instead of letting the read side pull.** The read path runs on every listing, roughly twice a second per
pane. Resolving a handle out of the registry made that hot path wait on the same mutex teardown holds while it works,
and made `read` depend on `lifecycle` while `lifecycle` depended on `read` for the handle types — a genuine two-way
dependency, and the reason the index engine's largest module cycle was 20 modules wide. Pushing keeps the read side
strictly underneath lifecycle: `INDEX_REGISTRY` guards lifecycle only, exactly as the top-level invariant claims.

**Lock ordering.** The handle tables are LEAF locks: every operation is a hash lookup plus an `Arc` clone, with nothing
called while the guard is alive. Lifecycle takes a table lock while holding `INDEX_REGISTRY` (in
`try_reserve_initializing_phase`, so a volume is never visible in the registry without a routable read path); nothing
ever takes `INDEX_REGISTRY` while holding a table. One direction only, so no cycle exists to deadlock on. ❌ Don't add a
callback parameter or any other reach-out to `../read/handles.rs` — that's what would create the reverse edge.

**Withdraw-before-delete.** `stop_indexing` / `clear_index` / `fail_index` uninstall the handles and `invalidate()` the
pool BEFORE the drain and before any DB file is deleted. Withdrawal is what makes reads skip: after it returns, the
volume routes `None`, so no reader can still be holding — or can still open — a connection to a file that's about to go.
This is also why the `Failed` phase needs no read-path special case: a `Failed` instance stays registered for the badge,
but `fail_index` withdrew its handles before flipping the phase, so reads already skip.

**Freeing a slot and withdrawing its handles is ONE critical section** (`remove_instance_and_handles`, the start-up
failure path). The two orders are not equivalent: withdraw-then-free is safe because the key still exists while the
withdrawal runs, so no competing start can reserve yet; free-then-withdraw is NOT, because a competing
`start_indexing_for` can take the freed slot in between, install fresh handles, and have them withdrawn under it —
leaving a registered, live index that routes no read pool, so its listings show `<dir>` until the next stop/start. The
teardown paths (`stop_indexing`, `clear_index`, `fail_index`) take the first order; the failure path holds the registry
across both steps. Holding it there is safe by the leaf-lock property above, and it's the same registry → table nesting
the reservation already uses.

This one resists a regression test: the window is a few nanoseconds of straight-line code after a mutex release, and a
competing thread — even one spinning on the lock with its `IndexStore` pre-opened — never lands inside it (measured: 5
runs × 1,000 rounds against the broken order, zero detections). Reproducing it would need an injection seam in
production code, which isn't worth it. The guardrail is the comment on the function.

**Test isolation follows the same shape.** `stress_test_helpers::TestInstanceGuard` installs a private volume's pool +
tracker alongside its registry entry and withdraws both on drop. ❌ A bare `INDEX_REGISTRY.remove(vid)` in a test no
longer un-routes reads; go through `stop_indexing` or uninstall explicitly.

## A volume's stop signal is handed down, never looked up

`VolumeSignals::cancel` is the root of every cancellation under a volume: each long walk it starts (a full scan, a
reconcile, a subtree rescan, a verification) runs on a `child_token()`, so tearing the volume down stops all of them at
once. Whoever OWNS the token hands a child to the work it starts — `IndexManager` into `ScanCompletion` and
`ReplayConfig` (and from there into `EventReconciler` and the post-replay verification walk), `trigger_verification`
into `maybe_verify` while it already holds the instance.

The same hand-down covers what a walk needs to know about its VOLUME. `trigger_verification` takes the volume id, the
writer, and `IndexManager::path_space()` off one instance under one lock, so the verifier's read pool, its writer, and
its path space can't name different volumes (the contract, and what a mismatch costs, are in `../reconcile/DETAILS.md`).
`path_space()` is also the single derivation of the space the scan and the replay + live loops get, so no two of them
can disagree about where the volume is rooted.

❌ Nothing below `lifecycle` resolves a token by volume id. Beyond the import cycle that created, a late lookup is wrong
on its own terms: a walk that starts after its volume was torn down finds no instance, falls back to a fresh token that
never fires, and runs to completion writing into a draining writer — precisely the walk that most needs to stop. A token
captured up front cancels correctly in that case.

## The `IndexPhase` machine (and where the pipeline-phase EVENT lives)

`IndexPhase` (state.rs) is the LIFECYCLE state: `Initializing { store }` → `Running` → `ShuttingDown` (transient) →
absent, plus the terminal `Failed { reason, db_path }`. This is distinct from the pipeline-phase (`ActivityPhase`:
Replaying/Scanning/Aggregating/Reconciling/Live/Idle) that drives the FE step checklist — that lives in
`../events/CLAUDE.md` as the `index-phase-changed` event. Fire every pipeline-phase transition through
`events::set_phase_for(app, volume_id, phase, trigger)` (it does the global debug ring AND the per-volume emit in one
call so they can't drift); the lifecycle-phase transitions here are the `IndexPhase` swaps under the registry lock.

## Capability axes (`IndexVolumeKind`)

`IndexVolumeKind` (defined in the leaf `../volume.rs`) has four variants (`Local`, `LocalExternal`, `Smb`, `Mtp`) and
four orthogonal capability methods — the canonical per-kind table lives on the enum's doc comment, so branch on the
axis, not the variant:

- `uses_local_scanner()` — the guarded walker + FSEvents pipeline (`Local`, `LocalExternal`) vs the `Volume`-trait
  scanner. Exact complement of `is_trait_scanned()` (`Smb`, `Mtp`); a partition test in `../volume.rs` pins that they
  never drift, so a fifth variant must pick a side.
- `has_event_journal()` — self-heals watch continuity via FSEvents replay on launch. Only `Local` (the boot disk). Feeds
  `initial_freshness_on_launch`; a non-journaled kind loads Stale. This — NOT `last_event_id.is_some()` — gates journal
  replay: the shared local event loop persists `last_event_id` for any local-scanner volume, so a completed
  `LocalExternal` index carries one despite having no journal to replay.
- `mount_rooted()` — the index `ROOT_ID` is the mount (`/Volumes/X`), not `/`. True for every kind but `Local`.
- `feeds_search()` — the single volume whose writes back the in-memory search index. Only `Local`.

## Lock discipline (the load-bearing decisions)

**Lock-first `start_indexing`.** `start_indexing_for(app, volume_id, root)` opens a temporary `IndexStore` plus the
volume's `ReadPool`/`PendingSizes`, then atomically claims the `(absent) → Initializing(store)` transition via
`try_reserve_initializing_phase(volume_id, kind, store, pool, pending, signals)` BEFORE constructing the heavy
`IndexManager` (which spawns the writer thread). The reservation rejects when the volume already has ANY instance, so a
second start for the SAME volume no-ops; different volumes reserve independently. Without the lock-first claim, two
near-simultaneous calls for one volume can both spawn writer threads — each with its own `Arc<AtomicI64>` ID counter and
`AccumulatorMaps` — racing on the same DB (one of the mechanisms behind a historical ghost-size bug; the other, two
writers racing, is closed by this guard, with `UNIQUE (parent_id, name_folded)` as the safety net). The reservation also
publishes the volume's read handles, in the same critical section, so enrichment works from `Initializing` onward and no
window exists where the registry knows a volume the read path can't route.

**Drop the registry guard before the shutdown drain.** `stop_indexing(vid)` and `clear_index(vid)` swap the volume's
phase to `ShuttingDown` under the registry lock (taking the `IndexManager` out by value), then RELEASE the lock before
`mgr.shutdown()`. `shutdown()` blocks up to 5 s draining the live-event task. Holding `INDEX_REGISTRY` across that drain
would stall every concurrent `get_status`/`is_active`/`trigger_verification` caller — for ANY volume — for the whole
window and park a tokio worker, violating "reads never contend on the lifecycle lock." Dropping the guard mid-shutdown
is safe because the live loop reads via `ReadPool` and never reacquires the registry lock; concurrent callers observe
the published `ShuttingDown` phase (reported as not-initialized). After the drain, both re-lock only to `remove()` the
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
is binary: continuously-watched-since-scan ⇒ Fresh, any break ⇒ Stale. UI colors: **gray** = no registered instance (the
"disabled = no key" model, NOT a `Freshness` variant); **blue** = `Scanning`; **green** = `Fresh`; **yellow** = `Stale`;
**red** = `Failed`.

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
- **Scan transitions.** `ScanStarted` ⇒ Scanning; a CLEAN `ScanCompleted` ⇒ Fresh (only the `Ok` arm reaches it); a
  FAILED LOCAL scan/reconcile ⇒ `ScanFailed` ⇒ Stale.
- **Failed LOCAL scan ⇒ Stale, never a stuck spinner** (`scan_completion.rs`). `start_scan`'s completion handler fires
  `ScanFailed` (through the cloned freshness handle, no registry re-lock) from `report_unfinished_scan`, on both failure
  arms: `Ok(Err(_))` (a typed `ScanError` like `EmptyRoot`, or a `catch_unwind`-converted `Panicked`) and `Err(_)`
  (thread-join panic). A CANCELLED scan never reaches it — `ScanError::Cancelled` is split off before, and keeps the
  volume's prior freshness rather than reporting a failure. `ScanStarted` already moved the badge to Scanning, so
  without this a failed scan strands it on a perpetual blue spinner until relaunch. The prior index is NOT blanked; it
  gets the honest Stale "rescan available" badge and heals on rescan.
- **Interrupted SMB/MTP scan: disconnect ⇒ keep an honest partial + Stale; user-cancel ⇒ heal-to-rescan (gray).** The
  completion handler in `start_volume_scan` splits on `match result` (NOT a freshness-enum change — one transition
  table, the handler just chooses WHICH event to apply):
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
  - **User cancel** (`Err(VolumeScanError::Cancelled)`, which `is_terminal_disconnect` deliberately excludes): the
    partial is discardable — `discard_buffered_changes` + `state::reset_to_not_indexed` ⇒ gray, healing to a clean fresh
    scan on the next enable. (Timeout / writer-send / non-disconnect root-fatal also take this discard path.)
- **The watcher-driven transitions** (`WatcherDied`, `OverflowUnrecoverable`) fire from the transport live-watch layer
  (`../transports/CLAUDE.md`); this area owns only the transition table they feed.

## The Failed state (fatal storage failure) — stop loudly, don't retry forever

A real incident: the local index DB began returning `SQLITE_IOERR` on every read and write mid-scan. The writer thread
and the reconciler each just `log::warn!`-and-continued and retried FOREVER: 12,700+ identical warnings over 8 minutes,
~190% CPU, a frozen webview, and "Find files" stuck at 0%. The fix makes a dead index DB fail loudly, stop cleanly, and
show an honest state.

**Classification is typed, never on the message string**. `store::IndexStoreError::sqlite_code()` extracts
`(rusqlite::ErrorCode, extended_code)`; `is_fatal_storage_error()` is `true` for the storage-death classes
(`SQLITE_IOERR*`, `SQLITE_CORRUPT`, `SQLITE_CANTOPEN`, `SQLITE_FULL`, `SQLITE_READONLY`, `SQLITE_NOTADB`). Transient
contention (`SQLITE_BUSY`/`SQLITE_LOCKED`) is deliberately NOT fatal (the busy handler backs those off). The detector
lives in the writer (`../writer/CLAUDE.md`); this area owns the LIFECYCLE representation of the trip.

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
  its read handles were withdrawn before the phase flipped, so reads SKIP cleanly (no per-navigation flood on a dead
  DB). The stored `db_path` lets `clear_index` reclaim the file.
- `Freshness::Failed` (red): drives the badge through the SAME `index-freshness-changed` event the other colors use. It
  is TERMINAL in `Freshness::on` (only `ScanStarted` leaves it), so a concurrent scan-completion unwinding as the index
  is torn down can't downgrade a dead index back to Stale/Fresh.

**The supervisor (`state::spawn_failure_supervisor` → `fail_index`).** Spawned once when the volume becomes `Running`
(the signal is one-shot and `notified()` resolves even if the trip already happened, so a failure in the
Initializing→Running window is never missed). On the trip it runs `fail_index`: uninstall + invalidate the read-path
handles, take the manager OUT of the registry under the lock (publishing a transient `ShuttingDown`), DROP the lock,
`mgr.shutdown()`, re-lock and install `IndexPhase::Failed`, then fire `set_phase_for(Failed)` +
`apply_freshness_event( StorageFailed)`. Same drop-the-guard-before-the-drain discipline as `stop_indexing`. A no-op if
the volume isn't `Running`.

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

## The cover walk (`cover.rs`) — filling in what a search can't answer for

`../read/coverage.rs` says which folders under a scope nothing has listed; this walks them. Every row goes into the
volume's real index through its ONE writer (Decision 2 of `docs/specs/unindexed-search-plan.md`), so the work outlives
the search that paid for it and the next search over the same ground walks less.

**Which ground, and which walk reads it** (`Ground`). Every volume kind falls in one of two halves, and that is the ONE
per-kind branch in the whole coverage concept: a local filesystem is read by the guarded walker, and everything else — a
share, a phone, whatever backend comes next — through its `Volume` (`../network_scanner/DETAILS.md` § "The scoped cover
walk"). Downstream of a discovered entry the two are identical: same writer, same epochs, same `dir_stats`, same
frontier query, same descent rule. `Ground::under` resolves the half from the kind on the registry instance the writer
came from, and answers `None` when a trait-scanned volume has been ejected since the coverage answer.

**Two primitives on the local half, and which one runs.** A frontier node is virgin ground by definition, so this is a
bulk add and the parallel guarded walker wins outright — 3.2–5.8x over the serial reconcile with identical row counts on
four real trees (`docs/notes/cover-walk-primitive-2026-08-05.md`). The serial reconcile is kept as the REPAIR path, for
the one case the parallel walker can't take: a frontier node the index already holds rows under (`ScanError::NotVirgin`,
see `../scanner/DETAILS.md` § "Three scan roots"). It compares by name and writes only differences, which is exactly
that case's shape. The trait half needs no such split — it is add-only per directory, so it simply takes the case. ❌ No
path ever deletes: covering is add-only work.

**What it costs to cover a whole volume this way**, measured over a real 6.06M-entry `/` against today's
truncate-and-bulk-build: `docs/notes/phased-vs-bulk-index-2026-08-14.md`. Two findings a caller planning many walks over
one volume needs. First, the shallow stitch and the frontier query are free (0.2 s and 5 ms across 1,496 walks), so the
cost is never in the bookkeeping. Second, and load-bearing: a directory a walk couldn't read must be RECORDED, or every
later walk over an ancestor scope offers it again and pays the failing read again — 101 s of 147 s of walk time on a
machine with 76 such directories. The walk does that for itself now (`UnreadableCause::Abandoned`,
`../scanner/DETAILS.md` § "Ground the walk couldn't read"), so a caller planning many walks needs no bookkeeping of its
own. ❌ Don't reach for `WalkHeartbeat::abandoned_count` if you ever need the signal in-process: it counts stall
timeouts and consecutive-failure pruning, and read zero for every walk in every arm of that measurement — the failing
`readdir` case, which was 100% of what fired, never touched it.

**The backend's scan session brackets the WHOLE frontier**, not each root: over SMB that's a pool of extra connections
(`begin_scan_session` / `end_scan_session`), and opening one per frontier root would pay the setup repeatedly inside one
walk. ❌ Nothing between the two calls may return early — `walk_frontier` keeps the loop in a helper for exactly that
reason, so a cancelled walk can't leave the pool standing.

**Terminal states.** `CoverOutcome.cancelled` separates "the index answers for this scope now" from "somebody stopped
us", which are different phases in the UI. Neither is a failure: a cancelled walk still left every directory it read
marked, so the next walk resumes rather than restarts. One frontier root that can't be walked doesn't stop the others —
it stays frontier, and the next `coverage` call names it again.

**Ownership and cancellation.** `cover_context_for` matches `IndexPhase::Running` only, so a walk reuses the volume's
existing writer and never stands a second one up (two writers on one DB race the id counter and the accumulator maps).
Cancel through the `CancellationToken` the caller passed to `Index::cover`, never through the handle: the handle owns a
`Receiver` and so can't be shared with the thread that decides to stop it (a closing dialog, a quitting app), which is
why `CoverWalk` has no `cancel` of its own. DROPPING the handle does not stop the walk either (Decision 11 — walking is
coverage work, matching is query work, so a superseded query keeps its walk). `finish` drops the batch channel BEFORE
joining, so a caller that stopped reading can't deadlock against a walk parked on a full one.

**The channel is bounded at eight batches.** A consumer that falls behind slows the walk rather than growing a queue to
the size of the subtree; each batch already carries up to 2 000 entries.

### The two single-flight questions a scan has to ask

`start_scan` refuses for two independent reasons, and each catches a walk the other can't see (`start_volume_scan` asks
both as well — the trait half has the same doors and the slowest walks):

- **`mgr.scanning`** — the volume's own full scan. Set by `start_scan`, cleared by the completion handler.
- **`cover::ground_being_walked(volume_id, &[volume_root])`** — a search-driven cover walk. It sets no flag at all: it
  holds a CLAIM (`cover/live.rs`), and `cover_context_for` refuses only NEW walks, so nothing else stops a rescan
  arriving mid-walk. The volume root as the frontier asks about the whole volume, since `overlaps` counts an ancestor.

Without the second, a coalesced shallow anchor, a journal-gap fallback, or the manual button sends `TruncateData` +
`BumpCurrentEpoch` while the walk is still inserting: the walk's rows land in a blanked database, its ids lose to
`INSERT OR IGNORE`, and everything hanging off them is orphaned.

A third question is asked ABOVE these two, in `cover_or_scan`: whether this volume's first index is the phase machine's
at all (§ "Every other way a full walk starts"). A volume with no completed scan never reaches `start_scan`, so these
two guard the volumes that do.

**Both refusals are TYPED** (`rescan_request::ScanStartError`: `AlreadyScanning`, `GroundBeingWalked`, `Internal`).
Their wording used to be the only thing separating them, which the project's hard rule forbids classifying on, and which
left a caller nothing to branch on but prose. Regression anchor:
`cover::cold_drive_tests::a_truncating_rescan_refuses_while_a_search_cover_walk_is_live`.

### The one walk a volume remembers

⚠️ **Its domain is a volume that HAS a completed scan** (or one with the escape hatch off) — the volumes whose "Rescan
now" is still a truncating or reconciling full walk. A volume with no completion marker is the phase machine's, the
machine composes with a live cover walk by design (ground another walk holds is left to it), and so there is nothing to
wait for: the request is served immediately and reported as `Started`. **The two mechanisms answer for disjoint index
states**; ❌ neither supersedes the other, and ❌ don't "fix" the phased route into a deferral. Anchor:
`cover::cold_drive_tests::a_rescan_during_the_phased_window_starts_the_machine_under_a_live_walk`.

An AUTOMATIC trigger needs nothing more than the refusal: a journal gap and a coalesced anchor both recur on their own,
and nobody is watching a button for them. `manager::perform_registry_rescan` therefore logs and moves on.

A MANUAL one is different. A cover walk holds ground for seconds to minutes, the person who clicked "Rescan now" (or
"Turn on indexing" on a volume a search built an index for) can't see when it lets go, and telling them to click again
puts the scheduling on the one participant who can't observe the schedule. So `state::force_scan` — the entry point
behind both buttons — records a `GroundBeingWalked` refusal in `rescan_request` and answers `RescanOutcome::Deferred`,
which reaches the frontend as `StartOutcome::DeferredUntilSearchEnds` and becomes a toast that promises the scan.

- **The walk that blocked it runs it**, from `cover::release_ground`, in an order that is the whole trick: the branch
  set, then the claim, then the owed scan. Fired before the claim goes, the scan would see this very walk's ground and
  defer itself again, forever.
- **The request is recorded BEFORE the attempt**, and dropped again by an attempt that got somewhere. Recording it on
  the way out of a `GroundBeingWalked` refusal reads more naturally and has a hole: the walk can end between the guard
  answering and the request landing, and its `run_if_owed` would carry nothing out, leaving a promise waiting on a walk
  that already finished.
- **❌ Nothing assumes the coast is clear.** The fire re-asks both guards by going through `force_scan`, so a second
  walk still holding ground re-defers the request behind ITS ending. That's what makes a truncating scan under a live
  walk unreachable however many walks are in flight
  (`cover::cold_drive_tests::a_remembered_rescan_waits_for_the_last_walk_out`).
- **One request per volume, memory only.** It carries nothing but "this volume wants a full walk", so a second click
  describes the same work and a set of volume ids is the whole state; quitting drops it. Every teardown path drops it
  too (`stop_indexing`, `clear_index`, `remove_instance_and_handles`), and the master switch going off drops it at fire
  time, so a stopped drive is owed nothing (`cover::cold_drive_tests::a_drive_that_stopped_indexing_is_owed_no_rescan`).

### What the walk leaves watched

A walk that covered ground and left nothing watching it is a snapshot of a folder taken once, which is what the plan's
rejected 24-hour expiry existed to bound. Instead the walk registers its frontier roots as WATCHED BRANCHES, and the
volume's live loop keeps exactly that ground current — so a walked branch is as live as an indexed drive's rows, with no
re-walking and no TTL (Decision 9). The mechanism, the admission rule, the persistence, and the Linux split are
canonical in `../watch/DETAILS.md` § "Watching what a search walked"; what belongs here is the lifecycle wiring:

- **`cover::start` registers before the walk thread spawns** (`state::begin_branch_coverage`), so a change landing in
  ground the walk has already passed waits instead of racing the walk's own ids — the same collision item 3 above
  describes between two walks, with the live loop as the second writer. It runs on EVERY volume with a loop, including a
  scanned one.
- **The walk thread finishes it** (`state::finish_branch_coverage`) after `walk_frontier`'s own writer flush, whatever
  the outcome: a cancelled walk still marked every directory it read, so that ground needs watching exactly as much.
- **The release is independent of the registry phase**, and the asymmetry with `begin` is deliberate. A walk can only
  START on a `Running` volume (`cover_context_for` hands out a context for no other phase), but it ENDS minutes later,
  possibly inside the `ShuttingDown` window `force_scan` / `perform_registry_rescan` publish for the whole of a scan
  start. So finish reaches the set through `branches::live_for` and only ASKS the manager (when there is one) what to
  leave behind; routing the release itself through `with_running_manager` left the branch at `walks > 0` for the rest of
  the session — its events buffered and never promoted, `may_walk` false for that ground, and never absorbed. Anchor:
  `cover::cold_drive_tests::a_walk_that_finishes_while_the_manager_is_shutting_down_still_releases_its_branch`.
- **`ensure_branch_watch` starts the watcher** for a volume that has none — the `WriterOnly` shape, the only one with
  coverage and no watcher. A scanned volume declines it (`branch_watched` says which of the two is up), and `start_scan`
  retires the branch set outright, so a volume is branch-watched or whole-watched and never both.
- **A resume is the `WriterOnly` arm of `start_indexing_for`**, not a launch pass: an unregistered volume answers
  neither sizes nor coverage questions, so the first moment that coverage can be read is the moment its index comes up.

What a walk needs standing up first, and how two walks stay off one patch of ground: `cover/DETAILS.md`.

## Vanished-volume scan abort (`scan_completion.rs`)

A drive yanked mid-scan makes the local scan's ROOT unlistable: the fresh guarded-walker scan detects `dirs_read == 0`
on a volume-root scan, and the reconcile walk hits `reader.read(root) == None` — both return the typed
`ScanError::RootUnlistable`, distinct from `ScanError::EmptyRoot` (a readable-but-empty root, e.g. a blank USB stick,
which legitimately completes). The completion handler fires `ScanFailed` ⇒ Stale for every failure and, ONLY for
`RootUnlistable`, additionally goes `Idle` and reports `IndexEvent::ScanAborted { volume_id }` — clearing the frontend's
stuck "scanning" row, mirroring the network path's disconnect arm. No `scan_completed_at` is written, so the index heals
to a rescan on remount. `scan_failure_is_vanished_volume` is the pure distinguisher; an empty root or a walk panic does
NOT abort. (The wedge-safe unmount/eject ORDERING that stops the index before the FS goes away lives in
`../transports/CLAUDE.md`.)

## The neutral lifecycle bus (`lifecycle_bus.rs`) — single source

A minimal in-process pub/sub so a backend subsystem (the importance scheduler; later the media-ML enrichment scheduler)
learns when a volume finished scanning, WITHOUT `indexing/` depending on it (the one-way `consumer → indexing`
direction). This is the single canonical home for the mechanism; consumer docs point here.

- **Published from the neutral chokepoint.** `apply_freshness_event_on` calls
  `lifecycle_bus::publish_scan_completed(vid)` on a `ScanCompleted`, alongside the FE `.emit`. Both the LOCAL and
  network completion paths funnel through this seam. It publishes on the EVENT, not on a freshness CHANGE: a Fresh→Fresh
  rescan completion still means new data to rescore.
- **`tokio::sync::watch`, NOT `broadcast`.** A `broadcast` doesn't replay a value sent before a receiver subscribes, so
  a `ScanCompleted` fired during `setup()` before the scheduler subscribes would be lost. A `watch` retains the last
  value. The publish uses `send_replace` (not `send`), which updates the retained value even with zero receivers.
- **Senders live in a module map, not `IndexInstance`.** A `watch::Sender` per volume id in a process-global `BUS`,
  created lazily. Keeping it OUT of the instance is deliberate: the sender must outlive the instance so a subscriber
  that took its receiver keeps seeing the last state after the volume unmounts. `ScanState` carries a monotonic
  `generation` so a consumer can coalesce a repeat.
- **The startup sweep is the bus's companion, not part of it.** A volume already Fresh at launch never re-fires
  `ScanCompleted`, so `state::ready_volumes_with_kind()` snapshots the volumes that are `Fresh` right now (with each
  volume's typed `IndexVolumeKind`) for the scheduler to enqueue once at startup.
- **A registration `broadcast`** (`publish_volume_registered` / `subscribe_registrations`) carries late-registering
  volumes (a share mounted AFTER startup), published from `start_indexing_for` right after a volume wins its
  `Initializing` reservation, carrying the id AND its typed kind. A lagged receiver only misses a registration the next
  `ScanCompleted` still covers, so a miss self-heals.
- **A `dir-changed` channel** (`publish_dirs_changed` / `subscribe_dirs_changed`, a per-volume `watch<DirsChanged>` in a
  separate `DIR_BUS` map) carries live listing changes from the live event loop and the per-navigation verifier — the
  importance scheduler's incremental-recompute trigger and the media index's live-tick trigger. Being a `watch`, a burst
  can drop an intermediate batch; accepted, because the next full recompute heals it.
- **The `dir-changed` payload is the ORIGIN dirs, never their ancestor closure.** A live change carries two different
  facts: _these directories' listings changed_ (a small set: the changed entry's parent, plus the entry itself when it's
  a new directory) and _these directories' recursive sizes need refreshing_ (the first set plus every ancestor up to
  `/`). The bus carries only the first; the second is rebuilt where it's needed by
  `paths::path_prefix::with_ancestor_closure` (the `index-dir-updated` emit and the "size updating" hourglass, both at
  the drain point in `watch/event_loop/live.rs`). **Gotcha/Why:** publishing the closure conflated them, and every batch
  therefore carried `/Users` and `/`. Both bus consumers expand each entry DOWNWARD — importance into the whole subtree
  (a folder renamed to `node_modules` floors everything below it), media into the dir's own image children — so a
  two-folder change rescored ~90,000 folders and reloaded 161,094 weights every 60 s, for the whole session (measured on
  prod v0.36.2, 2026-07-28). ❌ Don't hand `with_ancestor_closure`'s output to `publish_dirs_changed`.

See `importance/DETAILS.md` for how the scheduler combines the sweep + the bus with per-volume coalescing and per-kind
policy.

## The IPC surface (resolved here, commands elsewhere)

The per-drive freshness UX drives any drive through three thin `commands/indexing.rs` commands: `enable_drive_index`,
`disable_drive_index`, `rescan_drive_index`. For root they map to `start_indexing`/`stop_indexing`/`force_scan`;
SMB/MTP/ local-external routing lives in `../transports/CLAUDE.md`. `enable`/`rescan` return `EnableIndexingOutcome`
(`{ status: "started" }` or, for SMB, `{ status: "refused", reason: SmbIndexGateReason }`). The per-volume status IPC
(`get_volume_index_status(path)` for the active-drive badge, `get_volume_index_status_by_id` for the dropdown rows)
builds
`VolumeIndexStatus { volume_id, enabled, freshness, scan_completed_at, scan_duration_ms, coalesced_signals_since_sweep, next_sweep_due_at }`:
freshness from the registry, the scan facts from the persisted `meta`. `enabled: false` + `freshness: None` is gray. The
path→volume resolution feeding these lives in `../paths/CLAUDE.md`.

## The two indexing switches

Canonical model; everywhere else points here. Two switches decide whether a volume indexes, and they compose ONE way.

- The **master switch** is `indexing.enabled` in settings, mirrored into the process-wide atomic in `master.rs`. It's a
  hard gate: off ⇒ nothing indexes on its own, anywhere. Seeded in `lib.rs` setup BEFORE `indexing::init` (which is what
  unblocks the handle-free SMB reconnect resume, so a late seed lets a NAS re-index itself), then live-applied by
  `set_indexing_enabled` per the settings live-apply rule.
- The **per-drive intent** lives on each volume's own index DB, as two meta markers read by
  `master::drive_index_should_run`: the sticky `user_disabled` (an unconditional veto) and `scan_completed_at`. The boot
  disk is opt-OUT (`is_root`: it indexes unless disabled); every external drive is opt-IN, so it needs a completed scan
  on record before anything resumes it uninvited.

**Both switches govern BACKGROUND work only, never a user-initiated read** (Decision 13). A search-driven coverage walk
runs with the master off and on a `user_disabled` drive alike: it stands up a writer and nothing else, so no scan is
scheduled and no watcher starts, and the alternative isn't "less work" — it's a search that silently omits files on a
drive the user can see. The veto keeps real teeth anyway, and this is where they are: a vetoed drive gets no watcher
(`master::branch_watch_allowed`), so what a search walked there stays covered and served but stops being kept current
the moment the app stops. ❌ It is NOT re-walked — the walk marked those directories listed, so the frontier never
offers them again and Decision 5 trusts them as covered-but-stale. ❌ Don't turn either switch back into a gate on
`WriterOnly`; `cover::cold_drive_tests::a_search_walks_a_drive_with_the_master_switch_off` guards it, and
`a_vetoed_drive_is_walked_and_left_unwatched` guards the other half.

Enforcement is one choke point: `start_indexing_for`, which all four transports funnel through, refuses an
`IndexTheVolume` activation while the master is off. Callers that answer a user get a typed refusal of their own instead
of a silent no-op: `enable_drive_index` → `EnableIndexingOutcome::IndexingDisabled` (transport-neutral, so the FE has
one shape to match), `start_indexing_for_smb` → `SmbIndexGateReason::IndexingDisabled` (refused BEFORE the os_mount
upgrade, so a refused start can't clear the drive's `user_disabled` marker as a side effect).

Toggling the master switch never writes per-drive intent, in either direction. Off runs `stop_all_indexing`, which is
`stop_indexing` per volume and so leaves the markers alone (see `../transports/CLAUDE.md` for why that separation is
load-bearing). On walks `master::drives_to_resume` (root plus every registered volume whose persisted intent says yes,
minus the already-active ones) and routes each through the normal per-drive enable, so each transport's own gate still
applies and an offline share simply waits for its reconnect resume. Net effect: any number of master toggles round-trips
to exactly the set of drives the user chose.

The FE mirrors this rather than re-deriving it: `getDriveIndexingEnabled()` (reactive settings) makes the per-drive
badge menu and the `Indexing > Drive indexing` sub-rows render as overridden while the master is off.

## FDA-deferred root auto-start

At first launch on macOS, recursively scanning from `/` opens iCloud Drive, Photos, and other TCC-protected directories,
which makes macOS stack native permission popups on top of the in-app FDA modal (we hit 5-10 once).
`should_auto_start_indexing(indexing_enabled, fda_pending)` gates the launch-time start on a plain `bool`: the FDA rule
is the HOST's, not the index's. The app resolves it (`fda_gate::is_fda_pending(fda_choice, os_fda_granted)`: pending
when `fda_choice == NotAskedYet` AND `os_fda_granted == false`, so `os_fda_granted == true` overrides `NotAskedYet`) and
hands the answer to `Index::start_root_at_launch(fda_pending)`. ❌ Don't reach for a TCC concept in this crate; it can't
name `fda_gate` and shouldn't know what a TCC choice is. Once the user picks Allow (restart) or Deny (same session, via
`start_indexing_after_fda_decision`), the indexer starts. FDA gates ONLY `root` — SMB/MTP/external paths aren't
TCC-protected, so `start_indexing_for_smb` and the MTP/local-external enables never route through this gate (unlike the
master switch, which gates every transport). After Deny the indexer runs in degraded mode (one TCC prompt per protected
folder, the contract the user opted into). Launch-time NSWorkspace icon fetches in `volumes::list_locations` share the
same `is_fda_pending` predicate so the two gate sites can't drift.

## Testing

The registry / phase / freshness state-machine tests and their serialize-on-a-dedicated-mutex discipline live in
`../tests/CLAUDE.md` (`integration_tests.rs`, the stress suites) and colocated `state/tests.rs` +
`freshness.rs`/`failure.rs`/`manager.rs` unit tests. Key regression anchors named above:
`scan_start_freshness_firing_does_not_relock_the_registry`,
`force_rescan_routes_smb_and_mtp_to_the_trait_scanner_not_the_local_walker`,
`forget_stale_index_transitions_to_gray_and_deletes_db`, and the two disconnect-storm tests.

## `ScanProgressReporter` (`progress_reporter.rs`)

The 500 ms progress + mid-scan partial-aggregation tick loop shared by EVERY scan path (local fresh/reconcile via
`start_scan`, SMB/MTP trait fresh/reconcile via `network_scan`), so the coordinator reads as "dispatch scanner → await
completion → spawn live loop".

- `new(progress, writer, events, volume_id, partial_agg_source)` builds it; `spawn(scan_done)` runs the loop on
  `host::runtime::spawn` (a scan can start from the sync Tauri `setup()` hook) until the completion handler sets
  `scan_done`. Partial passes are therefore structurally scoped to the full-scan window.
- `partial_agg_source` is chosen by the caller per scan kind: `Maps` for a fresh scan (accumulator maps populated by
  `InsertEntriesV2`), `Sql` for a reconcile rescan (maps empty). See the `source: Maps|Sql` contract in
  `../writer/DETAILS.md`.
- Each `tick()` reports `IndexEvent::ScanProgress`, then — via a tick counter gated behind
  `partial_agg::should_send_partial_agg` — snapshots the listing cache (`caching::snapshot_listings()`), runs
  `partial_agg::collect_hot_paths`, maps each firmlink-normalized absolute hot path into the volume's index-relative
  space via `routing::index_read_path` (the SAME volume-root strip enrichment uses; a pass-through for `root`,
  mount/scheme strip for SMB/MTP), and fires a non-blocking
  `writer.try_send(ComputePartialAggregates { hot_paths, source })`. The whole partial-agg block sits behind the gate,
  so skipped ticks do zero extra work.
- Keeps its sink by value (cloned in by the caller), so the spawned loop owns everything it needs; the genuinely pure
  decision logic already lives (and is unit-tested) in `partial_agg`.

## `partial_agg` — the pure helpers (`partial_agg.rs`)

Side-effect-free so the timer loop stays a dumb caller and both helpers are exhaustively unit-tested.

- `should_send_partial_agg(tick, queue_depth)` — the send gate: fires every `PARTIAL_AGG_TICK_INTERVAL`-th tick (10 = 5
  s), never on tick 0, skips when `queue_depth > PARTIAL_AGG_MAX_QUEUE_DEPTH` (4,000; a depth of exactly the max still
  sends). So partial passes never compete with the real insert backlog.
- `collect_hot_paths(listings, scanned_volume_id)` — turns a `snapshot_listings()` result into firmlink-normalized hot
  paths: keeps only listings whose `volume_id` equals the scanned volume's (dropping `network`/`search-results`/`mtp-*`/
  SMB and other local volumes whose absolute-looking paths would resolve against the wrong per-volume DB) and whose
  `path` is absolute, normalizes via `firmlinks::normalize_path`, and dedups preserving first-seen order.
- Both constants live here with their rationale and the real-volume tuning numbers.

Why this exists (the UX call): during a full scan, folder sizes otherwise don't exist until the single end-of-scan
`ComputeAllAggregates` pass, so every listing shows placeholders for the whole scan (~2.5 min on a 5M-entry volume) and
all sizes pop in at once — exactly when a new user is judging the headline feature. Partial passes refresh listings
every few seconds with growing numbers next to the existing hourglass (a partial number beats a placeholder). The
writer-side handler that consumes these messages (borrow-not-consume the maps, the depth-≤3 write cap, the empty-maps
SQL-free no-op) is owned by `../writer/DETAILS.md`.
