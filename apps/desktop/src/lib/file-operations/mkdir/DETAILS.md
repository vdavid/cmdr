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

On Cloud without "Allow cloud AI" (`cloudAiBlocked`, after a `refreshCloudConsent()`), the dialog opens no stream and
shows no strip, with no copy: the quiet treatment is deliberate, since the feature's absence costs nothing here. The
backend refuses the call anyway (an empty `done`); `lib/ai/DETAILS.md` § Cloud AI consent.

### The name box is typeable the instant F7 opens

`../NewEntryNameField.svelte` focuses and selects the box in its own `onMount`, and `ModalDialog` skips its scrim focus
when something inside already owns it (why, and the ordering that made this a bug: `$lib/ui/DETAILS.md` § ModalDialog).
Guarded by `NewFolderDialog.focus.test.ts` and the F7 round-trip in `file-operations.spec.ts`.

### A slow volume is "still creating", never a failure

A slow `createDirectory` (a busy NAS took 7–12 s, ERR-AREUV) answers `stillRunning` at the backend's deadline and
resolves when the folder really lands. The dialog shows `StillCreatingNotice` and waits, then closes and lands the
cursor like any create; ❌ don't bring back a timeout banner or a "refresh to check" button, since the answer is coming.
The state machine is shared with the new-file dialog: `../DETAILS.md` § "Mutation refusals". Pinned by
`NewFolderDialog.slow-create.test.ts` and `../create-submission.svelte.test.ts`.
