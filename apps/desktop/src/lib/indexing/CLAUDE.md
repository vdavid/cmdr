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
  it stays 0 after a reload: the percent still works (elapsed-free), but the tier-1 ETA seed and elapsed extrapolation
  degrade until the sliding window fills. Accepted graceful degradation. The backfill's tier-1 calibration
  (`priorTotalEntries`, `priorScanDurationMs`) reads the nested `indexStatus` meta (`totalEntries` / `scanDurationMs`,
  which are the PREVIOUS completed scan's totals, not live counters). Meta values are TEXT; parse via `parseMetaNumber`.
- **Scan progress has two tiers** (`computeScanProgress`): tier 1 (`priorTotalEntries` present) is
  `entriesScanned / priorTotalEntries`, clamped to 0.99, apples-to-apples. Tier 2 (`volumeUsedBytes` present) is
  `bytesScanned / volumeUsedBytes`, clamped lower to 0.95 (APFS clones overshoot the statfs denominator) and flagged
  `rough`. Neither → null (counter-only). The ETA unit must match the tier (entries for tier 1, bytes for tier 2), so
  the component's scan window samples the same counter the tier divides by. `formatEta` carries a `Number.isFinite`
  guard so a dropped null gate can't surface "Infinitym left".
- **The status indicator is one component for all three activity states** (scan/aggregation/replay), since they're one
  thing in the user's mental model. Message priority: aggregation > scan > replay. Visibility:
  `isScanning() || isAggregating() || isReplaying()`, shown immediately (no grace delay; a small icon is unobtrusive).
- **The indicator is a focusable, hoverable icon** (`role="img"`, `tabindex="0"`), not `pointer-events: none`: the
  detail lives in a tooltip reached by hover or focus. The tab stop is indexing-only (renders nothing when idle, so no
  dead tab stop). Don't use `role="status"` (that's a live region for auto-announced changes, wrong for a focusable
  hover target); the tooltip carries the live label + ETA via `aria-describedby`.
- **`index-dir-updated` callbacks get a batch of paths** (multiple during DB replay, typically one in live FS-watch).
  `DualPaneExplorer` checks each against each pane's current dir with a path-prefix comparison (relies on trailing-slash
  normalization).
- **The `IndexingStatusIndicator.a11y.test.ts` mock must include every getter the indicator imports**, or the existing
  scanning case crashes on `undefined`.

## Dependencies

- `$lib/ipc/bindings`: `commands` (status query).
- `$lib/tauri-commands`: the `tauri-specta`-typed indexing event wrappers (`onIndexScan*`, `onIndexAggregation*`,
  `onIndexReplay*`, `onIndexRescanNotification`, `onIndexDirUpdated`) + `UnlistenFn`, in `tauri-commands/indexing.ts`.
- `$lib/ui/toast`: `addToast` (rescan notification toasts).
- `$lib/file-explorer/selection/selection-info-utils`: `formatNumber` (indicator only, `'en-US'` locale).
- `$lib/tooltip/tooltip`: `tooltip` action with the `contentEl` live-content param (indicator only).
- `$lib/ui/ProgressBar.svelte`: size `sm` (indicator tooltip).

Full details (full public API, the eight-event table, tooltip content per state, ETA blending, tests):
[DETAILS.md](DETAILS.md).
