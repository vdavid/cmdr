# Indexing events

The `EventSink` seam, the typed `IndexEvent` every index subsystem reports through, the values those events carry, and
the phase-transition emitter.

## Must-knows

- **This subtree is cycle-free, and stays that way by importing DOWN only** (`store`, `aggregator`,
  `lifecycle::freshness`). ❌ Never a sideways index area. A scan-progress pump that reached into `scanner`, `writer`,
  and `paths` is why `../lifecycle/progress_reporter.rs` exists.
- **A new value an event carries goes in `payload.rs`, NOT in `mod.rs`.** `mod.rs` holds the IPC response types and the
  debug ring, and it names the same enums — so an enum added there makes `sink.rs` import its own parent, which is the
  cycle this module just came out of. If both an event and a response need it, it belongs below both. A value keeps its
  `specta::Type` derive there: a schema derive on data is fine, a presentation decision isn't.
- **This area produces no wire format and no user-facing words.** A subsystem emits a typed `IndexEvent`; the app's
  `events/index_mapping.rs` owns the Tauri payloads, the kebab event names, and every sentence a human reads. Adding a
  frontend event means a variant here AND an arm there AND a `collect_events!` registration in `ipc.rs`. The `route`
  match is exhaustive, so the compiler catches the middle one.
- **`IndexEvent::Error` and `IndexEvent::PathAccessDenied` are not frontend events**: the app raises them through
  `log_error!` (the shipped error-report pipeline) and `restricted_paths::record_denial`. They're events because a
  subsystem can't invoke a crate-root macro. ❌ Don't "simplify" them into a `log::error!` — that silently drops the
  feedback loop.
- **`IndexEvent::FolderActivity` is the one variant a drop costs SIGNAL, not just a UI update.** It carries per-folder
  change rollups a host reads to notice things. Acceptable (the folder will change again), and said at the variant so
  the `EventSink` contract isn't silently violated.
- **All top-level phase transitions go through `set_phase_for(events, volume_id, phase, trigger)`**, never
  `DEBUG_STATS.set_phase`. It does BOTH in one call — the global phase ring AND the per-volume phase-changed report — so
  the two can't drift. Spawned tasks capture a cloned sink / `volume_id`, never re-resolving the manager.
- **Both phase events fire only on TRANSITIONS**, so a window reload learns neither from them. The ACTIVITY phase is
  backfilled from scan/aggregation activity (`VolumeIndexStatus` carries none, deliberately); the COVERAGE phase rides
  `IndexStatusResponse::coverage_phase`, since its last phase runs to the end of the run.
- **Network scans emit only `Scanning → Live`** (no `Aggregating` / `Reconciling`), and `saving_entries` never fires
  there (entries insert inline). Don't fake either with local-only helpers; the FE drives its "compute folder sizes"
  step off the aggregation events.
- **`ScanRunKind` on `ScanStarted` is the ONLY honest answer to "what kind of run is this"** (from
  `ScanRunKind::classify` at each scan-start funnel). Don't let the FE re-derive it from `prior_total_entries`: that
  disagrees on a populated index whose last scan never completed. Its `calibration_kind()` also picks the per-kind ETA
  bucket (`../store/`).

## Module map

- `payload.rs` — the values an event carries: `ScanRunKind`, `CoveragePhase` (also the phase queue's ranking),
  `RescanReason`, `ActivityPhase`, `MemoryWatchdogAction`, and `FolderChangeRollup` (the only one with no `serde` /
  `specta::Type`, since it rides into host machinery rather than onto a wire). The bottom of the subtree.
- `sink.rs` — `IndexEvent` + `IndexEventKind`, the `EventSink` trait, `NoopEventSink`, `Diagnostic`, `IndexErrorReport`,
  `MediaEnrichTerminalReason`, and the test `RecordingSink`.
- `mod.rs` — the IPC response types, `PhaseRecord`, `DebugStats`, `set_phase_for`, `emit_rescan_notification`, and
  `emit_dir_updated`.

Owned elsewhere: the scan-progress pump (`../lifecycle/progress_reporter.rs`); freshness and the phase lifecycle
(`../lifecycle/CLAUDE.md`); writer-side aggregation events (`../writer/CLAUDE.md`); the rescan triggers that pick each
`RescanReason` (`../watch/CLAUDE.md`, `../reconcile/CLAUDE.md`).

The event catalog, the error-report variants, and `set_phase_for`: `DETAILS.md`. Read it before any non-trivial work
here: editing, planning, reorganizing, or advising.
