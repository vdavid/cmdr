# File viewer module (frontend)

Opens files read-only with instant load, virtual scrolling, and background search. Details: `DETAILS.md`.

Backend: `apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`. Viewer route: `apps/desktop/src/routes/viewer/CLAUDE.md`.

## Key files

- `open-viewer.ts`: one `WebviewWindow` per viewer; `open-viewer-for-path.ts` resolves a bare path's volume first (MCP).
- `binary-warning.ts`: pure `categorizeForViewerWarning(fileName)` classifies a file into a `category` (`image` /
  `document` / `binary`, or `null` = "don't warn" for text/source/unknown) plus an uppercased `ext` for the `binary`
  case, which the `viewer.binaryWarning.body` select turns into words, keeping the classifier locale-free. The viewer
  route renders a red banner for flagged files without a native viewer, only in Text mode. Suppressible per-instance
  (banner **Close**) or forever (**Never show this warning again**, flips `fileViewer.suppressBinaryWarning` in
  Settings > Advanced).
- Route: `src/routes/viewer/+page.svelte`: viewer UI with virtual scrolling, search bar, status bar.

**Don't trim the image set in `binary-warning.ts` to suppress rendered formats.** The page uses the backend's current or
remembered media kind, so supported images and PDFs never warn, even in Text mode. Unsupported formats such as `.cr2`,
`.avif`, `.ico`, `.docx`, and archives still warn. Trimming the classifier would silence those too.

## User interaction

- **F3** in file list opens the viewer for the file under the cursor.
- **Cmd+F / Ctrl+F** opens the search bar (case-insensitive, 100ms debounce); **Enter / Shift+Enter** = next/previous
  match; **Escape** closes the search bar (if open) else the window.
- **W** toggles word wrap (per-line heights for FullLoad, averaged for others).
- **1 / 2 / 3** switch Text / Binary / Hex; **0** returns to Media when the file has an image or PDF viewer. Binary and
  Hex use original bytes, not decoded lines.

## Architecture (summary)

- **Virtual scrolling**: only visible lines rendered. Fixed 18px line height; per-line DOM-measured heights when wrap is
  on (FullLoad), averaged heights otherwise.
- **Session-based**: `viewer_open` returns a session ID passed to all operations; `viewer_close` frees resources.
- **Three backends, chosen by Rust on file size**: FullLoad (<1MB, in RAM), ByteSeek (instant, no pre-scan, byte-offset
  seeks, approximate line numbers), LineIndex (after a background scan, exact line seeks). ByteSeek scrolls by byte
  fraction; it switches to exact line seeks once the indexer finishes.
- **Background search**: frontend calls `search_start`, polls `search_poll` until done or canceled.
- **Multiple viewers**: each window has a unique label (`viewer-${timestamp}`). No limit.

Rationale for each in `DETAILS.md` § Key decisions.

## Gotchas (WebKit / load-bearing)

These guard against macOS WebKit crashes and toggle loops. Keep them; the why is in `DETAILS.md`.

- **Double `requestAnimationFrame` before `window.close()`.** WebKit on macOS can crash if you destroy a `WebPageProxy`
  while it recalculates content insets; one rAF isn't enough (the current frame must complete AND the next start). Also
  do NOT call `setFocus()` on another window before closing: that can trigger the dying window to recalculate.
- **The `canClose` flag gates `closeWindow()`**, queuing an Escape pressed before the mount settles (WebKit crashes
  closing a half-initialized window). It flips right after mount, ❌ not when the open resolves: a phone pull can run
  for minutes.
- **Word-wrap menu sync is two-way; keep the `fromMenu` guard.** "W" calls `viewerSetWordWrap` to update the menu's
  checked state; the menu item emits `viewer-word-wrap-toggled` back. The `fromMenu` parameter prevents an infinite
  toggle loop.

Other behavior gotchas (window position not remembered across sessions, lossy-UTF-8 binary display, byte-offset → UTF-16
search conversion, `needsFetch()` three-point sampling): `DETAILS.md` § Gotchas.

## Development

```typescript
import { openFileViewer } from '$lib/file-viewer/open-viewer'
await openFileViewer('/path/to/file.txt')
```
