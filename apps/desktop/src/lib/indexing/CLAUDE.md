# Indexing (frontend)

Frontend bridge to the Rust drive indexer: reactive scan state, Tauri event listeners, and the drive-indexing status
indicator. Rust counterpart: `crates/cmdr-index/src/indexing/`.

## Module map

`index-state.svelte.ts` is the reactive core — per-volume `SvelteMap`s (activity, aggregation, the pipeline `phase` via
`getVolumePhase`, and the run kind via `getVolumeScanRunKind`) fed by the Tauri index events; `index-events.ts` bridges
`index-dir-updated`. Pure helpers: `eta.ts` (+ `computeScanProgress`), `indexing-steps.ts` (`deriveSteps`,
`deriveRunLabel`), `elapsed.ts`, `media-enrich-queued.ts`. The status surface (`IndexingStatusIndicator` →
`IndexingDriveRow` wrapper → presentational `IndexingStatusBody` + `IndexingDriveSummary`) is the hourglass's per-drive
step checklist, placed by `$lib/status-corner/`; the breadcrumb badge reuses `IndexingDriveRow`.
`media-enrich-state.svelte.ts` + `IndexingEnrichRow.svelte`: image indexing (2nd publisher, below). Prompts (FE-owned):
`first-connect-trigger.ts` + `FirstConnectIndexToastContent`, `StaleDriveDialog.svelte`, `drive-index-prefs.ts`. Public
API barrel: `index.ts`. Per-file detail + the event tables: DETAILS.md or `codegraph_search`.

## Must-knows

- **`$state` must live in `.svelte.ts`**, not plain `.ts` (Svelte runes).
- **`initIndexState` uses "listen first, then query"**: register event listeners, THEN call `get_index_status`. The Rust
  indexer starts in `setup()` before the frontend mounts, so querying first races `index-scan-started` and the UI sticks
  on "not scanning". Don't reorder.
- **`index-state` is the SINGLE source of live activity** (scan/replay counters + aggregation), keyed by `volumeId`;
  read via `getVolumeActivity(volumeId)`. Don't add a second live-count path: the badge's `drive-index-manager` (in
  `navigation/`) owns ONLY freshness/menu facts, never live progress.
- **A keyed entry is cleared by a TERMINAL event, never by freshness** (`index-scan-complete` / `-replay-complete` /
  `-aggregation-complete`). An aborting network scan fires no completion, so `index-scan-aborted { volumeId }` drops
  that volume's activity + aggregation — else it leaves a stuck "scanning" row. Don't clear off
  `index-freshness-changed` (not subscribed here).
- **Image indexing is a SECOND publisher here** (`media-enrich-state.svelte.ts`): `media-enrich-progress` drives a
  per-volume row, `media-enrich-terminal` clears or re-voices it paused. The corner gate ORs `isAnyVolumeEnriching()`
  (active only). Same `index-state` discipline. DETAILS § Image-enrichment publisher.
- **Checklist STEPS are composed from the events that fire for THIS volume** (`deriveSteps`), never a fixed list: a
  network scan omits Save and Catch-up; a roll-on collapses to one Update step. Branch on typed discriminants only.
  Per-step ETA only; NO overall ETA by design (deferred — `docs/specs/later/indexing/drive-index-overall-eta.md`). The
  catch-up (reconcile) step has ONLY the `phase` event, so the visibility gate and the indicator/badge must include
  `phase`-only volumes (`getActivePhaseVolumeIds`), or the surface vanishes the moment aggregation completes. Full
  model: DETAILS § Step checklist.
- **The run kind is the BACKEND's answer** (`ScanRunKind` off `index-scan-started`, stashed per volume), never guessed
  from the calibration numbers: they disagree on a populated index whose last scan never finished. It picks the header,
  the second step's wording, and the find-files hint. DETAILS § Run-kind header.
- **Scan progress has two tiers** (`computeScanProgress`): each tier uses a specific counter as BOTH numerator and ETA
  window sample — don't mix them (swapping counter and denominator ships wrong ETAs). Tiers + clamps: DETAILS.md.
- **`getEntriesScanned` stays `root`-only** (SearchDialog's index-build progress). Only the corner hourglass is global
  (`isAnyVolumeIndexing()`); don't reintroduce a global `isScanning()`.
- **A phased run's checklist is ONE step** (`IndexRunKind: 'phased'`): the other three never separately happen.
- **The indicator is a focusable, hoverable icon** (`role="img"`, `tabindex="0"`), not `pointer-events: none`; detail
  lives in a hover/focus tooltip carrying the live label + ETA via `aria-describedby`. Not `role="status"` (a live
  region is wrong for a focusable hover target).
- **Directory sizes are HONEST: unknown (`<dir>`) ≠ empty (`0 bytes`) ≠ lower-bound (`≥`).** `getDirSizeDisplayState`
  (`views/full-list-utils.ts`) is the single source of truth. The hourglass on top keys on GROUND BEING WALKED
  (`getWalkedGround` + `isPathAffectedByWalk`), ❌ never "the volume is scanning" (a phased index scans for minutes
  while one branch moves), and tests BOTH ways since the roll-up repairs ancestors. Both travel in lockstep through
  `FullList` / `BriefList` / `SelectionInfo` / `measure-column-widths`; ❌ never write that map per progress tick.
  Rendering + sort: DETAILS.md.

Full public API, the ten-event table, the step model, per-state tooltip content, ETA blending, honest-size rendering,
dependencies, and tests: `DETAILS.md`. Read it before any non-trivial work here.
