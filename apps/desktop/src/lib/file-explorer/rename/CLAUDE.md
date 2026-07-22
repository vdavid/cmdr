# Rename

Inline file/folder rename with validation, conflict resolution, and extension-change confirmation. Activated by F2,
Shift+F6, the context menu, or click-to-rename (800 ms hold on the cursor item's name). Operates on the cursor item
only; selection is preserved and irrelevant.

## Module map

- **InlineRenameEditor.svelte**: the inline input that replaces the name cell.
- **RenameConflictDialog.svelte** / **ExtensionChangeDialog.svelte**: the two mid-flow confirmation dialogs.
- **rename-state.svelte.ts**: reactive state (`.svelte.ts` for Svelte 5 reactivity).
- **rename-operations.ts**: pure save flow, returns a `RenameResult` discriminated union (`noop` / `error` / `timeout` /
  `extension-ask` / `conflict` / `success`).
- **rename-activation.ts**: click-to-rename timer.

Full details (the three-stage save flow, permission/validation tiers, post-rename cursor tracking, decisions):
`DETAILS.md`.

## Must-knows

- **Same-name edit (`trimmedName === originalName`) is a cancel/no-op.** Don't emit a watcher event or refresh the pane;
  it avoids a spurious refresh on whitespace-only edits.
- **Case-only and known-equivalent extension changes are treated as no change in all extension-policy modes.** Case-only
  (`.JPG` → `.jpg`) and known-equivalent (`.jpeg` → `.jpg`, `.md` → `.txt`) never show the dialog or a red border. With
  policy "no" an extension change shows a red border while editing; with "ask" no red border (the dialog waits for
  save); with "yes" the extension is never validated.
- **Conflict detection on local FS uses inode comparison, not `exists()`.** On case-insensitive APFS, `readme.txt` →
  `README.txt` is the same file; `exists()` would false-positive. The backend compares `dev+ino` via
  `symlink_metadata()`.
- **`renameFile` and `moveToTrash` can time out on slow mounts; surface the honest "may have succeeded" warning and
  auto-refresh.** Don't treat a timeout as a hard failure: the rename may have landed on disk.
- **Thread `volumeId` through `renameFile` / `checkRenameValidity` / `checkRenamePermission`.** Validity (conflict)
  checks work for all volumes via the Volume trait, but permission checks are skipped for MTP (Unix `access()` doesn't
  work on MTP virtual paths).
- **Cancel triggers**: Escape, click elsewhere, Tab, drag start, and sort/hidden toggle all discard the rename. So does
  the editor losing focus for any reason: scrolling the renamed row out of the virtual window UNMOUNTS the input, whose
  `onblur` cancels (there's no scroll-distance threshold). The editor mounts BY PATH (`shouldMountRenameEditor`), so a
  watcher diff that shifts OTHER rows makes it follow its file instead of cancelling; only a diff that removes the
  renamed file itself cancels (`listing-diff-sync`). Watcher events otherwise do NOT cancel (the backend catches issues
  on save).
- **Double-click on the name area must open the file/folder, not activate rename.** The click-to-rename timer cancels on
  a double-click event.
- **While rename is active, Cmd+C/A/Z/X/V act as text-editing shortcuts, not app commands** (same flag mechanism as
  dialogs). Other shortcuts (Cmd+O, arrows, etc.) are suppressed.
