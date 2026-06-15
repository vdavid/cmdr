# File viewer (frontend) details

Depth and rationale for the frontend file viewer. `CLAUDE.md` holds the must-knows that prevent silent breakage.

## Key decisions

- **Three-tier backend strategy (FullLoad / ByteSeek / LineIndex), chosen automatically by file size.** Opening a 10 GB
  log file must feel instant. FullLoad is fastest for small files (everything in RAM) but impossible for large files.
  ByteSeek gives instant open for any file size by seeking to byte offsets without scanning, but line numbers are
  approximate. LineIndex builds an O(lines/256) index in the background for exact line seeks. The user sees
  progressively better behavior without choosing a mode.
- **Fraction-based seeking as the default scroll model for ByteSeek.** Without a line index, the frontend can't ask for
  "line 50000" and the backend doesn't know where it is. Scrolling maps to a byte fraction of the file (50% = seek to
  the byte offset at the file midpoint). The frontend caches lines at the position it requested, not at the backend's
  reported line number, because the two estimates can differ (different average-line-length assumptions). Once the
  background indexer finishes, it switches to exact line seeks automatically.
- **Timestamp-based unique window labels (`viewer-${Date.now()}`).** Each viewer needs its own Tauri `WebviewWindow`
  label. Using the file path would prevent opening the same file twice. Timestamps are unique enough (millisecond
  resolution) and need no escaping.
- **Double `requestAnimationFrame` before `window.close()`.** WebKit on macOS can crash if you destroy a `WebPageProxy`
  while it's recalculating content insets. A single `requestAnimationFrame` isn't enough: the current frame must
  complete AND the next start. This also means you must NOT call `setFocus()` on another window before closing, as that
  can trigger the dying window to recalculate.
- **FullLoad files use `@chenglou/pretext` for per-line height calculation when word wrap is on.** Pretext runs
  `prepare()` on each line's text using canvas font metrics, then `layout()` computes wrapped height without rendering
  to the DOM. A prefix-sum array (`Float64Array`) gives O(1) `getLineTop(n)` and O(log n) `getLineAtPosition(y)`.
  Preparation runs async via `requestIdleCallback` (with a 2s timeout and 50k line cap). While it runs, the viewer falls
  back to averaged heights, so there's zero regression. Width changes call `reflow()` (re-runs `layout()` only,
  ~0.0002ms/line) instead of re-preparing. A generation counter discards stale preparations.
- **Word wrap uses averaged line height as fallback for ByteSeek/LineIndex files and while pretext prepares.** Measuring
  every wrapped line would require rendering the whole file. Instead, the viewer measures the average height of
  currently-visible lines and uses that for the scroll spacer. This is slightly inaccurate (scroll thumb drifts) but
  keeps the O(1) virtual-scroll contract. The measurement effect depends on `scrollTop` rather than `visibleLines` to
  avoid a feedback loop: `visibleLines -> measure -> avgHeight -> effectiveLineHeight -> visibleLines`.
- **Proportional scroll compensation when `effectiveLineHeight` changes.** Toggling word wrap or updating the averaged
  height changes the total scroll height. Without compensation, the viewport jumps to a different part of the file.
  Multiplying `scrollTop` by `newHeight / oldHeight` preserves the same line at the top of the viewport, and since
  `ON ratio * OFF ratio = 1.0`, there's zero cumulative drift across toggles.

## Gotchas

- **Window position isn't remembered across sessions.** `tauri-plugin-window-state` persists size/position per window
  label, but each viewer label is unique (timestamp-based), so there's no stable identifier to key on (the same file can
  be opened multiple times). Within a session, viewers cascade from the main window's top-left (+24px per opened viewer,
  wrapping at 8) via `lib/window-positioning.ts` so successive opens don't pile up.
- **Binary files shown with lossy UTF-8** (replacement chars for invalid bytes, no binary mode). The viewer is designed
  for text/log files; a hex/binary mode would need a completely different rendering pipeline. Lossy display is good
  enough for quick inspection.
- **Search uses byte offsets internally, converted to UTF-16 code units for JS.** Rust searches over raw bytes for
  speed; JS `String.substring()` uses UTF-16 code units. The backend does the conversion so the frontend can highlight
  matches correctly in JavaScript strings.
- **The `windowReady` flag gates `closeWindow()`.** If Escape is pressed before mount finishes, close is queued. WebKit
  crashes if you close a window before its content process has finished initializing. The flag is set after a post-mount
  `requestAnimationFrame`, ensuring at least one paint cycle has completed.
- **Menu integration requires two-way sync for word-wrap state.** Both "W" and the menu CheckMenuItem toggle word wrap.
  "W" calls `viewerSetWordWrap` to update the menu's checked state; the menu item emits `viewer-word-wrap-toggled` back
  to the frontend. The `fromMenu` parameter prevents an infinite toggle loop.
- **`needsFetch()` samples three points instead of iterating the entire visible range.** The visible range can be
  hundreds of lines; checking every line against the cache on every scroll event would be expensive. Sampling the first,
  middle, and last lines catches the common case (scrolling into uncached territory) with O(1) work.

## Binary-warning suppression

`binary-warning.ts` classifies every image / document / binary extension. The viewer page shows the banner only when
`!isMedia && warning.shouldWarn`, where `isMedia` comes from the authoritative backend `kind` (image/pdf). So a rendered
image / PDF never shows it, while formats the classifier promotes to neither (RAW like `.cr2`/`.nef`, `.avif`, `.ico`,
`.docx`, `.epub`, archives, etc.) still warn. Don't trim the image set here to "suppress" rendered formats: that also
silences the unrendered ones (RAW/AVIF/ICO), which then show raw bytes with no nudge.
