# Indexing (frontend)

Frontend bridge to the Rust drive indexer. Owns reactive scan state, Tauri event listeners, priority-scan IPC, and the
scan status overlay.

Rust counterpart: `apps/desktop/src-tauri/src/indexing/`

## Files

| File                       | Purpose                                                          |
| -------------------------- | ---------------------------------------------------------------- |
| `index.ts`                 | Public API barrel export                                         |
| `index-state.svelte.ts`    | Module-level `$state` for scan progress; listens for scan events |
| `index-events.ts`          | Listens for `index-dir-updated`, calls back with updated paths   |
| `index-priority.ts`        | IPC wrappers for priority micro-scans                            |
| `ScanStatusOverlay.svelte` | Floating top-right spinner + live counters during scan           |

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
initIndexState(): Promise<void>     // call once at app mount
destroyIndexState(): void           // call at app teardown

// Directory update events
initIndexEvents(onDirUpdated: (paths: string[]) => void): Promise<UnlistenFn>

// Priority micro-scans
prioritizeDir(path: string, priority: 'user_selected' | 'current_dir'): Promise<void>
cancelNavPriority(path: string): Promise<void>
```

## Scan state (`index-state.svelte.ts`)

Module-level `$state` variables (`scanning`, `entriesScanned`, `dirsFound`, `aggregating`, `aggregationPhase`,
`aggregationCurrent`, `aggregationTotal`, `aggregationStartedAt`) react to five Tauri events:

| Event                        | Payload                                             | Effect                                                  |
| ---------------------------- | --------------------------------------------------- | ------------------------------------------------------- |
| `index-scan-started`         | `{ volumeId }`                                      | `scanning = true`, counters reset                       |
| `index-scan-progress`        | `{ volumeId, entriesScanned, dirsFound }`           | Update counters                                         |
| `index-scan-complete`        | `{ volumeId, totalEntries, totalDirs, durationMs }` | `scanning = false`, set final counts, reset aggregation |
| `index-rescan-notification`  | `{ volumeId, reason, details }`                     | Show info toast with reason-specific message            |
| `index-aggregation-progress` | `{ phase, current, total }`                         | `aggregating = true`, update phase/progress/ETA         |

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

## Priority micro-scans (`index-priority.ts`)

| Priority        | When used                       |
| --------------- | ------------------------------- |
| `user_selected` | Cursor moves onto a directory   |
| `current_dir`   | User navigates into a directory |

Both `prioritizeDir` and `cancelNavPriority` silently swallow all errors — indexing may be disabled in settings or not
yet initialized.

## Scan status overlay (`ScanStatusOverlay.svelte`)

Rendered in the top-right corner of the main window while `isScanning()` or `isAggregating()` is true. Uses
`pointer-events: none` so it never blocks clicks. Two modes:

- **Scan phase**: Spinner + live label like `Scanning... 42,000 entries, 1,200 dirs`.
- **Aggregation phase**: Spinner + phase label (for example, "Computing directory sizes...") + progress bar with real %
  and ETA estimate. Phases: `saving_entries` (flushing writer backlog), `loading`, `sorting` (no progress bar),
  `computing`, `writing` (progress bar on `saving_entries`, `computing`, and `writing`). ETA is computed from elapsed
  time and current/total ratio. Progress bar uses `--color-accent` fill with smooth CSS transition.

Uses `formatNumber` from selection-info-utils for number formatting (uses `'en-US'` locale, hardcoded via
`toLocaleString('en-US')`).

## Key decisions

**Decision**: "Listen first, then query" initialization pattern in `initIndexState`. **Why**: The Rust indexer starts in
Tauri's `setup()` hook, which runs before the frontend mounts. If we registered listeners after querying status, we'd
have a race window where `index-scan-started` fires between the query and the listener registration, leaving the UI
stuck on "not scanning". Registering listeners first closes this gap — any event that fires during or after the query is
caught.

**Decision**: All priority/cancel IPC calls silently swallow errors. **Why**: Indexing is an optional subsystem — it may
be disabled in settings, not yet initialized (setup hook hasn't finished), or unavailable on certain volumes. Bubbling
these errors would require every call site to handle a "not available" state, adding noise for a best-effort
optimization feature.

**Decision**: Two priority levels (`user_selected` vs `current_dir`) instead of a single "prioritize" call. **Why**: The
Rust indexer uses these to decide queue ordering. `current_dir` (user navigated into a folder) gets higher priority than
`user_selected` (cursor hovered over a folder). `cancelNavPriority` only cancels `current_dir` scans on navigate-away,
leaving `user_selected` scans running — the user might navigate back.

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
