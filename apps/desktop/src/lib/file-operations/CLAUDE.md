# File operations

Transfer (copy/move) and mkdir dialogs with progress tracking and conflict resolution.

## Purpose

Provides unified UI for file operations triggered by F5 (copy), F6 (move), and F7 (new folder). All transfer operations
share components parameterized by `operationType: 'copy' | 'move'`.

## Architecture

### Transfer UI flow

1. **TransferDialog** (destination picker + dry-run scan)
    - Pre-fills destination from opposite pane
    - Optional dry-run scan to detect conflicts upfront
    - Shows sampled conflicts (max 200) with streaming progress
    - User makes conflict decisions before operation starts

2. **TransferProgressDialog** (operation execution)
    - Calls `copyFiles()` or `moveFiles()` based on operationType
    - Subscribes via `onWriteProgress`, `onWriteComplete`, `onWriteError`, `onWriteCancelled`, `onWriteConflict`
      callback wrappers (which internally listen to Tauri events). Uses a `BufferedEvent` discriminated union
      (`{ type: 'progress'; event: WriteProgressEvent }`, etc.) to buffer events until the `operationId` is known.
    - Progress bar with ETA, speed (MB/s), current file
    - Dynamic stage indicator: "Scanning" → "Copying" (+ "Cleaning up" for cross-FS move)
    - Conflict resolution inline (if using `Stop` mode instead of dry-run)
    - Cancel button → rollback transaction (user chooses keep/rollback)

3. **TransferErrorDialog** (error display)
    - Operation-specific error messaging via `transfer-error-messages.ts`

### Shared utilities (`transfer/`)

- **transfer/transfer-dialog-utils.ts**: `generateTitle(operationType, files, folders)` → "Copy 3 files and 1 folder",
  `toBackendIndices()` / `toBackendCursorIndex()` for ".." offset handling
- **transfer/DirectionIndicator.svelte**: Arrow graphic showing source → destination (operation-agnostic)
- **transfer/TransferDialog.svelte**, **transfer/TransferProgressDialog.svelte**,
  **transfer/TransferErrorDialog.svelte**: Transfer UI components
- **transfer/transfer-error-messages.ts**: Operation-specific error strings

### New folder (`mkdir/`)

- **mkdir/NewFolderDialog.svelte**: F7 opens dialog pre-filled with cursor item name (sans extension for files)
- **mkdir/new-folder-operations.ts**: `getInitialFolderName()` extracts from cursor, `moveCursorToNewFolder()`
  subscribes to file watcher to track newly created folder
- **mkdir/new-folder-utils.ts**: Pure utility helpers for deriving the initial folder name from the cursor entry

## Key decisions

### Why unified components?

Copy and Move share 95%+ of UI/flow. Differences are:

- Labels ("Copy" vs "Move")
- Backend command (`copyFiles()` vs `moveFiles()`)
- Post-completion: move refreshes both panes (source files gone)
- Cross-FS move has extra "Cleaning up" stage

Parameterizing by `operationType` avoids duplication and guarantees UX consistency.

### Same-FS move optimization

When source and destination are on same filesystem (checked via `metadata.dev()`), backend uses instant `rename()`
syscall. Frontend handles this by:

- Skipping progress dialog if operation completes before render
- Showing brief success toast instead
- Still doing conflict scan upfront in dry-run mode (just `exists()` checks, ~100ms for 10k files)

### Index conversion for ".." entry

When directory has parent entry shown at index 0, frontend indices are offset by +1 from backend:

- Frontend [0, 1, 2, 3] with hasParent=true → Backend [-1, 0, 1, 2] → filtered to [0, 1, 2]
- Index 0 with hasParent=true is always the ".." entry (backend index -1, invalid)
- `toBackendCursorIndex(0, true)` returns `null` to signal no-op

## Gotchas

- **MTP move limitation**: Cross-volume move between MTP devices isn't supported yet. UI shows alert when attempted.
  `moveFiles()` backend only works with local paths.
- **Dry-run conflict sampling**: If >200 conflicts, `DryRunResult.conflicts` contains random sample. Check
  `conflictsSampled: true` and `conflictsTotal` for exact count.
- **Progress dialog edge case**: Same-FS move completes so fast that the complete event may fire before dialog mounts.
  Handle by checking operation status on mount and showing toast if already done.
- **Source pane refresh**: Move operations must refresh **both** panes post-completion (source files disappeared). Copy
  only refreshes destination.
