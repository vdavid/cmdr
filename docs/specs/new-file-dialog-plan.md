# Shift+F4: Create new file and edit

Total Commander-style: Shift+F4 opens a dialog to name a new file, creates it (empty), and opens it in the default
text editor.

## Context

- F4 already opens the file under cursor in the default editor (`file.edit` → `openInEditor`).
- F7 creates a new folder via `NewFolderDialog` — our primary reference pattern for the UI, validation, dialog wiring,
  and synthetic diff emission.
- The `Volume` trait already has `create_file(path, content)` implemented for local POSIX and in-memory volumes.
- No `create_file` Tauri command exists yet — we need to add one.

## Design decisions

- **No AI suggestions.** Unlike mkdir, file name suggestions aren't useful here — users always know what file they want
  to create. Keeps the dialog simple and fast.
- **Simpler than NewFolderDialog.** Same validation (disallowed chars, name length, path length, conflict check), but no
  AI suggestions section, no timeout warning (file creation is near-instant for local volumes). We do keep the timeout
  handling in the Rust command for MTP volumes, but the dialog can be more minimal.
- **Pre-fill from cursor.** Like mkdir, pre-fill the input with the name of the entry under cursor. Unlike mkdir, keep
  the extension (the user is creating a file, so the extension is relevant context). If cursor is on a directory or "..",
  leave the input empty. (The ".." case is handled by index math: `backendIndex < 0` returns early, same as
  `getInitialFolderName`.)
- **Conflict behavior.** If a file with that name already exists, show an error (same as mkdir). Don't overwrite. The
  user can F4 to edit the existing file instead.
- **Open after creation.** Immediately call `openInEditor` after successful creation — this is the whole point of
  Shift+F4 vs just creating an empty file. This is safe because `createFile` is awaited before the `onCreated` callback
  fires, so the file always exists on disk before the editor opens.
