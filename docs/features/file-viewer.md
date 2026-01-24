# File viewer

View a file's text content in a dedicated window via F3. Supports instant viewing of files of any size with virtual
scrolling and background search.

## User interaction

Press **F3** to open the file under the cursor in a new viewer window. Multiple viewer windows can be open at the same
time. Each window remembers its size and position between sessions.

### Viewing

- Content is displayed with line numbers in a monospace font.
- Virtual scrolling: only visible lines are rendered, so even multi-GB files open instantly.
- A status bar shows file name, line count (when known), file size, and backend mode.
- Text is selectable for copying.

### Search

- **Cmd+F** (or **Ctrl+F**): Open the search bar.
- Type to find matches (case-insensitive, 300ms debounce). Match count is shown.
- For large files, search runs in the background with a progress indicator.
- **Enter**: Jump to next match.
- **Shift+Enter**: Jump to previous match.
- Matches are highlighted. The active match uses a distinct color.
- Search wraps around at the end/beginning of the file.

### Keyboard shortcuts

- **Escape**: Close search bar if open, or close the viewer window.
- **Cmd+F / Ctrl+F**: Open or focus the search bar.
- **Enter / Shift+Enter**: Next/previous match (when search is open).

### Limitations

- Binary files are shown with lossy UTF-8 decoding (replacement characters for invalid bytes).
- Directories can't be viewed (F3 does nothing when the cursor is on a folder).

## Architecture

### Three-backend strategy

The viewer uses a `FileViewerBackend` trait with three implementations, chosen based on file size:

1. **FullLoadBackend** (files ≤ 1 MB): Loads entire file into memory. Instant random access by line number, byte
   offset, or fraction. Full search in RAM.

2. **ByteSeekBackend** (files > 1 MB, initial): Opens the file handle immediately with no pre-scan. Seeks by byte
   offset or fraction. Scans backward up to 8 KB to find a newline boundary. Used as the instant-open strategy before
   the line index finishes building.

3. **LineIndexBackend** (files > 1 MB, after scan): Builds a sparse index of newline positions (one checkpoint every
   256 lines). Memory: O(total_lines / 256). Supports O(1) line-based seeking. Built in a background thread using
   SIMD-accelerated `memchr` for newline scanning.

### Session orchestrator

`ViewerSession` manages the lifecycle:
- Opens with FullLoad for small files, ByteSeek for large files
- For large files, spawns a background thread to build the LineIndex
- When the index is ready, transparently upgrades from ByteSeek to LineIndex
- Manages search state (start, poll, cancel) with cancellation support

### Tauri commands

- `viewer_open(path)` → session ID + metadata + initial lines
- `viewer_get_lines(session_id, target_type, target_value, count)` → line chunk
- `viewer_search_start(session_id, query)` → starts background search
- `viewer_search_poll(session_id)` → matches + progress + status
- `viewer_search_cancel(session_id)` → cancels running search
- `viewer_close(session_id)` → frees resources

### Frontend

- **Route**: `apps/desktop/src/routes/viewer/+page.svelte` — virtual scrolling viewer
- **Window opener**: `apps/desktop/src/lib/file-viewer/open-viewer.ts`
- **Key binding**: F3 handler in `DualPaneExplorer.svelte` calls `openViewerForCursor()`
- **Function key bar**: F3 button in `FunctionKeyBar.svelte`
- **Tauri wrappers**: `viewerOpen()`, `viewerGetLines()`, etc. in `$lib/tauri-commands.ts`

### Virtual scrolling

The frontend only renders lines that are in or near the viewport:
- Line height is fixed at 18px
- A scroll spacer div provides the full scrollable height
- On scroll, the visible range is recalculated and lines are fetched from the backend
- Lines are cached client-side to avoid redundant requests
- Buffer of 50 lines above/below the viewport for smooth scrolling

### Window management

Each viewer opens as a new `WebviewWindow` with a unique label (`viewer-{timestamp}`). The file path is passed as a URL
query parameter. Window size and position are persisted by `tauri-plugin-window-state`.
