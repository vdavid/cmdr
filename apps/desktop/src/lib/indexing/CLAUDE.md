# Indexing (frontend)

Frontend bridge to the Rust drive indexer. Owns reactive scan state, Tauri event listeners, and the scan status overlay.

Rust counterpart: `apps/desktop/src-tauri/src/indexing/`

## Files

| File                         | Purpose                                                            |
| ---------------------------- | ------------------------------------------------------------------ |
| `index.ts`                   | Public API barrel export                                           |
| `index-state.svelte.ts`      | Module-level `$state` for scan progress; listens for scan events   |
| `index-events.ts`            | Listens for `index-dir-updated`, calls back with updated paths     |
| `ScanStatusOverlay.svelte`   | Thin wrapper feeding scan/aggregation state into `ProgressOverlay` |
| `ReplayStatusOverlay.svelte` | Thin wrapper feeding replay state into `ProgressOverlay`           |

## Public API (`index.ts`)

```ts
// Scan state (call from .svelte files or .svelte.ts reactive contexts)
isScanning(): boolean
getEntriesScanned(): number
getDirsFound(): number
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

Module-level `$state` variables (`scanning`, `entriesScanned`, `dirsFound`, `aggregating`, `aggregationPhase`,
`aggregationCurrent`, `aggregationTotal`, `aggregationStartedAt`, `replaying`, `replayEventsProcessed`,
`replayEstimatedTotal`, `replayStartedAt`) react to eight Tauri events:

| Event                        | Payload                                             | Effect                                                  |
| ---------------------------- | --------------------------------------------------- | ------------------------------------------------------- |
| `index-scan-started`         | `{ volumeId }`                                      | `scanning = true`, counters reset                       |
| `index-scan-progress`        | `{ volumeId, entriesScanned, dirsFound }`           | Update counters                                         |
| `index-scan-complete`        | `{ volumeId, totalEntries, totalDirs, durationMs }` | `scanning = false`, set final counts, reset aggregation |
| `index-rescan-notification`  | `{ volumeId, reason, details }`                     | Show info toast with reason-specific message            |
| `index-replay-progress`      | `{ volumeId, eventsProcessed, estimatedTotal }`     | `replaying = true` on first, update counters            |
| `index-replay-complete`      | `{ volumeId, durationMs }`                          | Reset replay state                                      |
| `index-aggregation-progress` | `{ phase, current, total }`                         | `aggregating = true`, update phase/progress/ETA         |
| `index-aggregation-complete` | `()`                                                | Reset aggregation state, dismiss overlay                |

**Startup race condition**: The Rust indexer starts in Tauri's `setup()` hook before the frontend registers listeners.
`initIndexState` uses a "listen first, then query" pattern: registers event listeners, then calls `get_index_status` IPC
to catch any scan already in progress. Errors from `get_index_status` are swallowed silently (indexing may be disabled
or not yet initialized).

`$state` must live in a `.svelte.ts` file — plain `.ts` files do not support Svelte runes.

## Directory update events (`index-events.ts`)

`initIndexEvents` registers a listener for `index-dir-updated` (payload: `{ paths: string[] }`). The callback is called
with a batch of paths — multiple paths during DB replay, typically one path during live FS-watch mode.

`DualPaneExplorer` calls this and checks each path against the current directory of each pane using a path-prefix
comparison (relies on trailing-slash normalization).

## Scan status overlay (`ScanStatusOverlay.svelte`)

Thin wrapper that computes label, progress, and ETA from scan/aggregation state, then delegates rendering to
`$lib/ui/ProgressOverlay.svelte`. Visible while `isScanning()` or `isAggregating()` is true.

- **Scan phase**: Label only (compact layout, no progress bar). Shows "Scanning... 42,000 entries, 1,200 dirs".
- **Aggregation phase**: Column layout with phase label + optional progress bar + ETA. Phases: `saving_entries`
  (flushing writer backlog), `loading`, `sorting` (no progress bar), `computing`, `writing` (progress bar on
  `saving_entries`, `computing`, and `writing`). ETA is computed from elapsed time and current/total ratio, reset on
  phase transitions.

Uses `formatNumber` from selection-info-utils for number formatting (uses `'en-US'` locale, hardcoded via
`toLocaleString('en-US')`).

## Replay status overlay (`ReplayStatusOverlay.svelte`)

Thin wrapper that computes label, progress, and ETA from replay state, then delegates rendering to
`$lib/ui/ProgressOverlay.svelte`. Visible when `isReplaying()` is true AND more than 4 seconds have elapsed since replay
started, AND not currently scanning or aggregating (to avoid stacking overlays).

- **Progress bar**: `eventsProcessed / estimatedTotal` ratio.
- **ETA**: 50-50 blend of total-based ETA (elapsed extrapolation) and a sliding-window rate over the last ~5 seconds.
  Falls back to whichever is available if one can't be computed yet.
- **Detail**: Shows "{N} events processed" with locale formatting.

## Key decisions

**Decision**: "Listen first, then query" initialization pattern in `initIndexState`. **Why**: The Rust indexer starts in
Tauri's `setup()` hook, which runs before the frontend mounts. If we registered listeners after querying status, we'd
have a race window where `index-scan-started` fires between the query and the listener registration, leaving the UI
stuck on "not scanning". Registering listeners first closes this gap — any event that fires during or after the query is
caught.

**Decision**: Scan overlay uses `pointer-events: none`. **Why**: The overlay sits in the top-right corner over the file
list. Without `pointer-events: none`, it would intercept clicks on files near the corner. The overlay is purely
informational — no interactive elements.

## No tests

No unit or integration tests exist for this module yet. Manual testing via the Rust indexer with `pnpm dev`.

## Dependencies

- `@tauri-apps/api/core` — `invoke`
- `$lib/tauri-commands` — `listen`, `UnlistenFn`
- `$lib/ui/toast` — `addToast` (rescan notification toasts)
- `$lib/file-explorer/selection/selection-info-utils` — `formatNumber` (overlay only)
- `$lib/ui/ProgressOverlay.svelte` — reusable progress overlay component (used by `ScanStatusOverlay`)
