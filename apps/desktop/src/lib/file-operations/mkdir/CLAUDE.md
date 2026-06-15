# New folder

F7 opens a dialog to create a new folder in the focused pane, with name validation, conflict detection, and AI-powered
name suggestions (when the AI subsystem is configured).

Backend counterpart:
[`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`](../../../../src-tauri/src/file_system/write_operations/CLAUDE.md)
(no dedicated subdir on the BE side — `create_directory` lives directly under `write_operations`).

## File map

- **`NewFolderDialog.svelte`**: Dialog UI, name validation, async conflict check, AI-suggestion streaming, timeout
  warning, post-create cursor
- **`new-folder-operations.ts`**: `getInitialFolderName()` from cursor entry; `moveCursorToNewFolder()` (subscribes to
  `directory-diff` watcher)
- **`new-folder-utils.ts`**: Pure helpers (`removeExtension()`) for deriving the initial folder name
- **`NewFolderDialog.a11y.test.ts`**: A11y assertions
- **`NewFolderDialog.streaming.test.ts`**: Tests for AI-suggestion streaming
- **`new-folder-utils.test.ts`**: Pure-utility tests

## How new-folder flows

1. F7 (or `file.newFolder` command) opens `NewFolderDialog` pre-filled with the cursor item name. For files, the
   extension is stripped via `removeExtension()`; for directories the full name is used; for ".." the field is empty.
2. As the user types, `validateName` runs (100 ms debounce):
   - Sync validators from `$lib/utils/filename-validation`: `validateDisallowedChars`, `validateNameLength`,
     `validatePathLength`.
   - Async conflict check via `findFileIndex(listingId, name, showHiddenFiles)` + `getFileAt` to report "There is
     already a folder/file by this name in this folder."
3. The AI suggestion panel queries `getAiStatus()` once. When available, `streamFolderSuggestions` streams suggestion
   chips the user can click to fill the input.
4. On confirm, `createDirectory(currentPath, name, volumeId)` is called. If it times out (slow volume), the dialog shows
   a warning banner with "Refresh listing" and "Dismiss" actions instead of a generic error. Warning uses
   `--color-warning` / `--color-warning-bg` to distinguish from permanent errors.
5. On success, `moveCursorToNewFolder()` positions the cursor on the new folder (see Gotchas below).

## Key decisions

### AI suggestions are optional, never blocking

The dialog opens immediately with a focused input; AI runs in the background. If AI is disabled or unavailable, the
suggestion strip simply doesn't render — the dialog stays fully usable. `aiAvailable` starts at `null` ("checking") to
avoid a flash-of-empty-strip on slow `getAiStatus()` responses.

### Timeout warning vs error

A slow `createDirectory` returns a timeout-shaped error after the BE's deadline. Rather than show a red "Failed" error,
we show a yellow "Still working…" banner so the user can refresh and verify outside the dialog. The folder may still
land via the directory watcher.

## Gotchas

- **`paneRef.setPendingCursorName(name)` MUST run before the optimistic `setCursorIndex`.** `create_directory` queues a
  synthetic `directory-diff` through `diff_emitter::enqueue_diff` (50 ms trailing-window coalesce). The optimistic
  `setCursorIndex` in `moveCursorToNewFolder` lands the cursor correctly at the moment, but when the deferred diff
  fires, `FilePane`'s diff handler runs the new entry's index through `adjustSelectionIndices` and shifts the cursor +1
  (an `add` at the cursor's index always pushes the cursor down). `setPendingCursorName` writes to the same
  `pendingCursorName` field the diff handler already checks for the rename flow: when the diff lands, it re-pins the
  cursor by name and `return`s before the structural shift runs. Regression guard:
  `file-operations.spec.ts › Create folder round-trip › cursor lands on the newly created folder`.
- **`moveCursorToNewFolder` is shared with `mkfile`.** The same function handles both since cursor positioning is
  entry-type-agnostic.

Full details: [DETAILS.md](DETAILS.md).
