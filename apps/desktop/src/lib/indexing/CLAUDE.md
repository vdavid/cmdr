# Indexing (frontend)

Frontend bridge to the Rust drive indexer: reactive scan state, Tauri event listeners, and the drive-indexing status
indicator. Rust counterpart: `apps/desktop/src-tauri/src/indexing/`.

## Module map

- **`index.ts`**: public API barrel.
- **`index-state.svelte.ts`**: per-volume `SvelteMap` of activity (scan/replay) keyed by `volumeId` + a global
  aggregation signal; reacts to the Tauri index events.
- **`index-events.ts`**: listens for `index-dir-updated`, calls back with updated paths.
- **`eta.ts`**: pure ETA helpers + `computeScanProgress` (two-tier scan fraction).
- **`IndexingStatusIndicator.svelte`** + **`IndexingDriveRow.svelte`**: top-right hourglass shown whenever ANY drive is
  indexing; tooltip lists one row per active drive (each row owns its ETA window).
- **`drive-index-prefs.ts`**: FE-OWNED persisted prefs the backend never reads: per-drive "don't ask again" silences
  (D6) and the one-time stale-dialog flag (D2), stored as hidden settings.
- **`first-connect-trigger.ts`** + **`FirstConnectIndexToastContent.svelte`**: the first-connect "index this drive?"
  prompt (D6), shown once per session per new external drive, self-gated on settings + silence + already-indexed.
- **`StaleDriveDialog.svelte`**: the one-time "your drive went stale" dialog (D2), mounted once in `+page.svelte`,
  subscribes to `index-freshness-changed`, fires on the first external Fresh→Stale edge (gated on
  `indexing.staleNotify`).

## Must-knows

- **`$state` must live in `.svelte.ts`**, not plain `.ts` (Svelte runes). `index-state.svelte.ts` is allowlisted in
  `coverage-allowlist.json` (event-driven module `$state`).
- **`initIndexState` uses "listen first, then query"**: register event listeners, THEN call `get_index_status`. The Rust
  indexer starts in `setup()` before the frontend mounts, so querying first leaves a race where `index-scan-started`
  fires between query and listener registration and the UI sticks on "not scanning". Don't reorder.
- **`get_index_status` backfill (root-only) recovers tier inputs after a mid-scan reload**, but `scanStartedAt` can't
  cross IPC so it stays 0 (ETA degrades until the window fills — accepted). Tier-1 reads prior scan totals from nested
  `indexStatus` meta via `parseMetaNumber`. Mechanics: DETAILS.md.
- **Scan progress has two tiers** (`computeScanProgress`). Each tier uses a specific counter as both the numerator and
  the ETA window sample — don't mix them (swapping counter and denominator ships wrong ETAs). Details and clamping
  values: DETAILS.md.
- **The indicator tracks ALL drives** via a per-volume `activity` map keyed by `volumeId`; aggregation carries NO
  `volumeId`, so it's attributed to the last scan to complete (default `root`). Don't assume aggregation is root's.
  State model, attribution, and the API: [DETAILS.md](DETAILS.md).
- **Don't widen `getEntriesScanned` to "any volume"**: it reports `root` on purpose (SearchDialog reads it as local
  index-build progress). `isScanning`/`isAggregating` are the "any volume" booleans.
- **The indicator is a focusable, hoverable icon** (`role="img"`, `tabindex="0"`), not `pointer-events: none`: the
  detail lives in a tooltip reached by hover or focus. Don't use `role="status"` (a live region — wrong for a focusable
  hover target); the tooltip carries the live label + ETA via `aria-describedby`.
- **`index-dir-updated` callbacks get a batch of paths** (multiple during DB replay, typically one in live FS-watch).
  `DualPaneExplorer` checks each against each pane's current dir with a path-prefix comparison (relies on trailing-slash
  normalization).
- **The `IndexingStatusIndicator.a11y.test.ts` mock must stub the whole `index-state.svelte` API the indicator imports
  AND `$lib/stores/volume-store.svelte` `getVolumes`** (the indicator resolves drive names through it), or a case
  crashes on `undefined`.
- **Directory sizes are HONEST: unknown (the `<dir>` placeholder) ≠ empty (`0 bytes`) ≠ lower-bound (`≥`).**
  `getDirSizeDisplayState` (`views/full-list-utils.ts`) is the single source of truth, consumed in lockstep by
  `FullList` / `SelectionInfo` / `measure-column-widths`. Rendering + sort: [DETAILS.md](DETAILS.md).

Full public API, the eight-event table, tooltip content per state, ETA blending, dependencies, and tests:
[DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
