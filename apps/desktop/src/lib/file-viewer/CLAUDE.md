# File viewer module (frontend)

Opens files in a read-only viewer with instant load for any file size, virtual scrolling, and background search. Full
details (decisions, the full gotcha catalog): [DETAILS.md](DETAILS.md).

Backend counterpart: [`src-tauri/src/file_viewer/CLAUDE.md`](../../../src-tauri/src/file_viewer/CLAUDE.md) for the three
backend strategies, session lifecycle, and background search. The viewer route shell:
[`src/routes/viewer/CLAUDE.md`](../../routes/viewer/CLAUDE.md).

## Key files

- `open-viewer.ts`: `openFileViewer(filePath)` creates a new `WebviewWindow` with a unique label.
- `binary-warning.ts`: pure `categorizeForViewerWarning(fileName)` that classifies a file as `image` / `document` /
  `<EXT-uppercased>` (or "don't warn" for text/source/unknown). The viewer route renders a red banner whenever
  `shouldWarn`. Suppressible per-instance (banner **Close**) or forever (**Never show this warning again**, flips
  `fileViewer.suppressBinaryWarning` in Settings > Advanced).
- Route: `src/routes/viewer/+page.svelte`: viewer UI with virtual scrolling, search bar, status bar.

**Don't trim the image set in `binary-warning.ts` to suppress rendered formats.** Rendered media is suppressed by the
authoritative backend `kind`, not by this list: the viewer page shows the banner only when
`!isMedia && warning.shouldWarn`. So a rendered image / PDF never shows it, while formats the classifier promotes to
neither (RAW like `.cr2`/`.nef`, `.avif`, `.ico`, `.docx`, `.epub`, archives) still warn. Trimming the image set here
also silences the unrendered ones, which then show raw bytes with no nudge.

## User interaction

- **F3** in file list opens the viewer for the file under the cursor.
- **Cmd+F / Ctrl+F** opens the search bar (case-insensitive, 100ms debounce); **Enter / Shift+Enter** = next/previous
  match; **Escape** closes the search bar (if open) else the window.
- **W** toggles word wrap (per-line heights for FullLoad, averaged for others).

## Architecture (summary)

- **Virtual scrolling**: only visible lines rendered. Fixed 18px line height; per-line pretext heights when wrap is on
  (FullLoad), averaged heights otherwise.
- **Session-based**: `viewer_open` returns a session ID passed to all operations; `viewer_close` frees resources.
- **Three backends, chosen by Rust on file size**: FullLoad (<1MB, in RAM), ByteSeek (instant, no pre-scan, byte-offset
  seeks, approximate line numbers), LineIndex (after a background scan, exact line seeks). ByteSeek scrolls by byte
  fraction; it switches to exact line seeks once the indexer finishes.
- **Background search**: frontend calls `search_start`, polls `search_poll` until done or canceled.
- **Multiple viewers**: each window has a unique label (`viewer-${timestamp}`). No limit.

Rationale for each in [DETAILS.md](DETAILS.md) § Key decisions.

## Gotchas (WebKit / load-bearing)

These guard against macOS WebKit crashes and toggle loops. Keep them; the why is in [DETAILS.md](DETAILS.md).

- **Double `requestAnimationFrame` before `window.close()`.** WebKit on macOS can crash if you destroy a `WebPageProxy`
  while it recalculates content insets; one rAF isn't enough (the current frame must complete AND the next start). Also
  do NOT call `setFocus()` on another window before closing: that can trigger the dying window to recalculate.
- **The `windowReady` flag gates `closeWindow()`.** If Escape is pressed before mount finishes, close is queued. WebKit
  crashes if you close a window before its content process finished initializing; the flag is set after a post-mount
  `requestAnimationFrame`.
- **Word-wrap menu sync is two-way; keep the `fromMenu` guard.** "W" calls `viewerSetWordWrap` to update the menu's
  checked state; the menu item emits `viewer-word-wrap-toggled` back. The `fromMenu` parameter prevents an infinite
  toggle loop.

Other behavior gotchas (window position not remembered across sessions, lossy-UTF-8 binary display, byte-offset → UTF-16
search conversion, `needsFetch()` three-point sampling): [DETAILS.md](DETAILS.md) § Gotchas.

## Development

```typescript
import { openFileViewer } from '$lib/file-viewer/open-viewer'
await openFileViewer('/path/to/file.txt')
```

Test large files via `dd if=/dev/zero of=large.txt bs=1m count=1000` (1GB file).
