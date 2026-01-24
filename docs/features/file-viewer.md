# File viewer

View a file's text content in a dedicated window via F3. Includes search with match highlighting.

## User interaction

Press **F3** to open the file under the cursor in a new viewer window. Multiple viewer windows can be open at the same
time. Each window remembers its size and position between sessions.

### Viewing

- File content is displayed with line numbers in a monospace font.
- A status bar shows the file name, line count, and file size.
- Text is selectable for copying.

### Search

- **Cmd+F** (or **Ctrl+F**): Open the search bar.
- Type to find matches (case-insensitive). The match count is shown.
- **Enter**: Jump to next match.
- **Shift+Enter**: Jump to previous match.
- Matches are highlighted in the content. The active match uses a distinct color.
- Search wraps around at the end/beginning of the file.

### Keyboard shortcuts

- **Escape**: Close search bar if open, or close the viewer window.
- **Cmd+F / Ctrl+F**: Open or focus the search bar.
- **Enter / Shift+Enter**: Next/previous match (when search is open).

### Limitations

- Not optimized for very large files. The entire file is loaded into memory.
- Binary files are shown with lossy UTF-8 decoding (replacement characters for invalid bytes).
- Directories can't be viewed (F3 does nothing when the cursor is on a folder).

## Implementation

### Backend

- **Command**: `read_file_content(path)` in `apps/desktop/src-tauri/src/commands/file_system.rs`
- Returns `FileContentResult { content, lineCount, size, fileName }`
- Uses `String::from_utf8_lossy` for binary-safe reading
- Supports tilde expansion for the path

### Frontend

- **Route**: `apps/desktop/src/routes/viewer/+page.svelte` — the viewer window's page
- **Search utilities**: `apps/desktop/src/lib/file-viewer/viewer-search.ts`
- **Window opener**: `apps/desktop/src/lib/file-viewer/open-viewer.ts`
- **Key binding**: F3 handler in `DualPaneExplorer.svelte` calls `openViewerForCursor()`
- **Function key bar**: F3 button in `FunctionKeyBar.svelte`
- **Tauri wrapper**: `readFileContent()` in `$lib/tauri-commands.ts`

### Window management

Each viewer opens as a new `WebviewWindow` with a unique label (`viewer-{timestamp}`). The file path is passed as a URL
query parameter. Window size and position are persisted by `tauri-plugin-window-state`.
