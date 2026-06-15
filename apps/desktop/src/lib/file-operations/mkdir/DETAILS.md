# New folder details

Depth for the F7 new-folder dialog. `CLAUDE.md` holds the must-knows; the flows and decisions live here.

## How new-folder flows

1. F7 (or the `file.newFolder` command) opens `NewFolderDialog` pre-filled with the cursor item name. For files the
   extension is stripped via `removeExtension()`; for directories the full name is used; for ".." the field is empty.
2. As the user types, validation runs on a 100 ms debounce:
   - Sync validators from `$lib/utils/filename-validation`: `validateDisallowedChars`, `validateNameLength`,
     `validatePathLength`.
   - Async conflict check via `findFileIndex(listingId, name, showHiddenFiles)` + `getFileAt`, reporting "There is
     already a folder/file by this name in this folder."
3. The AI suggestion panel queries `getAiStatus()` once. When available, `streamFolderSuggestions` streams suggestion
   chips the user can click to fill the input.
4. On confirm, `createDirectory(currentPath, name, volumeId)` runs. On success, `moveCursorToNewFolder()` positions the
   cursor on the new folder (see the `CLAUDE.md` cursor-pinning gotcha).

## Decisions

### AI suggestions are optional, never blocking

The dialog opens immediately with a focused input; AI runs in the background. If AI is disabled or unavailable, the
suggestion strip doesn't render and the dialog stays fully usable. `aiAvailable` starts at `null` ("checking") to avoid
a flash-of-empty-strip on slow `getAiStatus()` responses.

### Timeout warning vs error

A slow `createDirectory` returns a timeout-shaped error after the backend's deadline. Rather than a red "failed" error,
the dialog shows a yellow "still working" banner (`--color-warning` / `--color-warning-bg`) with "Refresh listing" and
"Dismiss" actions, so the user can refresh and verify outside the dialog. The folder may still land via the directory
watcher.
