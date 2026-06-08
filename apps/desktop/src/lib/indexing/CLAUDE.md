# Indexing (frontend)

Frontend bridge to the Rust drive indexer. Owns reactive scan state, Tauri event listeners, and the drive-indexing
status indicator.

Rust counterpart: `apps/desktop/src-tauri/src/indexing/`

## Files

| File                             | Purpose                                                                                                                        |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `index.ts`                       | Public API barrel export                                                                                                       |
| `index-state.svelte.ts`          | Module-level `$state` for scan progress; listens for scan events                                                               |
| `index-events.ts`                | Listens for `index-dir-updated`, calls back with updated paths                                                                 |
| `eta.ts`                         | Pure ETA helpers (formatting thresholds, elapsed + sliding-window estimation) + `computeScanProgress` (two-tier scan fraction) |
| `IndexingStatusIndicator.svelte` | Top-right hourglass icon; rich tooltip with scan / aggregation / replay state                                                  |

## Public API (`index.ts`)

```ts
// Scan state (call from .svelte files or .svelte.ts reactive contexts)
isScanning(): boolean
getEntriesScanned(): number
getDirsFound(): number
getBytesScanned(): number            // resolved post-dedup physical bytes scanned (tier-2 numerator)
getScanStartedAt(): number          // Date.now() at scan start; 0 on late-join (no wall-clock backfill)
getPriorTotalEntries(): number | null      // prior completed scan's entry total (tier-1 denominator)
getPriorScanDurationMs(): number | null    // prior completed scan's duration (tier-1 ETA seed)
getVolumeUsedBytes(): number | null        // scanned volume's used bytes (tier-2 denominator)
isAggregating(): boolean
getAggregationPhase(): string       // 'saving_entries' | 'loading' | 'sorting' | 'computing' | 'writing'
getAggregationCurrent(): number
getAggregationTotal(): number
getAggregationStartedAt(): number   // Date.now() timestamp
isReplaying(): boolean
getReplayEventsProcessed(): number
getReplayEstimatedTotal(): number
getReplayStartedAt(): number        // Date.now() timestamp
initIndexState(): Promise<void>     // call once at app mount
destroyIndexState(): void           // call at app teardown

// Directory update events
initIndexEvents(onDirUpdated: (paths: string[]) => void): Promise<UnlistenFn>
```

## Scan state (`index-state.svelte.ts`)

Module-level `$state` variables (`scanning`, `entriesScanned`, `dirsFound`, `bytesScanned`, `scanStartedAt`,
`priorTotalEntries`, `priorScanDurationMs`, `volumeUsedBytes`, `aggregating`, `aggregationPhase`, `aggregationCurrent`,
`aggregationTotal`, `aggregationStartedAt`, `replaying`, `replayEventsProcessed`, `replayEstimatedTotal`,
`replayStartedAt`) react to eight Tauri events:

| Event                        | Payload                                                                 | Effect                                                                                          |
| ---------------------------- | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `index-scan-started`         | `{ volumeId, priorTotalEntries, priorScanDurationMs, volumeUsedBytes }` | `scanning = true`, counters reset, `scanStartedAt = Date.now()`, stash the per-scan calibration |
| `index-scan-progress`        | `{ volumeId, entriesScanned, dirsFound, bytesScanned }`                 | Update counters                                                                                 |
| `index-scan-complete`        | `{ volumeId, totalEntries, totalDirs, durationMs }`                     | `scanning = false`, set final counts, reset aggregation                                         |
| `index-rescan-notification`  | `{ volumeId, reason, details }`                                         | Show info toast with reason-specific message                                                    |
| `index-replay-progress`      | `{ volumeId, eventsProcessed, estimatedTotal }`                         | `replaying = true` on first, update counters                                                    |
| `index-replay-complete`      | `{ volumeId, durationMs }`                                              | Reset replay state                                                                              |
| `index-aggregation-progress` | `{ phase, current, total }`                                             | `aggregating = true`, update phase/progress/ETA                                                 |
| `index-aggregation-complete` | `()`                                                                    | Reset aggregation state                                                                         |