- **Cursor moves to new file.** Same pattern as mkdir: listen for `directory-diff` to move cursor to the newly created
  file. Reuse the existing `moveCursorToNewFolder` function (it's name-agnostic — works for any entry type).
- **Always open as text.** - Even if creating `a.mov`, TextEdit should open it, as a plain text file.

## Milestone 1: Backend — `create_file` Tauri command

**Intent**: Expose file creation through IPC so the frontend can create files on any volume.

### 1.1 Add `create_file` Tauri command in `file_system.rs`

Mirror `create_directory` closely:

- Signature: `create_file(app, volume_id, parent_path, name) -> Result<String, IpcError>`
- Validation: name not empty, no `/` or `\0`
- Resolve volume, expand tilde for root
- Call `volume.create_file(&new_path, b"")` (empty content) inside `spawn_blocking` with 5s timeout
- Error mapping: `AlreadyExists` → `"'name' already exists"`, `PermissionDenied` → `"Permission denied: ..."`,
  generic → `"Couldn't create file: ..."`. Use file-specific wording — don't copy "folder" from mkdir.
- Emit synthetic diff (see 1.2)
- Return the full path as a string

Add a `create_file_core` function (testable without `AppHandle`), same pattern as `create_directory_core`.

### 1.2 Refactor synthetic diff emission

`emit_synthetic_mkdir_diff` is entirely entry-type-agnostic (`get_single_entry` handles both files and dirs). Rename it
to `emit_synthetic_entry_diff(app, entry_path, parent_path)` and call it from both `create_directory` and `create_file`.
This avoids duplicating ~30 lines of identical code.

### 1.3 Register the command

Add `create_file` to the Tauri command builder in `apps/desktop/src-tauri/src/lib.rs` (that's where all commands are
registered — look for `create_directory` to find the exact spot).

### 1.4 Add frontend IPC wrapper

In `apps/desktop/src/lib/tauri-commands/file-listing.ts` (where `createDirectory` already lives), add:
```ts
export async function createFile(parentPath: string, name: string, volumeId?: string): Promise<string> {
    return invoke<string>('create_file', { volumeId, parentPath, name })
}
```

Also re-export `createFile` from `apps/desktop/src/lib/tauri-commands/index.ts` — the project convention is to import
from `$lib/tauri-commands`, never from sub-files.

### 1.5 Test

- Rust unit test for `create_file_core`: happy path, empty name, invalid chars, already exists.
- Can reuse the testing patterns from `create_directory_core` tests (if they exist) or the in-memory volume tests.

## Milestone 2: Frontend — New file dialog

**Intent**: Provide a dialog that matches the mkdir UX but is simpler (no AI suggestions, no timeout warning banner).

### 2.1 Create `NewFileDialog.svelte`

Place in `src/lib/file-operations/mkfile/NewFileDialog.svelte` (parallel to `mkdir/`).

Props — same shape as `NewFolderDialog` but semantically for files:
```ts
interface Props {
    currentPath: string
    listingId: string
    showHiddenFiles: boolean
    initialName: string
    volumeId: string          // Required (not optional) — matches NewFolderDialogPropsData pattern
    onCreated: (fileName: string) => void
    onCancel: () => void
}
```

UI:
- Title: "New file"
- Subtitle: "Create file in **{currentDirName}**"
- Single text input with placeholder "Example: notes.txt"
- Same validation as NewFolderDialog (sync: disallowed chars, name length, path length; async: conflict check via
  `findFileIndex`)
- Buttons: Cancel (secondary), OK (primary) — matches NewFolderDialog's button label for consistency
- On confirm: `createFile(currentPath, trimmed, volumeId)`, then `onCreated(trimmed)`

Why simpler than mkdir:
- No AI suggestions (not useful for filenames — users know exactly what they want)
- No timeout warning banner (file creation is near-instant; timeout handling still exists in Rust but we show a generic
  error instead of a dedicated banner)

### 2.2 Add helper function in `mkfile/`

- `new-file-operations.ts`: `getInitialFileName(paneRef, listingId, showHiddenFiles, getFileAt)` — like
  `getInitialFolderName` but returns the full filename (with extension) for files, and empty string for directories.
  The ".." entry returns empty via index math (`backendIndex < 0`), same as the folder version.

For cursor movement after creation, reuse `moveCursorToNewFolder` from `mkdir/new-folder-operations.ts` — it's not
folder-specific (it just finds an entry by name in the listing). Optionally rename it to `moveCursorToNewEntry` for
clarity — but if that touches too many call sites, a re-export or alias from `mkfile/` is fine too.

### 2.3 Register `new-file-confirmation` dialog ID

Add to `SOFT_DIALOG_REGISTRY` in `dialog-registry.ts`:
```ts
{ id: 'new-file-confirmation', description: 'Opened by the new-file tool, not directly' },
```

(Using `new-file-confirmation` instead of `mkfile-confirmation` — `mkfile` is not a standard Unix command like `mkdir`,
and the other IDs in the registry use full words: `transfer-confirmation`, `delete-confirmation`, etc.)

### 2.4 Wire into dialog-state

In `apps/desktop/src/lib/file-explorer/pane/dialog-state.svelte.ts`:
- Define `NewFileDialogPropsData` interface (parallel to `NewFolderDialogPropsData`):
  `{ currentPath: string, listingId: string, showHiddenFiles: boolean, initialName: string, volumeId: string }`
- Add `showNewFileDialog` and `newFileDialogProps` state (same pattern as `showNewFolderDialog`)
- Add `showNewFile(props)` opener
- Add `handleNewFileCreated(fileName)` — close dialog, refocus pane, call `moveCursorToNewFolder` (or renamed
  `moveCursorToNewEntry`) for cursor movement, then invoke the `onOpenInEditor` callback (see below)
- Add `handleNewFileCancel()` — close dialog, null props, refocus
- Update `closeConfirmationDialog()` to also close the new file dialog if open
- Update `isConfirmationDialogOpen()` to include `showNewFileDialog` (this gates pane swaps — without it, users could
  swap panes while the dialog is open)

**`openInEditor` access**: `dialog-state.svelte.ts` is a pure state module with no IPC imports. Add an `onOpenInEditor`
callback to the `DialogStateDeps` interface. `DualPaneExplorer` provides the callback implementation — it needs to newly
import `openInEditor` from `$lib/tauri-commands/file-actions`. The handler constructs the full path by joining
`currentPath` and `fileName` properly (watch for double `/` when `currentPath` is root `/`).

**Sequencing**: `moveCursorToNewFolder` is fire-and-forget (listens for a future `directory-diff` event). Call
`onOpenInEditor` immediately after — don't wait for cursor movement. The editor opening doesn't depend on cursor
position, and the cursor will move on its own when the diff event arrives.

### 2.5 Wire into DialogManager

In `DialogManager.svelte`, add the `{#if}` block for `NewFileDialog`, passing props from dialog state. Follow the exact
pattern of the NewFolderDialog block.

### 2.6 Wire into DualPaneExplorer

Add `async openNewFileDialog()` export function, mirroring `openNewFolderDialog()`. Gets focused pane, path, volume ID,
listing ID, calls `getInitialFileName`, and opens the dialog via `dialogs.showNewFile(...)`.

### 2.7 Test

- Vitest unit test for `getInitialFileName`: file input returns full name with extension, directory input returns empty
  string.

## Milestone 3: Command registration and shortcut

**Intent**: Make the feature accessible via Shift+F4, command palette, and (later) menu.

### 3.1 Add command to registry

In `command-registry.ts`:
```ts
{ id: 'file.newFile', name: 'Create new file', scope: 'Main window/File list', showInPalette: true, shortcuts: ['⇧F4'] }
```

### 3.2 Add handler in `handleCommandExecute`

In `+page.svelte`, add case:
```ts
case 'file.newFile':
    void explorerRef?.openNewFileDialog()
    return
```

### 3.3 Add MCP tool

Follow the same pattern as `mkdir`:

In `apps/desktop/src-tauri/src/mcp/tools.rs`, add to `get_file_op_tools()`:
```rust
Tool::no_params("mkfile", "Create file in focused pane (triggers naming dialog)")
```

In `apps/desktop/src-tauri/src/mcp/executor.rs`, add `execute_mkfile` that emits `mcp-mkfile`:
```rust
fn execute_mkfile<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-mkfile", ())?;
    Ok(json!("OK: Create file dialog opened."))
}
```

In `+page.svelte`'s `setupMcpListeners()`, add:
```ts
await safeListenTauri('mcp-mkfile', () => {
    void explorerRef?.openNewFileDialog()
})
```

### 3.4 Test

- Manual test: Shift+F4 opens dialog, type a filename, confirm, file appears in listing, editor opens.
- Manual test: try creating a file that already exists — error shown.
- Manual test: Escape closes dialog without creating anything.
- MCP test: call `mkfile` tool via `./scripts/mcp-call.sh mkfile` — dialog opens.

## Milestone 4: Cleanup and docs

### 4.1 Update CLAUDE.md files

- Add `mkfile/` section to `src/lib/file-operations/CLAUDE.md` (parallel to the mkdir section)
- Update `src-tauri/src/commands/CLAUDE.md` file map if `create_file` added notable patterns

### 4.2 Run checks

`./scripts/check.sh` — ensure all lints, tests, and type checks pass.

## What NOT to do

- Don't add a native menu item yet — match the current pattern where mkdir (F7) also doesn't have a menu entry. Can be
  added later if needed.
- Don't add the command to `menuCommands` in `shortcuts-store.ts` — same reason.
- Don't over-engineer the dialog with features like file templates, content pre-fill, or encoding selection. Keep it
  minimal: name → create empty file → open in editor.
- Don't duplicate `moveCursorToNewFolder` — it's entry-type-agnostic, reuse it.
