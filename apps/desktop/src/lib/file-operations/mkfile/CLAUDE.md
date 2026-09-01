# New file

Shift+F4 dialog (`file.newFile` command) that creates a new empty file in the focused pane. Flow narrative and design
rationale: `DETAILS.md`.

## Module map

- `NewFileDialog.svelte`: dialog UI around the shared `../NewEntryNameField.svelte` +
  `../new-entry-name-check.svelte.ts` (subtitle, name field, validation, async conflict check), then `createFile` and
  `onCreated(name)`.
- `new-file-operations.ts`: `getInitialFileName()`, the full filename (with extension) of the `../cursor-entry.ts`
  lookup.

## Must-knows

- **The dialog only validates and creates; it does NOT open the editor or move the cursor.** It calls `onCreated(name)`
  and stops. Opening the new file in the editor and landing the cursor on it happen in the parent's
  `handleNewFileCreated` (`file-explorer/pane/dialog-state.svelte.ts`, via `moveCursorToNewFolder` and
  `onOpenInEditor`). Editor-launch / cursor logic edits go there, not here.
- **The pre-fill cursor offset is shared with `mkdir`.** Both `getInitialFileName` and `getInitialFolderName` read the
  cursor entry through `getCursorEntry()` (`../cursor-entry.ts`), which applies the `..` + `hasParent` arithmetic once.
  Don't re-derive the index here, or the pre-fill reads the wrong entry.
- **The name field and its validation are shared with `NewFolderDialog`** (`../NewEntryNameField.svelte` +
  `../new-entry-name-check.svelte.ts`): the `$lib/utils/filename-validation` validators, then the async conflict check
  via `findFileIndex` + `getFileAt`. A validation change lands there, once.
- **Extension is preserved on pre-fill** (unlike the folder dialog, which strips it): a cursor item `report.pdf` opens
  the dialog with `report.pdf` selected. Directories and `..` pre-fill empty.

Backend counterpart: `create_file` lives directly under
`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` (no dedicated subdir).