**Startup race condition**: The Rust indexer starts in Tauri's `setup()` hook before the frontend registers listeners.
`initIndexState` uses a "listen first, then query" pattern: registers event listeners, then calls `get_index_status` IPC
to catch any scan already in progress. Errors from `get_index_status` are swallowed silently (indexing may be disabled
or not yet initialized).

The `get_index_status` backfill also recovers scan-progress tier inputs after a mid-scan window reload: `bytesScanned`
and `volumeUsedBytes` ride the top-level response, and the tier-1 calibration (`priorTotalEntries`,
`priorScanDurationMs`) comes from the nested `indexStatus` meta read (`totalEntries` / `scanDurationMs`, which are the
PREVIOUS completed scan's totals — the completion handler is the only writer — i.e. exactly the tier-1 denominator, not
the live counters). Meta values are TEXT, so `parseMetaNumber` (`Number()` + NaN-guard) parses them. The one input the
backfill can't recover is `scanStartedAt` (no scan-start wall-clock crosses IPC), so after a reload it stays 0: the
percent still works (elapsed-free), but the tier-1 ETA seed and elapsed extrapolation degrade until the sliding window
fills. Accepted graceful degradation.

`$state` must live in a `.svelte.ts` file: plain `.ts` files do not support Svelte runes.

## Directory update events (`index-events.ts`)

`initIndexEvents` registers a listener for `index-dir-updated` (payload: `{ paths: string[] }`). The callback is called
with a batch of paths: multiple paths during DB replay, typically one path during live FS-watch mode.

`DualPaneExplorer` calls this and checks each path against the current directory of each pane using a path-prefix
comparison (relies on trailing-slash normalization).

## Status indicator (`IndexingStatusIndicator.svelte`)

One component for all three index-activity states (scan, aggregation, replay). They're logically the same in the user's
mental model — "the drive index is updating" — so they share a single quiet indicator instead of three overlays fighting
for the corner.

- **Rendering**: a small Lucide hourglass (`~icons/lucide/hourglass`, ~14px, the same icon as the size-column stale
  indicator) pinned `position: absolute; top/right: var(--spacing-sm)` in tertiary text color. It pulses opacity gently
  to signal activity (design principle: show some anim when the app is doing something), gated behind
  `prefers-reduced-motion: reduce` (static then). The full detail lives in a rich tooltip on hover/focus.
- **Visibility**: `isScanning() || isAggregating() || isReplaying()`. Any index activity shows the icon immediately — no
  grace delay, because a small icon is unobtrusive and showing it immediately keeps the indicator honest.
- **Message priority**: aggregation > scan > replay. One mode owns the tooltip at a time, so a running scan or
  aggregation shows its own message rather than a replay one underneath.
