# Indexing (frontend)

Frontend bridge to the Rust drive indexer: reactive scan state, Tauri event listeners, and the drive-indexing status
indicator. Rust counterpart: `crates/cmdr-index/src/indexing/`.

`index-state.svelte.ts` is the reactive core (per-volume `SvelteMap`s fed by the ten index events);
`media-enrich-state.svelte.ts` is the second publisher. Pure helpers: `eta.ts`, `indexing-steps.ts`, `elapsed.ts`,
`media-enrich-queued.ts`. The status surface is `IndexingStatusIndicator` → `IndexingDriveRow` → presentational
`IndexingStatusBody` + `IndexingDriveSummary`, placed by `$lib/status-corner/`. Public API barrel: `index.ts`. Per-file
detail and the event tables: `DETAILS.md`.

## Must-knows

- **`$state` must live in `.svelte.ts`**, not plain `.ts` (Svelte runes).
- **`initIndexState` uses "listen first, then query"**: register listeners, THEN call `get_index_status`. The Rust
  indexer starts before the frontend mounts, so querying first races `index-scan-started` and the UI sticks on "not
  scanning". ❌ Don't reorder.
- **`index-state` is the SINGLE source of live activity**, keyed by `volumeId` (read via `getVolumeActivity`). ❌ Don't
  add a second live-count path: the badge's `drive-index-manager` owns ONLY freshness and menu facts.
- **A keyed entry is cleared by a TERMINAL event, ❌ never by freshness.** An aborting network scan fires no completion,
  so `index-scan-aborted` drops that volume's activity + aggregation, else the row sticks on "scanning".
- **The terminal PHASE (`live`/`idle`) is the backstop the others need**, clearing activity, aggregation, and walked
  ground beside the run-shape facts: both live streams outlive their own terminal event, and every late tick re-creates
  an entry nothing else would close again. ❌ `-scan-complete` clears only what the WALK owns; dropping
  `coveredInPhases`/`coveragePhase` there reverts the checklist to the bulk shape under a "First full scan" header.
- **Image indexing is a SECOND publisher here** (`media-enrich-state.svelte.ts`), on the same `index-state` discipline:
  `media-enrich-progress` drives a per-volume row, `media-enrich-terminal` clears or re-voices it paused. The corner
  gate ORs `isAnyVolumeEnriching()`.
- **Checklist STEPS are composed from the events that fire for THIS volume** (`deriveSteps`), ❌ never a fixed list, and
  ❌ branch on typed discriminants only. Per-step ETA only; NO overall ETA by design. The catch-up (reconcile) step has
  ONLY the `phase` event, so the visibility gate and the indicator must include `phase`-only volumes
  (`getActivePhaseVolumeIds`), or the surface vanishes the moment aggregation completes.
- **The run kind is the BACKEND's answer** (`ScanRunKind` off `index-scan-started`, stashed per volume), ❌ never
  guessed from the calibration numbers: they disagree on a populated index whose last scan never finished.
- **Scan progress has two tiers** (`computeScanProgress`): each tier uses a specific counter as BOTH numerator and ETA
  window sample. ❌ Don't mix them; swapping counter and denominator ships wrong ETAs.
- **`getEntriesScanned` stays `root`-only** (SearchDialog's index-build progress). Only the corner hourglass is global
  (`isAnyVolumeIndexing()`); ❌ don't reintroduce a global `isScanning()`.
- **A phased run's checklist is ONE step** (`IndexRunKind: 'phased'`): the other three never separately happen.
- **The indicator is a focusable, hoverable icon** (`role="img"`, `tabindex="0"`), ❌ not `pointer-events: none` and ❌
  not `role="status"` — a live region is wrong for a focusable hover target. Detail lives in its tooltip via
  `aria-describedby`.
- **Directory sizes are HONEST: unknown (`<dir>`) ≠ empty (`0 bytes`) ≠ lower-bound (`≥`).** `getDirSizeDisplayState`
  (`views/full-list-utils.ts`) is the single source of truth. The hourglass on top keys on GROUND BEING WALKED
  (`getWalkedGround` + `isPathAffectedByWalk`), ❌ never "the volume is scanning", and tests BOTH ways since the roll-up
  repairs ancestors. Every run announces its ground, so ❌ nothing here is seeded or branches on the kind of run, and ❌
  never write that map per progress tick.

Full public API, the ten-event table, the step model, tooltip content, ETA blending, honest-size rendering, and tests:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
