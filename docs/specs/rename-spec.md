# Rename feature spec

Inline rename for files and folders in both panes.

## Access points

1. **Menu**: Edit > Rename (Shift+F6), registered in command registry (`command-registry.ts`)
2. **Context menu**: "Rename" item with shortcut hint, on right-click of file/folder
3. **Click-to-rename**: Click name area of entry already under cursor, hold ~800 ms without moving >10 px or
   double-clicking. Cancel on double-click.
4. Always operates on the cursor item. Selection is irrelevant and preserved. No-op on `..` parent entry.

## Activation

- Replace the static name cell with an `<input>` the same size as the name area (full entry width in Brief mode,
  name column only in Full mode). Position so the text doesn't shift.
- Select filename excluding extension, cursor at end.
- 300 ms glow/zoom animation on the input border (CSS `@keyframes`).
- Green outline + glow while editing (`--color-accent` / a green variant).
- Immediately call new Tauri command `check_rename_permission` to verify the file is renameable (parent writable,
  no immutable/SIP/lock flags). On failure: cancel rename, show top-right notification with reason.
- On read-only volumes: show ModalDialog alert "This is a read-only volume. Renaming isn't possible here." with OK.
- Suppress app-level shortcuts while rename is active (same mechanism as open dialogs — set a flag in
  `keyboard-handler.ts` or reuse dialog-open check). Standard text-editing shortcuts (Cmd+C/A/Z/X/V) work normally.

## Saving / closing

| Trigger                                   | Action                  |
|-------------------------------------------|-------------------------|
| Enter                                     | Save (after validation) |
| Escape                                    | Discard                 |
| Click elsewhere / Tab / focus leaves pane | Discard                 |
| Drag event starts (internal or external)  | Discard                 |
| Scroll beyond cumulative 200 px threshold | Discard                 |
| Sort change / toggle hidden files         | Discard                 |

## Validation (real-time, on each keystroke)

All checks operate on the **trimmed** value (leading/trailing whitespace silently stripped for checking/saving,
but kept in the input while typing).

### Error state (red border + glow)

- Contains disallowed character (`/` or `\0` on macOS; future: per-OS, add TODO)
- Empty or whitespace-only after trim
- Name >= 255 bytes or resulting path >= 1024 bytes
- Extension changed when setting is "No" (see Settings below)

On Enter in error state: shake animation (CSS `@keyframes`) + top-right notification with reason. Notification
clears on next keypress or click.

On click-elsewhere in error state: discard (revert name).

### Warning state (yellow border + glow)

- New name matches an existing sibling file (case-insensitive on APFS).
    - Exception: case-only rename of the same file (same inode) — no warning.

### Filename validation utility

Extract to `apps/desktop/src-tauri/src/file_system/validation.rs` (new module). Reusable for rename, mkdir,
and file transfers. Contains:

- `validate_filename(name: &str) -> Result<(), ValidationError>` — checks disallowed chars, empty, byte length
- `validate_path_length(path: &Path) -> Result<(), ValidationError>`
- Per-OS logic with TODO for future platforms

Frontend mirror: `apps/desktop/src/lib/utils/filename-validation.ts` for instant keystroke feedback. Calls
backend `check_rename_validity` command for authoritative server-side check before saving.

## Conflict resolution

When saving and the new name collides with an existing sibling:

1. Show ModalDialog (new `rename-conflict` dialog ID) with:
    - Original file info (size, last modified) and conflicting file info, styled like TransferDialog conflicts.
    - Buttons: `[Overwrite and trash old file]` `[Overwrite and delete old file]` `[Cancel]` `[Continue renaming]`
    - Enter = Overwrite and trash, Escape = Continue renaming
2. "Overwrite and trash" calls backend rename with `force: true` + move-to-trash for old file.
3. "Overwrite and delete" calls backend rename with `force: true` + permanent delete of old file.
4. "Cancel" reverts to original name, closes editor.
5. "Continue renaming" returns to editing state.

## Extension change confirmation

When saving and the extension changed (and setting is "Always ask"):

- ModalDialog: "Are you sure you want to change the extension from ".{old}" to ".{new}"? Your file may open in a
  different app next time you open it."
- Buttons: `[Keep .{old}]` `[Use .{new}]`
- Checkbox: `[ ] Always allow extension changes` — sets setting to "Yes".

New setting in settings store: `allowFileExtensionChanges: 'yes' | 'no' | 'ask'` (default: `'ask'`).
Visible in Settings > General > File operations.

## Backend changes

### Tauri commands (new)

1. `check_rename_permission(path: String) -> Result<(), String>` — checks file renameability (parent writable,
   not immutable, not SIP-protected, not locked).
2. `check_rename_validity(dir: String, old_name: String, new_name: String) -> Result<RenameCheck, String>` —
   returns validity status and whether a conflict exists (with conflicting file metadata if so).
3. `rename_file(from: String, to: String, force: bool) -> Result<(), String>` — performs the rename. If `force`,
   proceeds even if destination exists. Uses `std::fs::rename` (which renames symlinks, not targets).

### Volume trait

Extend `fn rename(&self, from: &Path, to: &Path)` signature to `fn rename(&self, from: &Path, to: &Path, force: bool)`.
Update all implementations (LocalPosixVolume, InMemoryVolume, MtpVolume). When `force` is false: check destination
exists and return `AlreadyExists` error. When `force` is true: proceed (POSIX `rename` silently overwrites).

### Validation module

New file: `src/file_system/validation.rs`. Extracted from `write_operations/helpers.rs` where applicable.
Both modules can share constants (`MAX_NAME_BYTES`, `MAX_PATH_BYTES`).

## Post-rename behavior

- File watcher picks up the rename event and refreshes the listing.
- After refresh: cursor moves to the renamed file (find by new name).
- If the new name is dot-prefixed and hidden files are off: show info notification "Your file disappeared from view
  because hidden files aren't shown." Auto-dismiss on next navigation event, closeable with [x].
- If old name == new name (after trim): no-op, same as cancel.

## Edge cases

- External rename/delete of the file during editing: cancel rename, no error.
- Sorting or hidden-file toggle during editing: cancel rename.
- File watcher events during editing: keep editing, don't re-validate (backend will catch issues on save).
- MTP volumes: works via Volume trait, may be slow — accepted for now.
- Network drives: may be slow — accepted for now.
- Very long filenames that were clipped: full name shown in input, scrollable.

## Accessibility

- Input gets `role="textbox"` with `aria-label="Rename {filename}"`.
- `aria-live="assertive"` region announces validation errors.
- `aria-invalid="true"` on error state.

## New components / files

| File                                                        | Purpose                          |
|-------------------------------------------------------------|----------------------------------|
| `src/lib/file-explorer/rename/InlineRenameEditor.svelte`    | The inline input component       |
| `src/lib/file-explorer/rename/rename-state.svelte.ts`       | Rename state management ($state) |
| `src/lib/file-explorer/rename/rename-activation.ts`         | Click-to-rename timer logic      |
| `src/lib/file-explorer/rename/RenameConflictDialog.svelte`  | Conflict resolution dialog       |
| `src/lib/file-explorer/rename/ExtensionChangeDialog.svelte` | Extension change confirmation    |
| `src/lib/utils/filename-validation.ts`                      | Frontend filename validation     |
| `src-tauri/src/file_system/validation.rs`                   | Backend validation module        |
| `docs/features/rename.md`                                   | Feature documentation            |

## MCP integration

Add to DualPaneExplorer.svelte exports:

- `startRename()` — activates rename on focused pane cursor item
- `cancelRename()` — discards any active rename
- `isRenaming()` — returns whether rename is active