- **Tooltip content** (the component renders the content inside a `<div hidden>` host and passes the inner content div —
  not the hidden host — to the tooltip action via `contentEl`, so the adopted element doesn't carry `hidden` into the
  tooltip):
  - Scan: a two-tier label + counters, plus a `ProgressBar` + percent + ETA when a denominator exists (see below). Tier
    1 (calibrated): "Scanning your drive... 42,000 entries, 1,200 dirs" with "42% · 1m 20s left". Tier 2 (first scan,
    rough): "Scanning your drive (first scan)... 42,000 entries, 1,200 dirs" with "36% · roughly 19m left". When
    `computeScanProgress` returns null (no denominator), it falls back to the counter-only label.
  - Aggregation: phase label ("Saving entries...", "Loading directories...", "Sorting directories...", "Computing
    directory sizes...", "Saving directory sizes...") + `ProgressBar` + percent + ETA for the phases that have progress
    (`saving_entries`, `computing`, `writing`).
  - Replay: "Updating index..." + "N events processed" + `ProgressBar` + blended ETA.
- **ETA**: pure helpers in `eta.ts`. Aggregation uses a single elapsed extrapolation. Scan and replay blend that 50-50
  with a sliding-window rate over the last ~5 seconds (early extrapolation alone is wildly wrong). The window snapshot
  collection is the only stateful glue and stays in the component; it feeds the pure `pruneSnapshots` /
  `computeWindowEta` / `blendEtas` / `formatEta` functions.
- **Scan progress + tier selection** (`computeScanProgress` in `eta.ts`): picks the tier from the available
  denominators. Tier 1 (`priorTotalEntries` present): `entriesScanned / priorTotalEntries`, clamped to
  `SCAN_PROGRESS_CALIBRATED_MAX` (0.99), apples-to-apples (both sides are the scan's own entry counter). Tier 2
  (`priorTotalEntries` absent, `volumeUsedBytes` present): `bytesScanned / volumeUsedBytes`, clamped lower to
  `SCAN_PROGRESS_ROUGH_MAX` (0.95) since APFS clones can overshoot the statfs denominator, and flagged `rough`. Neither
  -> null (counter-only). The ETA unit must match the tier — entries for tier 1, bytes for tier 2 — so the component's
  scan window samples the same counter the tier divides by; a tier-1 prior-duration seed
  (`priorScanDurationMs − elapsed`, ms→seconds) covers the gap before the window has samples. Tier 2's ETA is prefixed
  "roughly". `formatEta` carries a `Number.isFinite` guard so a dropped null gate can't surface "Infinitym left".

Uses `formatNumber` from selection-info-utils for number formatting (uses `'en-US'` locale, hardcoded via
`toLocaleString('en-US')`).

## Key decisions

**Decision**: "Listen first, then query" initialization pattern in `initIndexState`. **Why**: The Rust indexer starts in
Tauri's `setup()` hook, which runs before the frontend mounts. If we registered listeners after querying status, we'd
have a race window where `index-scan-started` fires between the query and the listener registration, leaving the UI
stuck on "not scanning". Registering listeners first closes this gap: any event that fires during or after the query is
caught.

**Decision**: The status indicator is a focusable, hoverable icon (`role="img"`, `tabindex="0"`), not a
`pointer-events: none` glyph. **Why**: the rich detail lives in a tooltip the user reaches by hover or focus, so the
icon must accept pointer and keyboard interaction. The hover target is a tiny ~14px icon in the corner, so stealing
clicks near files isn't a real concern (the old full-width overlay needed `pointer-events: none` because it spanned a
visible band). The tab stop is intentional and indexing-only — the component renders nothing when idle, so there's no
dead tab stop in the steady state. The tooltip carries the live label + ETA via `aria-describedby`; `role="status"`
would be wrong here (it's a live region for auto-announced changes, not a focusable hover target).

## Tests

- `eta.test.ts`: the pure ETA helpers (thresholds, elapsed + window estimation, blending, snapshot pruning) plus
  `computeScanProgress` (tier selection, both clamps, null/zero-denominator fallbacks) and the `formatEta` non-finite
  pin.
- `IndexingStatusIndicator.a11y.test.ts`: tier-3 axe checks for idle (renders nothing), scanning (counter-only AND
  calibrated-with-bar), and aggregating-with-progress, mocking `index-state.svelte`. Every getter the indicator imports
  must appear in that mock, or the pre-existing scanning case crashes on `undefined`.

The reactive event-driven glue in `index-state.svelte.ts` is allowlisted in `coverage-allowlist.json` (module `$state`
driven by Tauri events). Manual end-to-end testing via the Rust indexer with `pnpm dev`.

## Dependencies

- `$lib/ipc/bindings`: `commands` (status query)
- `$lib/tauri-commands`: typed indexing event wrappers (`onIndexScanStarted`, `onIndexScanProgress`,
  `onIndexScanComplete`, `onIndexAggregationProgress`, `onIndexAggregationComplete`, `onIndexRescanNotification`,
  `onIndexReplayProgress`, `onIndexReplayComplete`, `onIndexDirUpdated`) plus `UnlistenFn`. The events are
  `tauri-specta`-typed (wire names pinned via `event_name`); the wrappers live in `tauri-commands/indexing.ts`.
- `$lib/ui/toast`: `addToast` (rescan notification toasts)
- `$lib/file-explorer/selection/selection-info-utils`: `formatNumber` (indicator only)
- `$lib/tooltip/tooltip`: `tooltip` action with the `contentEl` live-content param (indicator only)
- `$lib/ui/ProgressBar.svelte`: reusable progress bar, size `sm` (in the indicator tooltip)
