# File operations

Transfer (copy/move), delete/trash, and mkdir dialogs with progress tracking and conflict resolution.

## Purpose

Provides unified UI for file operations triggered by F5 (copy), F6 (move), F7 (new folder), and F8/Shift+F8
(trash/delete). Transfer and delete operations share `TransferProgressDialog`, parameterized by
`operationType: 'copy' | 'move' | 'delete' | 'trash'`.

## Architecture

### Transfer UI flow

1. **TransferDialog** (destination picker + dry-run scan)
    - Pre-fills destination from opposite pane
    - Validates path structure via `validateDirectoryPath()` from `$lib/utils/filename-validation` (empty, absolute,
      null bytes, length limits), then checks logical constraints (subfolder, same location)
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

### Delete/trash (`delete/`)

- **delete/DeleteDialog.svelte**: Confirmation dialog with file list, scan preview, symlink notice, no-trash warning
- **delete/delete-dialog-utils.ts**: Pure utilities: `generateDeleteTitle()`, `abbreviatePath()`, `getSymlinkNotice()`
- F8 = trash, Shift+F8 = permanent delete. On no-trash volumes, dialog forces permanent mode with warning banner.
- After confirm, transitions to `TransferProgressDialog` with `operationType: 'delete' | 'trash'`
- See `delete/CLAUDE.md` for full details

### New folder (`mkdir/`)

- **mkdir/NewFolderDialog.svelte**: F7 opens dialog pre-filled with cursor item name (sans extension for files). Uses
  shared validators from `$lib/utils/filename-validation` (`validateDisallowedChars`, `validateNameLength`,
  `validatePathLength`) for sync checks, then runs async `findFileIndex()` for conflict detection. If `createDirectory`
  times out (slow volume), shows a warning banner with "Refresh listing" and "Dismiss" actions instead of a generic
  error. Warning uses `--color-warning` / `--color-warning-bg` to distinguish from permanent errors.
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
