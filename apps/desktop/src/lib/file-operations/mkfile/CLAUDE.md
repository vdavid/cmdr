# New file

Shift+F4 dialog (`file.newFile` command) that creates a new empty file in the focused pane. Flow narrative and design
rationale: [DETAILS.md](DETAILS.md).

## Module map

- `NewFileDialog.svelte`: dialog UI, name validation, async conflict check, then `createFile` and `onCreated(name)`.
- `new-file-operations.ts`: `getInitialFileName()`, extracts the full filename (with extension) from the cursor entry.

## Must-knows

- **The dialog only validates and creates; it does NOT open the editor or move the cursor.** It calls `onCreated(name)`
  and stops. Opening the new file in the editor and landing the cursor on it happen in the parent's
  `handleNewFileCreated` (`file-explorer/pane/dialog-state.svelte.ts`, via `moveCursorToNewFolder` and
  `onOpenInEditor`). Editor-launch / cursor logic edits go there, not here.
- **The pre-fill cursor offset matches `mkdir`.** `getInitialFileName` picks the backend index from
  `paneRef.getCursorIndex()` using the same `..` + `hasParent` arithmetic as the folder dialog. Keep the two helpers in
  lock-step, or the pre-fill reads the wrong entry.
- **Validation uses the shared filename validators** (`validateDisallowedChars`, `validateNameLength`,
  `validatePathLength` from `$lib/utils/filename-validation`), same as `NewFolderDialog`. Async conflict check via
  `findFileIndex` + `getFileAt`.
- **Extension is preserved on pre-fill** (unlike the folder dialog, which strips it): a cursor item `report.pdf` opens
  the dialog with `report.pdf` selected. Directories and `..` pre-fill empty.

Backend counterpart: `create_file` lives directly under
[`src-tauri/src/file_system/write_operations/`](../../../../src-tauri/src/file_system/write_operations/CLAUDE.md) (no
dedicated subdir).
