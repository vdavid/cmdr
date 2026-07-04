# Indexing (frontend)

Frontend bridge to the Rust drive indexer: reactive scan state, Tauri event listeners, and the drive-indexing status
indicator. Rust counterpart: `apps/desktop/src-tauri/src/indexing/`.

## Module map

`index-state.svelte.ts` is the reactive core — per-volume `SvelteMap`s (activity, aggregation, and the pipeline `phase`
via `getVolumePhase`) fed by the Tauri index events; `index-events.ts` bridges `index-dir-updated`. Pure helpers:
`eta.ts` (+ `computeScanProgress`), `indexing-steps.ts` (`deriveSteps`), `elapsed.ts`. The status surface
(`IndexingStatusIndicator` → the `IndexingDriveRow` wrapper → the presentational `IndexingStatusBody` +
`IndexingDriveSummary`) is the top-right hourglass rendering the per-drive step checklist; the breadcrumb badge reuses
`IndexingDriveRow`. Prompts (FE-owned): `first-connect-trigger.ts` + `FirstConnectIndexToastContent`,
`StaleDriveDialog.svelte`, `drive-index-prefs.ts`. Public API barrel: `index.ts`. Per-file detail + the ten-event table:
DETAILS.md or `codegraph_search`.

## Must-knows

- **`$state` must live in `.svelte.ts`**, not plain `.ts` (Svelte runes). `index-state.svelte.ts` is allowlisted in
  `coverage-allowlist.json` (event-driven module `$state`).
- **`initIndexState` uses "listen first, then query"**: register event listeners, THEN call `get_index_status`. The Rust
  indexer starts in `setup()` before the frontend mounts, so querying first races `index-scan-started` and the UI sticks
  on "not scanning". Don't reorder.
- **`index-state` is the SINGLE source of live activity** (scan/replay counters + aggregation), keyed by `volumeId`.
  Read a volume's activity via `getVolumeActivity(volumeId)`. Don't reintroduce a second live-count path — the
  breadcrumb badge's `drive-index-manager` (in `navigation/`) owns ONLY freshness/menu facts, never live progress.
- **A keyed entry is cleared by a TERMINAL event, never by freshness** (`index-scan-complete` / `index-replay-complete`
  / `index-aggregation-complete`). A network (SMB/MTP) scan that aborts fires no completion, so the backend emits
  `index-scan-aborted { volumeId }` and `index-state` drops that volume's activity + aggregation — else an aborted
  network scan leaves a stuck "scanning" row. Don't clear activity off `index-freshness-changed` (not subscribed here).
- **Checklist STEPS are composed from the events that fire for THIS volume** (`deriveSteps`), never a fixed list: a
  network scan omits Save and Catch-up; a roll-on collapses to one Update step. Branch on typed discriminants only.
  Per-step ETA only; NO overall ETA by design (deferred — `docs/specs/later/drive-index-overall-eta.md`). The catch-up
  (reconcile) step has ONLY the `phase` event, so the visibility gate and the indicator/badge must include `phase`-only
  volumes (`getActivePhaseVolumeIds`), or the surface vanishes the moment aggregation completes. Full model: DETAILS §
  Step checklist.
- **Scan progress has two tiers** (`computeScanProgress`): each tier uses a specific counter as BOTH numerator and ETA
  window sample — don't mix them (swapping counter and denominator ships wrong ETAs). Tiers + clamps: DETAILS.md.
- **`getEntriesScanned` stays `root`-only** (SearchDialog reads it as local index-build progress). The per-folder size
  hourglass is PER-VOLUME (`isVolumeScanning(volumeId)` / `isVolumeAggregating(volumeId)`), so a scan on drive B never
  flags drive A. No global scanning boolean; only the corner hourglass is global (`isAnyVolumeIndexing()`). Don't
  reintroduce a global `isScanning()`.
- **The indicator is a focusable, hoverable icon** (`role="img"`, `tabindex="0"`), not `pointer-events: none`; the
  detail lives in a tooltip reached by hover or focus. Don't use `role="status"` (a live region — wrong for a focusable
  hover target); the tooltip carries the live label + ETA via `aria-describedby`.
- **Directory sizes are HONEST: unknown (the `<dir>` placeholder) ≠ empty (`0 bytes`) ≠ lower-bound (`≥`).**
  `getDirSizeDisplayState` (`views/full-list-utils.ts`) is the single source of truth, consumed in lockstep by
  `FullList` / `SelectionInfo` / `measure-column-widths`. Rendering + sort: DETAILS.md.

Full public API, the ten-event table, the step model, tooltip content per state, ETA blending, honest-size rendering,
dependencies, and tests: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
