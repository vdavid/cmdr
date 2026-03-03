# File viewer module (frontend)

Opens files in a read-only viewer with instant load for any file size, virtual scrolling, and background search.

## Key files

- `open-viewer.ts` — `openFileViewer(filePath)` creates new `WebviewWindow` with unique label
- Route: `apps/desktop/src/routes/viewer/+page.svelte` — viewer UI with virtual scrolling, search bar, status bar

## User interaction

- **F3** in file list opens viewer for file under cursor
- **Cmd+F / Ctrl+F** opens search bar (case-insensitive, 100ms debounce)
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

## Key decisions

**Decision**: Three-tier backend strategy (FullLoad / ByteSeek / LineIndex) chosen automatically by file size.
**Why**: Opening a 10 GB log file must feel instant. FullLoad is fastest for small files (everything in RAM), but
impossible for large files. ByteSeek gives instant open for any file size by seeking to byte offsets without scanning,
but line numbers are approximate. LineIndex builds an O(lines/256) index in the background for exact line seeks. The
user sees progressively better behavior without choosing a mode.

**Decision**: Fraction-based seeking as the default scroll model for ByteSeek.
**Why**: Without a line index, the frontend can't ask for "line 50000" — the backend doesn't know where it is.
Instead, scrolling maps to a byte fraction of the file (e.g., 50% = seek to byte offset at file midpoint). The
frontend caches lines at the position it requested, not at the backend's reported line number, because the two
estimates can differ (different average line length assumptions). Once the background indexer finishes, it switches to
exact line seeks automatically.

**Decision**: Timestamp-based unique window labels (`viewer-${Date.now()}`).
**Why**: Each viewer needs its own Tauri `WebviewWindow` label. Using the file path would prevent opening the same file
twice. Timestamps are unique enough (millisecond resolution) and don't need escaping.

**Decision**: Double `requestAnimationFrame` before `window.close()`.
**Why**: WebKit on macOS can crash if you destroy a `WebPageProxy` while it's recalculating content insets. A single
`requestAnimationFrame` isn't enough — you need the current frame to complete AND the next one to start. This also
means you must NOT call `setFocus()` on another window before closing, as that can trigger the dying window to
recalculate.

**Decision**: Word wrap uses averaged line height for virtual scroll.
**Why**: When lines wrap, each line has a different rendered height. Measuring every line would require rendering the
entire file. Instead, the viewer measures the average height of currently-visible lines and uses that for the scroll
spacer. This is slightly inaccurate (scroll thumb position drifts) but keeps the O(1) virtual scroll contract. The
measurement effect depends on `scrollTop` rather than `visibleLines` to avoid a feedback loop:
`visibleLines -> measure -> avgHeight -> effectiveLineHeight -> visibleLines`.

**Decision**: Proportional scroll compensation when `effectiveLineHeight` changes.
**Why**: Toggling word wrap or updating the averaged height changes the total scroll height. Without compensation, the
viewport jumps to a different part of the file. Multiplying `scrollTop` by `newHeight / oldHeight` preserves the same
line at the top of the viewport, and since `ON ratio * OFF ratio = 1.0`, there's zero cumulative drift across toggles.

## Gotchas

**Gotcha**: Window position isn't remembered across sessions.
**Why**: `tauri-plugin-window-state` persists size/position per window label, but each viewer label is unique
(timestamp-based). There's no stable identifier to key on since the same file can be opened multiple times.

**Gotcha**: Binary files shown with lossy UTF-8 (replacement chars for invalid bytes, no binary mode).
**Why**: The viewer is designed for text/log files. Adding a hex/binary mode would require a completely different
rendering pipeline. Lossy display is good enough for quick inspection.

**Gotcha**: Search uses byte offsets internally, converted to UTF-16 code units for JS.
**Why**: Rust searches over raw bytes for speed. JS `String.substring()` uses UTF-16 code units. The backend does the
conversion so the frontend can highlight matches correctly in JavaScript strings.

**Gotcha**: The `windowReady` flag gates `closeWindow()` — if Escape is pressed before mount finishes, close is queued.
**Why**: WebKit crashes if you close a window before its content process has finished initializing. The flag is set
after a `requestAnimationFrame` post-mount, ensuring at least one paint cycle has completed.

**Gotcha**: Menu integration requires two-way sync for word wrap state.
**Why**: The "W" key and the menu CheckMenuItem both toggle word wrap. When "W" is pressed, the frontend calls
`viewerSetWordWrap` to update the menu's checked state. When the menu item is clicked, it emits
`viewer-word-wrap-toggled` back to the frontend. The `fromMenu` parameter prevents an infinite toggle loop.

**Gotcha**: `needsFetch()` samples three points instead of iterating the entire visible range.
**Why**: The visible range can be hundreds of lines. Checking every line against the cache on every scroll event would
be expensive. Sampling the first, middle, and last lines catches the common case (scrolling into uncached territory)
with O(1) work.

## Development

**Open viewer programmatically**:

```typescript
import { openFileViewer } from '$lib/file-viewer/open-viewer'
await openFileViewer('/path/to/file.txt')
```

**Test large files**: Generate via `dd if=/dev/zero of=large.txt bs=1m count=1000` (1GB file).
