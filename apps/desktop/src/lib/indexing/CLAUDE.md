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
initIndexState(): Promise<void>     // call once at app mount
destroyIndexState(): void           // call at app teardown

// Directory update events
initIndexEvents(onDirUpdated: (paths: string[]) => void): Promise<UnlistenFn>

// Priority micro-scans
prioritizeDir(path: string, priority: 'user_selected' | 'current_dir'): Promise<void>
cancelNavPriority(path: string): Promise<void>
```

## Scan state (`index-state.svelte.ts`)

Module-level `$state` variables (`scanning`, `entriesScanned`, `dirsFound`) react to three Tauri events:

| Event                 | Payload                                             | Effect                               |
| --------------------- | --------------------------------------------------- | ------------------------------------ |
| `index-scan-started`  | `{ volumeId }`                                      | `scanning = true`, counters reset    |
| `index-scan-progress` | `{ volumeId, entriesScanned, dirsFound }`           | Update counters                      |
| `index-scan-complete` | `{ volumeId, totalEntries, totalDirs, durationMs }` | `scanning = false`, set final counts |

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

Both `prioritizeDir` and `cancelNavPriority` silently swallow all errors — indexing may be disabled
(`CMDR_DRIVE_INDEX=1` required in dev mode) or not yet initialized.

## Scan status overlay (`ScanStatusOverlay.svelte`)

Rendered in the top-right corner of the main window while `isScanning()` is true. Uses `pointer-events: none` so it
never blocks clicks. Displays a CSS spinner and a live label like `Scanning... 42,000 entries, 1,200 dirs`. Uses
`formatNumber` from selection-info-utils for number formatting (uses `'en-US'` locale, hardcoded via
`toLocaleString('en-US')`).

## No tests

No unit or integration tests exist for this module yet. Manual testing via the Rust indexer with
`CMDR_DRIVE_INDEX=1 pnpm dev`.

## Dependencies

- `@tauri-apps/api/core` — `invoke`
- `$lib/tauri-commands` — `listen`, `UnlistenFn`
- `$lib/file-explorer/selection/selection-info-utils` — `formatNumber` (overlay only)
