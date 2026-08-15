# Indexing events details

Read this before any non-trivial work in `indexing/events/`: editing, planning, reorganizing, or advising. Must-know
invariants are in `CLAUDE.md`.

This area owns the typed event values, the `IndexEvent` envelope and its `EventSink` seam, the IPC response types, and
the phase-transition emitter. It imports DOWN only, so it sits in no dependency cycle.

## The values an event carries (`payload.rs`)

`ScanRunKind`, `RescanReason`, `ActivityPhase`, and `MemoryWatchdogAction` live below both `sink.rs` and `mod.rs`
because both name them: the envelope carries them, and the IPC responses (`IndexStatusResponse.scan_run_kind`,
`IndexDebugStatusResponse.activity_phase`) report them back. Keeping them in `mod.rs` is what made `sink.rs` import its
own parent — one of the three edges in the `events ↔ sink ↔ media_index::events` cycle. `MediaEnrichTerminalReason` is
the same idea from the other direction: it now sits in `sink.rs` beside the `IndexEvent` variant that carries it, with a
`pub use` from `media_index::events` so the scheduler's call sites are unchanged.

## The event seam (`sink.rs`)

The index subsystems say what happened; the app decides what a human sees. A subsystem builds an `IndexEvent` and hands
it to an injected `EventSink`. Nothing here names a wire format, an event name, or a sentence.

`IndexEvent` has 21 variants. Eighteen become frontend events (`ScanStarted`, `CoverageBranchStarted`,
`CoverageBranchEnded`, `CoveragePhaseStarted`, `ScanProgress`, `ScanComplete`, `ScanAborted`,
`DirsUpdated`, `ReplayProgress`, `ReplayComplete`, `RescanScheduled`, `AggregationProgress`, `AggregationComplete`,
`MemoryWarning`, `FreshnessChanged`, `PhaseChanged`, `MediaEnrichProgress`, `MediaEnrichTerminal`). Three reach the
host's own machinery instead:

- **`Error { report: IndexErrorReport }`** — a failure worth an error report, described by what broke rather than by the
  sentence someone would write about it: `MemoryWatchdog` (action, footprint, limit, escalation, the breakdown),
  `StorageFailed` (the typed `IndexFailure` plus context), `LiveEventLoopUnavailable`, `WalkWorkerSpawnFailed`. The app
  renders each and raises it through `log_error!`, which is what feeds `error_reporter::auto_dispatcher`. **This exists
  because a crate can't invoke a crate-root macro**; dropping it would silently cost the shipped-error feedback loop.
  The backtrace is still the failure's, not the mapper's — `emit` is a synchronous call from the failing code.
- **`PathAccessDenied { path }`** — the scanner hit an OS denial. The app decides whether it's TCC-restricted and worth
  the sidebar's "limited by macOS" styling.
- **`HomeCovered { volume_id }`** — the user's home folder stopped needing a walk. A REPORT, so a host can time the
  moment their own files started answering; the marker behind it still drives exactly one subscriber INSIDE the crate
  (`lifecycle/phases/completion.rs`). The app routes it `Destination::AnalyticsOnly` and nothing renders it, since what
  a user sees is the size that appears, not the marker.

