# Indexing events

The `EventSink` seam, the typed `IndexEvent` every index subsystem reports through, the values those events carry, and
the phase-transition emitter.

## Must-knows

- **This subtree is cycle-free, and it stays that way by importing DOWN only** (`store`, `aggregator`,
  `lifecycle::freshness`). ❌ Never import a sideways index area from here. A scan-progress pump used to live here and
  reached into `scanner`, `writer`, and `paths`; that's why it's now `../lifecycle/progress_reporter.rs`.
- **A new value an event carries goes in `payload.rs`, NOT in `mod.rs`.** `mod.rs` holds the IPC response types and the
  debug ring, and it names the same enums — so an enum added there makes `sink.rs` import its own parent, which is the
  cycle this module just came out of. If both an event and a response need it, it belongs below both.

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
- **Both phase events fire only on TRANSITIONS**, so a frontend that joins mid-scan (window reload) learns neither from
  them. The ACTIVITY phase is backfilled from scan/aggregation activity, and `VolumeIndexStatus` deliberately carries
  none; the drive's COVERAGE phase rides `IndexStatusResponse::coverage_phase` instead, because its last phase is the
  rest of the drive and a reloaded window would otherwise have no header until the run ended.
- **Network scans emit only `Scanning → Live`** (no distinct `Aggregating` / `Reconciling`), and `saving_entries` never
  fires for network (entries insert inline). Don't fake either by calling local-only helpers on the network path; the FE
  drives the "compute folder sizes" step off the aggregation events instead.
- **`ScanRunKind` on `ScanStarted` is the ONLY honest answer to "what kind of run is this"** (`FirstScan` /
  `FullRebuild` / `ChangeCheck`, from `ScanRunKind::classify` at each scan-start funnel). Don't let the FE re-derive it
  from `prior_total_entries`: that disagrees on a populated index whose last scan never completed. Its
  `calibration_kind()` also picks the per-kind ETA bucket (`../store/`).
- **The typed data an event carries stays here, the envelope doesn't.** The five `payload.rs` enums keep their
  `specta::Type` derives: a schema derive on a value is fine, a presentation decision isn't.

## Module map

- `payload.rs` — the five values an event carries: `ScanRunKind`, `CoveragePhase` (which is also the phase queue's
  ranking), `RescanReason`, `ActivityPhase`, `MemoryWatchdogAction`. The bottom of the subtree.
- `sink.rs` — `IndexEvent` + `IndexEventKind`, the `EventSink` trait, `NoopEventSink`, `Diagnostic`, `IndexErrorReport`,
  `MediaEnrichTerminalReason`, and the test `RecordingSink`.
- `mod.rs` — the IPC response types, `PhaseRecord`, `DebugStats`, `set_phase_for`, `emit_rescan_notification`, and
  `emit_dir_updated`.

Owned elsewhere: the scan-progress pump is `../lifecycle/progress_reporter.rs`; the freshness state machine and phase
lifecycle live in `../lifecycle/CLAUDE.md`; the writer-side aggregation events in `../writer/CLAUDE.md`; the rescan
triggers that pick each `RescanReason` in `../watch/CLAUDE.md` and `../reconcile/CLAUDE.md`.

The event catalog, the error-report variants, and `set_phase_for`: `DETAILS.md`. Read it before any non-trivial work
here: editing, planning, reorganizing, or advising.
