# File viewer module (frontend)

Opens files in a read-only viewer with instant load for any file size, virtual scrolling, and background search.

## Key files

- `open-viewer.ts` — `openFileViewer(filePath)` creates new `WebviewWindow` with unique label
- Route: `apps/desktop/src/routes/viewer/+page.svelte` — viewer UI with virtual scrolling, search bar, status bar

## User interaction

- **F3** in file list opens viewer for file under cursor
- **Cmd+F / Ctrl+F** opens search bar (case-insensitive, 300ms debounce)
- **Enter / Shift+Enter** navigates to next/previous match
- **W** toggles word wrap (averaged-height virtual scroll)
- **Escape** closes search bar (if open) or closes window

## Architecture

- **Virtual scrolling** — only visible lines rendered. Fixed 18px line height (or averaged when wrap is on).
- **Session-based** — `viewer_open` returns session ID. All operations pass session ID. `viewer_close` frees resources.
- **Three backends** (chosen by Rust based on file size):
    - FullLoad (<1MB) — entire file in RAM
    - ByteSeek (instant) — no pre-scan, seeks by byte offset
    - LineIndex (after scan) — O(lines/256) memory, O(1) line seeks
- **Background search** — frontend calls `search_start`, polls `search_poll` until done or canceled
- **Multiple viewers** — each window has unique label (`viewer-${timestamp}`). No limit on open viewers.

## Gotchas

- **Window management** — `tauri-plugin-window-state` persists size/position per label. Each viewer label is unique, so
  position isn't remembered across sessions.
- **Binary files shown with lossy UTF-8** — replacement chars for invalid bytes. No binary mode.
- **Search uses byte offsets** — converted to UTF-16 code units for JS `String.substring()` compatibility.
- **Word wrap scroll position** — approximate when wrap is on (averaged line height). Only the visible chunk height is
  exact.
- **Menu integration** — viewer windows get their own menu via `viewer_setup_menu`. Word wrap CheckMenuItem syncs with
  "W" key presses via `viewer_set_word_wrap`.

## Development

**Open viewer programmatically**:

```typescript
import { openFileViewer } from '$lib/file-viewer/open-viewer'
await openFileViewer('/path/to/file.txt')
```

**Test large files**: Generate via `dd if=/dev/zero of=large.txt bs=1m count=1000` (1GB file).
