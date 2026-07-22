# New file details

Flow and design rationale. The must-knows are in `CLAUDE.md`.

## How the new-file flow runs

1. Shift+F4 (or the `file.newFile` command) opens `NewFileDialog`, pre-filled with the cursor item's full filename
   (extension kept for files; empty for directories and `..`), computed by `getInitialFileName()`.
2. Validation reuses the shared validators (`validateDisallowedChars`, `validateNameLength`, `validatePathLength`). An
   async conflict check (`findFileIndex` + `getFileAt`) reports "There is already a file/folder by this name in this
   folder." The dialog also listens for `onDirectoryDiff` on its listing to re-validate when the folder changes
   underneath it.
3. On confirm, `createFile(currentPath, name, volumeId)` creates an empty file, then the dialog calls `onCreated(name)`
   and stops.
4. The parent's `handleNewFileCreated` (`file-explorer/pane/dialog-state.svelte.ts`) then lands the cursor on the new
   file via `moveCursorToNewFolder` (entry-type-agnostic; shared with mkdir) and opens it in the default editor via
   `onOpenInEditor` (which calls `openInEditor`). If the editor launch fails (no default app, denied), the file is still
   created and the dialog has already closed, so the user can open it manually.

## Why simpler than `NewFolderDialog`

- **No AI suggestions.** Users always know the filename they want; an AI panel would be noise.
- **No timeout warning banner.** File creation is near-instant on every supported backend.
- **Extension preserved on pre-fill.** A cursor item named `report.pdf` opens with `report.pdf` selected (the folder
  dialog strips `.pdf`). The user can keep or change the extension.
