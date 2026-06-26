# Indexing (frontend)

Frontend bridge to the Rust drive indexer: reactive scan state, Tauri event listeners, and the drive-indexing status
indicator. Rust counterpart: `apps/desktop/src-tauri/src/indexing/`.

## Module map

- **`index.ts`**: public API barrel.
- **`index-state.svelte.ts`**: module-level `$state` for scan/aggregation/replay progress; reacts to eight Tauri events.
- **`index-events.ts`**: listens for `index-dir-updated`, calls back with updated paths.
- **`eta.ts`**: pure ETA helpers + `computeScanProgress` (two-tier scan fraction).
- **`IndexingStatusIndicator.svelte`**: top-right hourglass icon with a rich tooltip (scan / aggregation / replay).
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
- **`initIndexState` uses "listen first, then query"**: register the event listeners, THEN call `get_index_status`. The
  Rust indexer starts in Tauri's `setup()` hook before the frontend mounts, so querying first would leave a race window
  where `index-scan-started` fires between the query and listener registration and the UI sticks on "not scanning".
  Don't reorder. Errors from `get_index_status` are swallowed (indexing may be disabled or not yet initialized).
- **`get_index_status` backfill recovers tier inputs after a mid-scan reload**, but `scanStartedAt` can't cross IPC, so
  it stays 0: the percent still works (elapsed-free), while the tier-1 ETA seed and elapsed extrapolation degrade until
  the sliding window fills (accepted). Tier-1 calibration (`priorTotalEntries`, `priorScanDurationMs`) reads the nested
  `indexStatus` meta (`totalEntries` / `scanDurationMs` — the PREVIOUS completed scan's totals, not live; TEXT, parse
  via `parseMetaNumber`).
- **Scan progress has two tiers** (`computeScanProgress`): tier 1 (`priorTotalEntries` present) is
  `entriesScanned / priorTotalEntries`, clamped to 0.99, apples-to-apples. Tier 2 (`volumeUsedBytes` present) is
  `bytesScanned / volumeUsedBytes`, clamped lower to 0.95 (APFS clones overshoot the statfs denominator) and flagged
  `rough`. Neither → null (counter-only). The ETA unit must match the tier (entries for tier 1, bytes for tier 2), so
  the component's scan window samples the same counter the tier divides by. `formatEta` carries a `Number.isFinite`
  guard so a dropped null gate can't surface "Infinitym left".
- **The status indicator is one component for all three activity states** (scan/aggregation/replay). Message priority:
  aggregation > scan > replay. Visibility: `isScanning() || isAggregating() || isReplaying()`, shown immediately.
- **The indicator is a focusable, hoverable icon** (`role="img"`, `tabindex="0"`), not `pointer-events: none`: the
  detail lives in a tooltip reached by hover or focus. The tab stop is indexing-only (nothing renders when idle). Don't
  use `role="status"` (a live region — wrong for a focusable hover target); the tooltip carries the live label + ETA via
  `aria-describedby`.
- **`index-dir-updated` callbacks get a batch of paths** (multiple during DB replay, typically one in live FS-watch).
  `DualPaneExplorer` checks each against each pane's current dir with a path-prefix comparison (relies on trailing-slash
  normalization).
- **The `IndexingStatusIndicator.a11y.test.ts` mock must include every getter the indicator imports**, or the existing
  scanning case crashes on `undefined`.
- **Directory sizes are HONEST: unknown (the `<dir>` placeholder) ≠ empty (`0 bytes`) ≠ lower-bound (`≥`).**
  `getDirSizeDisplayState` (`views/full-list-utils.ts`) is the single source of truth, consumed in lockstep by
  `FullList` / `SelectionInfo` / `measure-column-widths`. Rendering + sort: [DETAILS.md](DETAILS.md).

Full public API, the eight-event table, tooltip content per state, ETA blending, dependencies, and tests:
[DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
