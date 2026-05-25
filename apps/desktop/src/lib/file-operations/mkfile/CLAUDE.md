# New file

Shift+F4 opens a dialog to create a new empty file in the focused pane, then opens it in the default editor.

Backend counterpart:
[`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`](../../../../src-tauri/src/file_system/write_operations/CLAUDE.md)
(`create_file` lives directly under `write_operations`, no dedicated subdir).

## File map

| File                          | Responsibility                                                                            |
| ----------------------------- | ----------------------------------------------------------------------------------------- |
| `NewFileDialog.svelte`        | Dialog UI, name validation, async conflict check, create + open in editor                 |
| `new-file-operations.ts`      | `getInitialFileName()`: extracts the full filename (with extension) from the cursor entry |
| `NewFileDialog.a11y.test.ts`  | A11y assertions                                                                           |
| `new-file-operations.test.ts` | Pure-utility tests                                                                        |

## How new-file flows

1. Shift+F4 (or `file.newFile` command) opens `NewFileDialog` pre-filled with the cursor item's full filename (keeping
   the extension for files; empty for directories and ".." entries).
2. Validation uses the same shared validators as `NewFolderDialog`: `validateDisallowedChars`, `validateNameLength`,
   `validatePathLength`. Async conflict check via `findFileIndex()` + `getFileAt` reports "There is already a
   file/folder by this name in this folder."
3. On confirm, `createFile(currentPath, name, volumeId)` creates an empty file, then `openInEditor` opens it in the
   default text editor.
4. Cursor lands on the new file via the shared `moveCursorToNewFolder` helper from `../mkdir/new-folder-operations.ts`
   (the function is entry-type-agnostic; see the mkdir `setPendingCursorName` gotcha for why it works that way).

## Key decisions

### Simpler than `NewFolderDialog` by design

- **No AI suggestions.** Users always know what filename they want; the AI panel would be noise.
- **No timeout warning banner.** File creation is near-instant on every supported backend.
- **Extension is preserved on pre-fill.** A cursor item named `report.pdf` opens the dialog with `report.pdf` selected
  (folder dialog would strip the `.pdf`). The user can keep or change the extension.

## Gotchas

- **The pre-fill cursor offset matches `mkdir`.** Both dialogs route through the same ".." + `hasParent` arithmetic to
  pick the backend index from `paneRef.getCursorIndex()`. Keep the two helpers in lock-step.
- **`createFile` then `openInEditor` is a two-step IPC.** If the editor launch fails (no default app, denied), the file
  is still created; the dialog closes successfully and the user can open it manually.