**`CoveragePhaseStarted` carries a typed `CoveragePhase`**, one of the crate's own public values (`payload.rs`), and
that enum's declaration order IS the schedule the phase queue runs. The order lives there and nowhere else, so a host
never has to hold a second idea of which folders come first — or of `IndexPathSpace`, where an app-side home path can
disagree about firmlinks and mislabel on somebody else's machine. `VisitedRoot` is one of the four: a folder the user
opened mid-run is a phase like any other, ranked, queued, and run, so the crate reports it as itself and the host
decides whether to word it apart (today it doesn't).

**The event names the PHASE root; the branch pair names the frontier roots one level down.** The two are not
interchangeable: a host reading the phase off the branch events couldn't tell `~/Library` (the home phase) from
`~/Downloads` (a priority root), and the branch events are debounced besides, so the answer would lag a phase boundary
or skip it.

**It fires on TRANSITIONS, so it can't be the only door.** A host that joined mid-run reads the running phase off
`IndexStatusResponse::coverage_phase`, which answers the same question for a window that reloaded inside a phase — on
the whole-volume phase, the next boundary is the end of the run.

**The coverage-branch pair brackets one walk over one branch, and every kind of run emits it.** A phase names the
frontier root it is covering; a walk that takes the volume whole names the volume root (`announce_whole_volume_walk`).
That equivalence is the point: `/` is at or above every path on the volume, so a consumer's bidirectional membership
test matches every row through the same predicate that matches `~/Downloads` to the rows inside and above it — with no
sentinel value, and nothing anywhere that branches on which kind of run is running.

A phase emits both ends itself, on every exit path (covered, left to another walk, cancelled), because a consumer that
marked rows in flux on the start has nothing else to take that back. A whole-volume walk emits only the START: its end
is the run's own terminal event, and the host closes a volume's open ground there, which is the one path that also
covers an abort. ❌ The crate does NOT decide how long a walk has to run before it's worth announcing — that's the
host's presentation call, in the app's `events/index_mapping/walk_announcer.rs`.

`ScanStarted { covered_in_phases }` survives that collapse with ONE consumer left: which family of pipeline steps the
run produces (a phased run takes one of the four, and three permanently-pending steps read as a stuck scan). ❌ It is
not an input to anything about folder sizes, and ❌ nothing may re-derive it from the walked ground, which is empty
between branches and would flip the checklist's shape mid-run.

`IndexEventKind` is the payload-free twin, so a test can assert the SHAPE of a stream
(`[ScanStarted, ScanProgress, ScanComplete]`) without spelling out fields. Its `ALL` array is complete by construction,
and the app-side completeness test then fails until a sample joins `one_of_every_kind`.

**How `ALL` stays complete** (three compile errors, in the order you hit them): the private `slot_of` matches
exhaustively over the enum, so a new variant has no arm and the match doesn't compile. Each arm wraps its index in a
`const` block, which the compiler evaluates whether or not the arm ever runs, so the new arm's `Self::slot(n)` panics at
compile time until `ALL` has an `n`th entry. Adding that entry then trips `ALL`'s declared length. ❌ Don't "simplify"
any of the three away: an array literal's length says nothing about a variant count, so without them the app-side test
passes vacuously for exactly the variant somebody forgot. A `const _: () = { … }` block below `slot_of` closes the last
gap, asserting `ALL[i].slot_of() == i` so the array can't hold a duplicate, a stray, or a gap either.

Three sinks ship: the app's `TauriEventSink`, `NoopEventSink` (paths and tests with nothing to say —
`NoopEventSink::shared()` hands out one `Arc`), and the test `RecordingSink`.

`Diagnostic(String)` wraps English the index produces for logs and never for the UI. The newtype is the point: a bare
`String` leaves the next reader guessing whether it needs translating. `RescanScheduled.details` is the live case (the
frontend handler reads `reason` and resolves its own message key; `details` is console-only).

