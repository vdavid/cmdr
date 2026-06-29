# Indexing (frontend)

Frontend bridge to the Rust drive indexer: reactive scan state, Tauri event listeners, and the drive-indexing status
indicator. Rust counterpart: `apps/desktop/src-tauri/src/indexing/`.

## Module map

- **`index.ts`**: public API barrel.
- **`index-state.svelte.ts`**: per-volume `SvelteMap`s keyed by `volumeId` — activity (scan/replay), aggregation, and
  the top-level pipeline `phase` (`getVolumePhase`, fed by `index-phase-changed`, for the step checklist); reacts to the
  Tauri index events.
- **`index-events.ts`**: listens for `index-dir-updated`, calls back with updated paths.
- **`eta.ts`**: pure ETA helpers + `computeScanProgress` (two-tier scan fraction).
- **`indexing-steps.ts`**: pure, unit-tested step-checklist derivation (`deriveSteps`) + the step/sub-phase label maps.
- **`elapsed.ts`**: pure `formatElapsedClock` (`m:ss`, `null` under 1s) — the first-scan elapsed clock, used by the
  shared `IndexingStatusBody` (so both the corner indicator and the badge tooltip show it from one source).
- **The status surface** (`IndexingStatusIndicator` / `IndexingDriveRow` / `IndexingStatusBody` /
  `IndexingDriveSummary`): the top-right hourglass shown whenever ANY drive is indexing. `IndexingStatusBody` is the
  shared PRESENTATIONAL per-volume step checklist; `IndexingDriveRow` the thin WRAPPER (heading + body + ETA windows + 1
  Hz tick). The corner expands the primary drive and collapses each secondary to a one-line `IndexingDriveSummary`; the
  breadcrumb badge renders the same `IndexingDriveRow`. One representation everywhere. DETAILS § Step checklist.
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
- **The indicator tracks ALL drives** via a per-volume `activity` map keyed by `volumeId`. Aggregation is per-volume too
  (its own map). State model, attribution, and the API: [DETAILS.md](DETAILS.md).
- **`index-state` is the SINGLE source of live activity** (scan/replay counters + aggregation), keyed by `volumeId`. The
  breadcrumb badge reads its own volume's via `getVolumeActivity(volumeId)` to render the shared body; the badge's
  `drive-index-manager` owns ONLY freshness/menu facts (the dot color, last-scan facts), never live progress. Don't
  reintroduce a second live-count path.
- **Checklist STEPS are composed from the events that fire for THIS volume** (`deriveSteps`), never a fixed list: a
  network (SMB/MTP) scan omits Save and Catch-up; a roll-on collapses to one Update step. Branch on typed discriminants
  only. Per-step ETA only; NO overall ETA by design (deferred — `docs/specs/later/drive-index-overall-eta.md`). The
  catch-up (reconcile) step has ONLY the `phase` event, so the visibility gate (`isAnyVolumeIndexing`) and the
  indicator/badge must include `phase`-only volumes (`getActivePhaseVolumeIds`) or the surface vanishes the moment
  aggregation completes and the step never shows. Full step model + composition: DETAILS § Step checklist.
- **A keyed entry is cleared by a TERMINAL event**, never by freshness. Scan → `index-scan-complete`; replay →
  `index-replay-complete`; aggregation → `index-aggregation-complete`. A network (SMB/MTP) scan that aborts
  (disconnect/cancel/timeout) fires NO completion, so the backend emits `index-scan-aborted { volumeId }` and
  `index-state` removes that volume's activity + aggregation on it — without it, an aborted network scan leaves a stuck
  "scanning" row. Don't clear activity off `index-freshness-changed` (it's not subscribed here).
- **`getEntriesScanned` stays `root`-only** (SearchDialog reads it as local index-build progress). The per-folder size
  hourglass is PER-VOLUME — `isVolumeScanning(volumeId)` / `isVolumeAggregating(volumeId)` on the folder's own volume,
  so a scan on drive B never flags drive A. No global scanning boolean exists; only the corner hourglass is global
  (`isAnyVolumeIndexing()`). Don't reintroduce a global `isScanning()`.
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

Full public API, the ten-event table, the step model + per-volume composition, tooltip content per state, ETA blending,
dependencies, and tests: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
