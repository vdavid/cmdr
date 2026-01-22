# New folder

Create a new folder in the current directory via F7.

## User interaction

Press **F7** to open the "New folder" dialog. The dialog shows:

- Title: "New folder"
- Subtitle: "Create folder in {current directory name}"
- A text input for the folder name
- OK and Cancel buttons

### Pre-fill behavior

The input is pre-filled with the name of the entry under the cursor: for files, the extension is removed; for
directories, the name is used as-is. If the cursor is on the ".." entry, the input starts empty.

### Validation

The folder name is validated in real time against the current directory listing:

- If a folder with that name already exists: "There is already a folder by this name in this folder."
- If a file with that name already exists: "There is already a file by this name in this folder."
- The OK button is disabled while there's an error or the input is empty.

Validation is reactive to the file watcher — if the conflicting file is deleted externally, the error clears. If a new
file is created externally with that name, the error appears.

### Keyboard shortcuts

- **Enter**: Confirm (create folder)
- **Escape**: Cancel and close dialog

## Implementation

### Backend

- **Command**: `create_directory(parent_path, name)` in `apps/desktop/src-tauri/src/commands/file_system.rs`
- Validates name (non-empty, no `/` or null characters)
- Uses `std::fs::create_dir` with descriptive error messages
- Supports tilde expansion for the parent path

### Frontend

- **Dialog**: `apps/desktop/src/lib/file-explorer/NewFolderDialog.svelte`
- **Utilities**: `apps/desktop/src/lib/file-explorer/new-folder-utils.ts`
- **Integration**: F7 handler in `DualPaneExplorer.svelte`
- **Tauri wrapper**: `createDirectory()` in `$lib/tauri-commands.ts`

The new folder appears automatically in the file list via the existing file watcher (no manual refresh needed).