**Where an event goes.** `events/index_mapping.rs` app-side holds the 15 `tauri_specta::Event` payload structs and one
exhaustive `route(event, app)`. It EMITS as it decides and returns the wire name it emitted under (read off the
payload's own `Event::NAME`), so there's no second lookup table to drift from what actually ships. `route(event, None)`
suppresses the Tauri emit and nothing else, which is how the mapping is tested without an app.

The data types the payloads carry stay on this side: `ScanRunKind`, `RescanReason`, `ActivityPhase`,
`MemoryWatchdogAction`, `Freshness`, `AggregationPhase`, `MediaEnrichTerminalReason`, `IndexFailure`. A `specta::Type`
derive on a value is fine here; a presentation decision isn't.

## Response types and the debug ring (`mod.rs`)

`IndexStatusResponse`, `VolumeIndexStatus`, and `IndexDebugStatusResponse` are IPC RESPONSES, not events, so they stay
here with their serde shapes. `DebugStats` is the app-wide phase ring the debug window reads; `PhaseRecord.trigger` is a
free-text English line rendered only in that developer panel.

`RescanReason` lives here too: `StaleIndex`, `JournalGap`, `ReplayOverflow`, `WatcherStartFailed`,
`ReconcilerBufferOverflow`, `IncompletePreviousScan`, `WatcherChannelOverflow`, `IngestionBacklog`. Every path that
falls back to a full rescan reports one through `emit_rescan_notification`, which also logs the reason; the frontend
maps each to a toast. The triggers that CHOOSE each reason live in `../watch/DETAILS.md` and `../reconcile/DETAILS.md`.

Two reports that could look like they belong here but don't: `AggregationProgress` is the writer's
(`../writer/DETAILS.md`), and `SearchIndexReadyEvent` (`search-index-ready`) is `commands/search.rs`'s.

## `ScanRunKind` — what kind of run this is (`payload.rs`)

A serde `snake_case` specta enum shipped on `index-scan-started` (and, for a mid-scan reload, on
`IndexStatusResponse.scan_run_kind`) so the frontend states the run instead of inferring it:

- **`FirstScan`**: no prior completed scan's calibration, so the index is built from nothing.
- **`FullRebuild`**: an existing index truncated and re-walked. Folder sizes go blank for the whole run.
- **`ChangeCheck`**: the rescan-in-place that diffs each directory and writes only changes. The last-good folder sizes
  stay on screen (stale) throughout, and the run is roughly 5x slower per entry.

`ScanRunKind::classify(reconciles_in_place, prior_total_entries)` is the whole derivation, called at both scan-start
funnels (`lifecycle/manager.rs` for the local walker, `lifecycle/network_scan.rs` for the trait scan) right after each
decides reconcile-vs-truncate. The frontend previously guessed from `prior_total_entries` alone, which disagrees on a
populated index whose last scan never completed: that one truncates, so it's a rebuild, not a change check.

`calibration_kind()` maps the run onto its ETA-calibration bucket (`store::ScanCalibrationKind`): the first scan and the
full rebuild run the SAME walker so they share `FullWalk`, and only `ChangeCheck` gets its own. The buckets and the
same-kind-then-any-kind fallback live in `../store/DETAILS.md`.

## `set_phase_for` — the two phase records (`mod.rs`)

There are TWO records of the top-level pipeline phase (`Scanning → Aggregating → Reconciling → Live`, plus `Replaying` /
`Idle`), and they answer different questions:

- **Global, app-wide**: `DEBUG_STATS.set_phase()` appends to one `PhaseRecord` ring (capped at 20) that the debug
  window's "Phase timeline" reads. It's a singleton: under two concurrent volumes it interleaves their transitions and
  can't say WHICH drive changed. Debug-only; keep it.
- **Per-volume**: the `IndexEvent::PhaseChanged { volume_id, phase }` report tells the frontend which drive moved to
  which phase, driving the per-volume step checklist. `ActivityPhase` (Replaying/Scanning/Aggregating/
  Reconciling/Live/Idle) is a serde `snake_case` specta enum, so the FE branches on the typed variant, no
  string-matching on labels.

`set_phase_for(events, volume_id, phase, trigger)` (a `pub(super)` fn) does BOTH in one call — the global ring plus a
fire-and-forget per-volume report — so the two can't drift. Every `set_phase` site where a `volume_id` and a sink are in
scope goes through it: `lifecycle/manager.rs` (local `Replaying`/`Scanning`, the completion task's
`Aggregating → Reconciling → Live`, `Idle` in stop/shutdown), `lifecycle/network_scan.rs` (`Scanning` at start; `Live`
on clean finish, `Idle` on disconnect), `lifecycle/scan_completion.rs`, and `watch/event_loop/replay.rs` (`Live` at the
end of replay). Spawned tasks capture a cloned sink / `volume_id`, never re-resolving the manager in the registry (same
discipline as the freshness `Arc`).

The event fires only on TRANSITIONS, so a frontend that joins mid-scan (window reload) can't learn the current phase
from it. The FE backfills observable steps from the scan/aggregation activity it already receives; the reconcile step is
the one transition with no other signal, so it's briefly unobservable after a reload that lands mid-reconcile (accepted,
rare). `VolumeIndexStatus` deliberately does NOT carry a current phase: it isn't stored per-volume (only in the global
`DEBUG_STATS`), so exposing it would mean threading a new per-instance phase handle through the spawned completion tasks
— lifecycle complexity the brief reconcile gap doesn't justify.

**Network-scan honesty.** SMB/MTP emit only `Scanning → Live` (no distinct `Aggregating` / `Reconciling` phase), yet the
writer still runs aggregation and emits its per-volume sub-phase events (`loading → sorting → computing → writing`). So
the FE drives the "compute folder sizes" step off the aggregation events, not a top-level phase network never sends; and
`saving_entries` never fires for network (entries insert inline during the walk), so that step simply doesn't appear.
Don't fake either by calling local-only helpers on the network path.

The scan-progress pump that used to live here is `../lifecycle/DETAILS.md` § "`ScanProgressReporter`". It drives a scan
rather than describing one, and its reach into `scanner`, `writer`, and `paths` is what kept this subtree inside a
dependency cycle.
