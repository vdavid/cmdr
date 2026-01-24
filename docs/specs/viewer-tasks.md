# File viewer — task list

## Phase 1: Basic viewer (done)

- [x] Create this task list
- [x] Add `read_file_content` Rust command (reads file, returns text + metadata)
- [x] Add Rust tests for the command
- [x] Create `/viewer` SvelteKit route with `FileViewer` component
- [x] Wire F3 key binding in `DualPaneExplorer` and enable F3 in `FunctionKeyBar`
- [x] Open viewer in a new Tauri window (unique label per file, multiple allowed)
- [x] Persist window size/position via `tauri-plugin-window-state`
- [x] Display file content with line numbers and monospace font
- [x] Add search functionality (Cmd+F / Ctrl+F, find next/previous, match count)
- [x] ESC closes the viewer window (or closes search bar if open)
- [x] Add `readFileContent` wrapper in `tauri-commands.ts`
- [x] Write Vitest unit tests for viewer search utilities
- [x] Document in `docs/features/file-viewer.md`
- [x] Run checker script, fix any issues

## Phase 2: Three-backend architecture + virtual scrolling

- [x] Design `FileViewerBackend` trait with SeekTarget, LineChunk, SearchMatch types
- [x] Implement `FullLoadBackend` (files ≤ 1 MB, in-memory)
- [x] Implement `ByteSeekBackend` (byte-offset seeking, no pre-scan, 8 KB backward cap)
- [x] Implement `LineIndexBackend` (sparse index, checkpoint every 256 lines, cancellable scan)
- [x] Implement `ViewerSession` orchestrator (picks strategy, background upgrade)
- [x] Add Tauri commands: viewer_open, viewer_get_lines, viewer_search_start/poll/cancel, viewer_close
- [x] Write 64 Rust tests covering all three backends + session orchestrator
- [x] Update frontend to virtual scrolling (renders only visible lines)
- [x] Add new TypeScript wrappers (viewerOpen, viewerGetLines, etc.)
- [x] Implement backend-side search with polling + progress indicator
- [x] Remove old `read_file_content` command and `viewer-search.ts` utilities
- [x] Update documentation
- [x] Run checks, fix issues
