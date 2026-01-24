# File viewer — task list

## Tasks

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
- [x] Create PR
