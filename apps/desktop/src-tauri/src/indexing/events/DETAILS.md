# Indexing events + progress surface details

Read this before any non-trivial work in `indexing/events/`: editing, planning, reorganizing, or advising. Must-know
invariants are in [CLAUDE.md](CLAUDE.md).

This area owns the frontend-facing event payloads, the phase-transition emitter, and the scan-progress tick loop.

## The Tauri event payload catalog (`mod.rs`)

Each struct derives `tauri_specta::Event` with a pinned `#[tauri_specta(event_name = "…")]` kebab wire name (the
`…Event` suffix wouldn't kebab-case to the existing string), emits via `payload.emit(app)`, is registered in `ipc.rs`'s
`collect_events!`, and is consumed via typed `on*` wrappers in `tauri-commands/indexing.ts`:

- `IndexScanStartedEvent` (`index-scan-started`) — carries `prior_total_entries`, `prior_scan_duration_ms`,
  `volume_used_bytes` (the static per-scan calibration; the FE's calibrated-vs-rough tier decision is a pure function
  of it).
- `IndexScanProgressEvent` (`index-scan-progress`) — `entries_scanned`, `dirs_found`, `bytes_scanned`; emitted every
  500 ms by the reporter.
- `IndexScanCompleteEvent` (`index-scan-complete`).
- `IndexScanAbortedEvent` (`index-scan-aborted`, `{ volume_id }`) — emitted by the network disconnect/cancel/fail arms
  and the local `RootUnlistable` abort. A network scan that ends WITHOUT completing fires no scan-complete, so this
  tells the FE to clear that volume's live activity, else the corner indicator and breadcrumb badge keep a stuck
  "scanning" row. Carries no completion facts.
- `IndexDirUpdatedEvent` (`index-dir-updated`).
- `IndexReplayProgressEvent` / `IndexReplayCompleteEvent`.
- `IndexAggregationCompleteEvent` (`index-aggregation-complete`) — carries `volume_id` so the FE clears the right
  drive's aggregation row.
- `IndexMemoryWarningEvent` (`index-memory-warning`) — carries `resident_gb`, `phys_footprint_gb`, `heap_mb`.
- `IndexPhaseChangedEvent` (`index-phase-changed`, `{ volume_id, phase: ActivityPhase }`) — the per-volume
  pipeline-phase event driving the FE step checklist; see § "set_phase_for".
- `index-freshness-changed` — the per-volume badge-color event (`VolumeIndexStatus.freshness`); emitted by the
  freshness layer, whose state machine is owned by [`../lifecycle`](../lifecycle/DETAILS.md).

`RescanReason` (and `emit_rescan_notification`, `index-rescan-notification`) lives here too: `StaleIndex`, `JournalGap`,
`ReplayOverflow`, `WatcherStartFailed`, `ReconcilerBufferOverflow`, `IncompletePreviousScan`, `WatcherChannelOverflow`,
`IngestionBacklog`. Every code path that falls back to a full rescan emits one; the FE maps each to a toast. The
triggers that CHOOSE each reason live in [`../watch`](../watch/DETAILS.md) and
[`../reconcile`](../reconcile/DETAILS.md), not here — this area owns only the payload shape.

Two payloads that could look like they belong here but don't: `AggregationProgressEvent` (`index-aggregation-progress`)
lives in [`../writer`](../writer/DETAILS.md), and `SearchIndexReadyEvent` (`search-index-ready`) lives in
`commands/search.rs`. Also here: the IPC response types (`IndexStatusResponse`, `IndexDebugStatusResponse`).

## `set_phase_for` — the two phase records (`mod.rs`)

There are TWO records of the top-level pipeline phase (`Scanning → Aggregating → Reconciling → Live`, plus `Replaying` /
`Idle`), and they answer different questions:

- **Global, app-wide**: `DEBUG_STATS.set_phase()` appends to one `PhaseRecord` ring (capped at 20) that the debug
  window's "Phase timeline" reads. It's a singleton: under two concurrent volumes it interleaves their transitions and
  can't say WHICH drive changed. Debug-only; keep it.
- **Per-volume**: the `IndexPhaseChangedEvent { volumeId, phase: ActivityPhase }` event tells the frontend which drive
  moved to which phase, driving the per-volume step checklist. `ActivityPhase` (Replaying/Scanning/Aggregating/
  Reconciling/Live/Idle) is a serde `snake_case` specta enum, so the FE branches on the typed variant, no
  string-matching on labels.

`set_phase_for(app, volume_id, phase, trigger)` (a `pub(super)` fn) does BOTH in one call — the global ring plus a
fire-and-forget per-volume emit — so the two can't drift. Every `set_phase` site where a `volume_id` and an `AppHandle`
are in scope goes through it: `lifecycle/manager.rs` (local `Replaying`/`Scanning`, the completion task's `Aggregating →
Reconciling → Live`, `Idle` in stop/shutdown), `lifecycle/network_scan.rs` (`Scanning` at start; `Live` on clean
finish, `Idle` on disconnect), `lifecycle/scan_completion.rs`, and `watch/event_loop/replay.rs` (`Live` at the end of
replay). Spawned tasks capture cloned `app` / `volume_id`, never re-resolving the manager in the registry (same
discipline as the freshness `Arc`).

The event fires only on TRANSITIONS, so a frontend that joins mid-scan (window reload) can't learn the current phase
from it. The FE backfills observable steps from the scan/aggregation activity it already receives; the reconcile step is
the one transition with no other signal, so it's briefly unobservable after a reload that lands mid-reconcile (accepted,
rare). `VolumeIndexStatus` deliberately does NOT carry a current phase: it isn't stored per-volume (only in the global
`DEBUG_STATS`), so exposing it would mean threading a new per-instance phase handle through the spawned completion
tasks — lifecycle complexity the brief reconcile gap doesn't justify.

**Network-scan honesty.** SMB/MTP emit only `Scanning → Live` (no distinct `Aggregating` / `Reconciling` phase), yet
the writer still runs aggregation and emits its per-volume sub-phase events (`loading → sorting → computing → writing`).
So the FE drives the "compute folder sizes" step off the aggregation events, not a top-level phase network never sends;
and `saving_entries` never fires for network (entries insert inline during the walk), so that step simply doesn't
appear. Don't fake either by calling local-only helpers on the network path.

## `ScanProgressReporter` (`progress_reporter.rs`)

The 500 ms progress + mid-scan partial-aggregation tick loop shared by EVERY scan path (local fresh/reconcile via
`start_scan`, SMB/MTP trait fresh/reconcile via `network_scan`), so the coordinator reads as "dispatch scanner → await
completion → spawn live loop".

- `new(progress, writer, app, volume_id, partial_agg_source)` builds it; `spawn(scan_done)` runs the loop on
  `tauri::async_runtime::spawn` (a scan can start from the sync Tauri `setup()` hook) until the completion handler sets
  `scan_done`. Partial passes are therefore structurally scoped to the full-scan window.
- `partial_agg_source` is chosen by the caller per scan kind: `Maps` for a fresh scan (accumulator maps populated by
  `InsertEntriesV2`), `Sql` for a reconcile rescan (maps empty). See the `source: Maps|Sql` contract in
  [`../writer`](../writer/DETAILS.md).
- Each `tick()` emits an `IndexScanProgressEvent`, then — via a tick counter gated behind
  `partial_agg::should_send_partial_agg` — snapshots the listing cache (`caching::snapshot_listings()`), runs
  `partial_agg::collect_hot_paths`, maps each firmlink-normalized absolute hot path into the volume's index-relative
  space via `routing::index_read_path` (the SAME volume-root strip enrichment uses; a pass-through for `root`,
  mount/scheme strip for SMB/MTP), and fires a non-blocking `writer.try_send(ComputePartialAggregates { hot_paths,
  source })`. The whole partial-agg block sits behind the gate, so skipped ticks do zero extra work.
- Keeps `AppHandle` by value rather than abstracting emission behind a closure: emitting progress is the reporter's
  whole job, and the genuinely pure decision logic already lives (and is unit-tested) in `partial_agg`.

## `partial_agg` — the pure helpers (`partial_agg.rs`)

Side-effect-free so the timer loop stays a dumb caller and both helpers are exhaustively unit-tested.

- `should_send_partial_agg(tick, queue_depth)` — the send gate: fires every `PARTIAL_AGG_TICK_INTERVAL`-th tick (10 =
  5 s), never on tick 0, skips when `queue_depth > PARTIAL_AGG_MAX_QUEUE_DEPTH` (4,000; a depth of exactly the max still
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
SQL-free no-op) is owned by [`../writer`](../writer/DETAILS.md).
