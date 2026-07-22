# Indexing events + progress surface

The frontend-facing event payloads, the phase-transition emitter, and the scan-progress tick loop (progress events plus
mid-scan partial aggregation) shared by every scan path.

## Must-knows

- **All top-level phase transitions go through `set_phase_for(app, volume_id, phase, trigger)`**, never
  `DEBUG_STATS.set_phase` directly. It does BOTH in one call — the global phase ring AND the per-volume
  `index-phase-changed` emit — so the two can't drift. Spawned tasks capture cloned `app` / `volume_id`, never
  re-resolving the manager in the registry.
- **The phase event fires only on TRANSITIONS**, so a frontend that joins mid-scan (window reload) can't learn the
  current phase from it (the FE backfills from scan/aggregation activity). `VolumeIndexStatus` deliberately carries no
  current phase.
- **Network scans emit only `Scanning → Live`** (no distinct `Aggregating` / `Reconciling`), and `saving_entries` never
  fires for network (entries insert inline). Don't fake either by calling local-only helpers on the network path; the
  FE drives the "compute folder sizes" step off the aggregation events instead.
- **`partial_agg` helpers are pure and side-effect-free** so the timer loop stays a dumb caller. `should_send_partial_agg`
  fires every `PARTIAL_AGG_TICK_INTERVAL`-th tick (10 = 5 s), never on tick 0, skips when `queue_depth >
  PARTIAL_AGG_MAX_QUEUE_DEPTH` (4,000). `collect_hot_paths` keeps only listings whose `volume_id` matches the scanned
  volume (else they'd resolve against the wrong per-volume DB), firmlink-normalizes, and dedups.
- **The reporter runs on `tauri::async_runtime::spawn`, not `tokio::spawn`** — a scan can start from the synchronous
  Tauri `setup()` hook where no Tokio runtime exists. Its loop dies with the scan, which is what structurally scopes
  partial passes to the full-scan window (no partial passes in replay, subtree scans, or live mode).
- **Event structs derive `tauri_specta::Event` with a pinned kebab `event_name`** (the `…Event` suffix wouldn't
  kebab-case to the wire string) and are registered in `ipc.rs`'s `collect_events!`. Add a new event in both places or
  the FE never sees it.

## Module map

- `mod.rs` — the Tauri event payload structs + `RescanReason`, `ActivityPhase`, `PhaseRecord`, `DebugStats`, and
  `set_phase_for`.
- `progress_reporter.rs` — `ScanProgressReporter`, the 500 ms tick loop shared by all scan paths.
- `partial_agg.rs` — the pure send-decision (`should_send_partial_agg`) and hot-path collection (`collect_hot_paths`).

Owned elsewhere: the freshness state machine and phase lifecycle live in `../lifecycle/CLAUDE.md`; the writer-side
`ComputePartialAggregates` handler + aggregation events in `../writer/CLAUDE.md`; the `index_read_path` mapping the
reporter uses in `../paths/CLAUDE.md`; the rescan triggers that pick each `RescanReason` in `../watch/CLAUDE.md` and
`../reconcile/CLAUDE.md`.

The payload catalog, `set_phase_for`, the progress reporter, and partial aggregation: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
