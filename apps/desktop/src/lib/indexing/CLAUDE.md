# Indexing (frontend)

Frontend bridge to the Rust drive indexer: reactive scan state, Tauri event listeners, and the drive-indexing status
indicator. Rust counterpart: `crates/cmdr-index/src/indexing/`.

`index-state.svelte.ts` is the reactive core (per-volume `SvelteMap`s fed by the ten index events);
`media-enrich-state.svelte.ts` is the second publisher. Pure helpers: `eta.ts`, `indexing-steps.ts`, `elapsed.ts`,
`media-enrich-queued.ts`, `walked-ground.ts`. The status surface is `IndexingStatusIndicator` → `IndexingDriveRow` →
presentational `IndexingStatusBody` + `IndexingDriveSummary`, placed by `$lib/status-corner/`. Public API barrel:
`index.ts`.

## Must-knows

- **`$state` must live in `.svelte.ts`**, not plain `.ts` (Svelte runes).
- **`initIndexState` listens FIRST, then queries.** The Rust indexer starts before the frontend mounts, so calling
  `get_index_status` ahead of the listeners races `index-scan-started` and the UI sticks on "not scanning". ❌ Don't
  reorder.
- **`index-state` is the SINGLE source of live activity**, keyed by `volumeId` (`getVolumeActivity`); the badge's
  `drive-index-manager` owns ONLY freshness and menu facts. ❌ Don't add a second live-count path or reintroduce a
  global `isScanning()`: only the corner hourglass is global (`isAnyVolumeIndexing()`), and `getEntriesScanned` stays
  `root`-only for the search dialog.
- **A keyed entry is cleared by a TERMINAL event, ❌ never by freshness**, and the terminal PHASE (`live` / `idle`) is
  the backstop the others need, since both live streams outlive their own terminal event. `-scan-complete` clears only
  what the WALK owns; dropping the run-shape facts there reverts the checklist to the bulk shape.
- **Image indexing is a SECOND publisher** (`media-enrich-state.svelte.ts`) on the same discipline, and the corner gate
  ORs `isAnyVolumeEnriching()`.
- **Checklist STEPS are composed from the events that fire for THIS volume** (`deriveSteps`), ❌ never a fixed list, and
  branch on typed discriminants only. Per-step ETA only; no overall ETA by design. A `phase`-only volume still counts
  (`getActivePhaseVolumeIds`), or the surface vanishes the moment aggregation completes, and a phased run's checklist is
  ONE step.
- **The run kind is the BACKEND's answer** (`ScanRunKind` off `index-scan-started`), ❌ never guessed from the
  calibration numbers: they disagree on a populated index whose last scan never finished.
- **Scan progress has two tiers** (`computeScanProgress`), each using one counter as BOTH numerator and ETA window
  sample. ❌ Don't mix them; swapping counter and denominator ships wrong ETAs.
- **The indicator is a focusable, hoverable icon** (`role="img"`, `tabindex="0"`), ❌ not `role="status"` and not
  `pointer-events: none`: a live region is wrong for a focusable hover target. Detail rides in its tooltip via
  `aria-describedby`.
- **Directory sizes are HONEST: unknown (`<dir>`) ≠ empty (`0 bytes`) ≠ lower-bound (`≥`)**, decided by
  `getDirSizeDisplayState` (`views/full-list-utils.ts`). The hourglass on top keys on GROUND BEING WALKED
  (`getWalkedGround` + `isPathAffectedByWalk`), ❌ never "the volume is scanning", tested BOTH ways since the roll-up
  repairs ancestors. Every run announces its own ground, so nothing here seeds that map or branches on the kind of run.

Full public API, the ten-event table, the step model, tooltip content, ETA blending, honest-size rendering, and tests:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
