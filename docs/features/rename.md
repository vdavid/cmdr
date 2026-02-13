# Rename

Inline rename for files and folders in both panes.

## How to start renaming

There are four ways to rename a file or folder:

- **F2** (or **Shift+F6**): Renames the entry under the cursor. Also available via Edit > Rename.
- **Context menu**: Right-click a file or folder and select "Rename".
- **Click-to-rename**: Click the name of the entry already under the cursor and hold for about 800 ms without moving
  more than 10 px. If you double-click instead, it opens the file or folder as usual.

Renaming always operates on the cursor item. Selection is preserved and doesn't affect which file gets renamed. The ".."
parent entry can't be renamed.

## Inline editing

When you start renaming, the static name is replaced by an input field:

- The filename is pre-selected excluding the extension, with the cursor at the end. This makes it easy to type a new
  base name while keeping the extension.
- A brief glow/zoom animation (300 ms) highlights the input.
- The border glows green while you type.
- Standard text shortcuts (Cmd+C, Cmd+A, Cmd+Z, Cmd+X, Cmd+V) work as expected. App-level shortcuts are suppressed
  while rename is active.

## Saving and canceling

| Trigger                                        | What happens                          |
|------------------------------------------------|---------------------------------------|
| Enter                                          | Saves the new name (after validation) |
| Escape                                         | Discards changes                      |
| Click elsewhere, Tab, or focus leaves the pane | Discards changes                      |
| Drag starts (internal or external)             | Discards changes                      |
| Scroll more than 200 px                        | Discards changes                      |
| Sort change or hidden-files toggle             | Discards changes                      |

If the trimmed name is the same as the original, it's treated as a cancel (no-op).

## Validation

The name is validated in real time as you type. Leading and trailing whitespace is stripped when checking and saving,
but
kept visible in the input while you edit.

### Error state (red border)

You'll see a red border if:

- The name contains `/` or null characters (macOS restriction)
- The name is empty or whitespace-only
- The name is 255 bytes or longer, or the resulting path is 1024 bytes or longer
- The extension changed and the "Allow extension changes" setting is set to "No"

Pressing Enter while in error state triggers a shake animation and shows a notification in the top-right corner
explaining what's wrong. Clicking elsewhere discards the invalid name.

### Warning state (yellow border)

You'll see a yellow border if the new name matches an existing file in the same folder (case-insensitive on APFS).
Exception: changing only the letter casing of the same file (for example, "readme" to "README") doesn't produce a
warning.

## Conflict resolution

If you save a name that collides with an existing file, a dialog appears showing both files' sizes and modification
dates side by side. You can:

- **Overwrite and trash old file**: Renames your file and moves the old one to the trash.
- **Overwrite and delete old file**: Renames your file and permanently deletes the old one.
- **Cancel**: Reverts to the original name.
- **Continue renaming**: Returns to the editing state so you can pick a different name.

Enter defaults to "Overwrite and trash". Escape returns to editing.

## Extension change confirmation

If you change the file extension and the "Allow extension changes" setting is "Always ask" (the default), a dialog
appears:

> "Are you sure you want to change the extension from ".txt" to ".md"? Your file may open in a different app next time
> you open it."

Options:

- **Keep .txt**: Reverts to the original extension.
- **Use .md**: Applies the new extension.
- **Always allow extension changes** (checkbox): Sets the preference to "Yes" so the dialog doesn't appear again.

## Settings

In Settings > General > File operations:

| Setting                      | Values              | Default    | Description                                                                      |
|------------------------------|---------------------|------------|----------------------------------------------------------------------------------|
| Allow file extension changes | Yes, No, Always ask | Always ask | Controls whether extension changes are allowed, blocked, or require confirmation |

## After renaming

- The file list refreshes automatically via the file watcher.
- The cursor moves to the renamed file.
- If you renamed a file to a dot-prefixed name (making it hidden) while hidden files aren't shown, a notification tells
  you: "Your file disappeared from view because hidden files aren't shown."

## Edge cases

- **Read-only volumes**: An alert dialog appears: "This is a read-only volume. Renaming isn't possible here."
- **Permission denied**: If the parent folder isn't writable or the file is locked/immutable, the rename is canceled and
  a notification shows the reason.
- **External changes**: If the file is renamed or deleted externally while you're editing, the rename is canceled
  gracefully.
- **Hidden files**: Renaming to a dot-prefixed name is allowed. If hidden files are off, the file disappears with an
  info notification.
- **Long filenames**: If the original name was clipped in the list, the full name appears in the input and is
  scrollable.

## Accessibility

- The rename input has `aria-label="Rename {filename}"`.
- An `aria-live="assertive"` region announces validation errors as you type.
- `aria-invalid="true"` is set when the name is invalid.
- The conflict and extension change dialogs use `role="alertdialog"` with `aria-describedby` pointing to the dialog
  description.

## Implementation

### Backend

- **Validation**: `apps/desktop/src-tauri/src/file_system/validation.rs` — reusable filename and path validation
- **Commands**: `check_rename_permission`, `check_rename_validity`, `rename_file` in `commands/file_system.rs`
- **Volume trait**: `rename()` with `force` parameter across all volume implementations

### Frontend

- **Inline editor**: `src/lib/file-explorer/rename/InlineRenameEditor.svelte`
- **State management**: `src/lib/file-explorer/rename/rename-state.svelte.ts`
- **Click-to-rename**: `src/lib/file-explorer/rename/rename-activation.ts`
- **Save flow**: `src/lib/file-explorer/rename/rename-operations.ts`
- **Conflict dialog**: `src/lib/file-explorer/rename/RenameConflictDialog.svelte`
- **Extension dialog**: `src/lib/file-explorer/rename/ExtensionChangeDialog.svelte`
- **Validation**: `src/lib/utils/filename-validation.ts`
- **MCP exports**: `startRename()`, `cancelRename()`, `isRenaming()` in `DualPaneExplorer.svelte`
