# Indexing events + progress surface

The `EventSink` seam, the typed `IndexEvent` every index subsystem reports through, the phase-transition emitter, and
the scan-progress tick loop (progress plus mid-scan partial aggregation) shared by every scan path.

## Must-knows

- **This area produces no wire format and no user-facing words.** A subsystem emits a typed `IndexEvent`; the app's
  `events/index_mapping.rs` owns the Tauri payloads, the kebab event names, and every sentence a human reads. Adding a
  frontend event means a variant here AND an arm there AND a `collect_events!` registration in `ipc.rs`. The `route`
  match is exhaustive, so the compiler catches the middle one.
- **`IndexEvent::Error` and `IndexEvent::PathAccessDenied` are not frontend events**: the app raises them through
  `log_error!` (the shipped error-report pipeline) and `restricted_paths::record_denial`. They're events because a
  subsystem can't invoke a crate-root macro. ❌ Don't "simplify" them into a `log::error!` — that silently drops the
  feedback loop.
- **All top-level phase transitions go through `set_phase_for(events, volume_id, phase, trigger)`**, never
  `DEBUG_STATS.set_phase` directly. It does BOTH in one call — the global phase ring AND the per-volume phase-changed
  report — so the two can't drift. Spawned tasks capture a cloned sink / `volume_id`, never re-resolving the manager in
  the registry.
- **The phase event fires only on TRANSITIONS**, so a frontend that joins mid-scan (window reload) can't learn the
  current phase from it (the FE backfills from scan/aggregation activity). `VolumeIndexStatus` deliberately carries no
  current phase.
- **Network scans emit only `Scanning → Live`** (no distinct `Aggregating` / `Reconciling`), and `saving_entries` never
  fires for network (entries insert inline). Don't fake either by calling local-only helpers on the network path; the FE
  drives the "compute folder sizes" step off the aggregation events instead.
- **`partial_agg` helpers stay pure and side-effect-free** so the timer loop is a dumb caller. `collect_hot_paths` keeps
  only listings on the scanned volume (else they resolve against the wrong per-volume DB). Constants and cadence:
  `DETAILS.md`.
- **The reporter runs on `host::runtime::spawn`, not `tokio::spawn`** — a scan can start from the synchronous Tauri
  `setup()` hook where no Tokio runtime exists. Its loop dies with the scan, which structurally scopes partial passes to
  the full-scan window. It SLEEPS 500 ms before its first tick, so a small fixture scan can finish without ever
  reporting progress; don't write a test that assumes one fired.
- **`ScanRunKind` on `ScanStarted` is the ONLY honest answer to "what kind of run is this"** (`FirstScan` /
  `FullRebuild` / `ChangeCheck`, from `ScanRunKind::classify` at each scan-start funnel). Don't let the FE re-derive it
  from `prior_total_entries`: that disagrees on a populated index whose last scan never completed. Its
  `calibration_kind()` also picks the per-kind ETA bucket (`../store/`).
- **The typed data an event carries stays here, the envelope doesn't.** `ScanRunKind`, `RescanReason`, `ActivityPhase`,
  and `MemoryWatchdogAction` keep their `specta::Type` derives: a schema derive on a value is fine, a presentation
  decision isn't.

## Module map

- `sink.rs` — `IndexEvent` + `IndexEventKind`, the `EventSink` trait, `NoopEventSink`, `Diagnostic`, `IndexErrorReport`,
  and the test `RecordingSink`.
- `mod.rs` — the shared payload data types (`ScanRunKind`, `RescanReason`, `ActivityPhase`, `MemoryWatchdogAction`), the
  IPC response types, `PhaseRecord`, `DebugStats`, `set_phase_for`, `emit_rescan_notification`, and `emit_dir_updated`.
- `progress_reporter.rs` — `ScanProgressReporter`, the 500 ms tick loop shared by all scan paths.
- `partial_agg.rs` — the pure send-decision (`should_send_partial_agg`) and hot-path collection (`collect_hot_paths`).

Owned elsewhere: the freshness state machine and phase lifecycle live in `../lifecycle/CLAUDE.md`; the writer-side
`ComputePartialAggregates` handler + aggregation events in `../writer/CLAUDE.md`; the `index_read_path` mapping the
reporter uses in `../paths/CLAUDE.md`; the rescan triggers that pick each `RescanReason` in `../watch/CLAUDE.md` and
`../reconcile/CLAUDE.md`.

The payload catalog, `set_phase_for`, the progress reporter, and partial aggregation: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
