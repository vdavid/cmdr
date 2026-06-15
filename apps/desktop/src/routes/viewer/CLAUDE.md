# Viewer

The file viewer opens files in a separate Tauri window with virtual scrolling and text search.

Backend counterpart: [`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`](../../../src-tauri/src/file_viewer/CLAUDE.md)
for the three backend strategies (chunked, full-load, pretext), session orchestration, and background search. Reusable
FE primitives live at [`src/lib/file-viewer/CLAUDE.md`](../../lib/file-viewer/CLAUDE.md).

## Files

- **`+page.svelte`**: Top-level component: lifecycle, window management, UI
- **`viewer-scroll.svelte.ts`**: Virtual scroll composable: line cache, fetch debounce, scroll compression, effects
- **`viewer-search.svelte.ts`**: Search composable: start/poll/cancel/navigate, match highlighting, debounce, `useRegex`
  and `caseSensitive` toggles, regex-error projection
- **`viewer-line-heights.svelte.ts`**: Height map for accurate word-wrap scrolling via pretext (FullLoad files only)
- **`viewer-text-width.svelte.ts`**: `ResizeObserver`-driven tracker for the width available to line text (scroll
  container minus gutter and `.line` padding)
- **`viewer-indexing-poll.ts`**: Periodic `viewer_get_status` poll while the backend builds a line index
- **`viewer-keyboard.ts`**: Pure key helpers (`handleNavigationKey` / `handleToggleKey` / `handleSearchToggleKey` /
  `handleTailToggleKey`) plus `createViewerKeyboard`, the page's full keydown router (modifier shortcuts, Escape ladder,
  ⌘A, bare-key dispatch)
- **`selection.svelte.ts`**: Selection model: state + pure helpers (normalise, in-range, segment bounds, byte estimator)
- **`line-segments.ts`**: Pure shared segmenter: merges search matches + selection bounds into render spans
- **`viewer-pointer.ts`**: Pure caret-from-point math: `(x, y)` -> `LineOffset` with surrogate-safe sibling-offset sum
- **`viewer-pointer-drag.svelte.ts`**: `createViewerPointerDrag`: stateful pointer / drag / context-menu controller
  (drag `pointerId`, `contextMenuPos`, autoscroll wiring, double / triple-click word/line select)
- **`viewer-copy.ts`**: Pure three-band copy policy (silent / confirm / refuse) and threshold constants
- **`viewer-copy.svelte.ts`**: `createViewerCopy` (read/write composable: busy flag + per-call read_id + cancel
  plumbing + saveAs) and `createViewerCopyOrchestrator` (copy-flow orchestration: confirm/refuse dialog state, clipboard
  writes, save-as panel, toasts)
- **`viewer-autoscroll.ts`**: Pure speed curve for drag-past-edge autoscroll
- **`viewer-autoscroll.svelte.ts`**: Autoscroll RAF controller: start / stop / self-terminate
- **`viewer-word.ts`**: Pure word-boundary finder via `Intl.Segmenter` for double-click selection
- **`ViewerContextMenu.svelte`**: Minimal in-app right-click menu (Copy, Select all)
- **`ViewerToolbar.svelte`**: Presentational title-bar overlay: file name, view-mode + encoding pickers, tail toggle,
  reindexing indicator. Owns `data-tauri-drag-region`. Same controls in every mode: in media mode the encoding picker
  and tail toggle render **disabled** (not hidden) so the chrome doesn't reshuffle when switching media↔text; the
  encoding picker shows its "Encoding" placeholder there (no decoded bytes yet).
- **`ViewerStatusBar.svelte`**: Presentational bottom bar: line / byte counts, backend badge, word-wrap badge, shortcut
  hint. Keeps `user-select: text` (see must-knows).
- **`ViewerCopyDialogs.svelte`**: Presentational copy-confirm (10 to 100 MiB) and refuse (> 100 MiB) modals.
  `createViewerCopyOrchestrator` owns the copy-flow state and IPC handlers.
- **`EncodingPicker.svelte`**: `ui/Select` with Unicode / Western `group` headings. Reactive to backend
  `EncodingChoice[]`. Detected encoding gets a "(Detected)" suffix.
- **`ViewModePicker.svelte`**: `ui/Select` showing the detected kind (Image / PDF / Text) with a two-way switch. For
  media it offers "View as text" (sentinel `viewAsText` → `onViewAsText`); while a media file is read as text (it gets
  `lastMediaKind`) it offers the reverse "View as image" / "View as PDF" (sentinel `viewAsMedia` → `onViewAsMedia`); a
  genuine text file (no `lastMediaKind`) is a single disabled "Text" option.
- **`media-view.ts`**: pure helpers for the media branch: `mediaUrl(token)` (the ONE place the `cmdr-media://localhost/`
  origin lives), `isMediaKind`, `mediaKindLabel`, `formatMediaDimensions`, and the image zoom math (`clampZoom`,
  `nextClickZoom`). Unit-tested in `media-view.test.ts`.
- **`viewer-media.svelte.ts`**: `createViewerMedia` composable: owns the media state (`kind` / `mediaToken` /
  `mediaDimensions`), the `isMedia` / `mediaSrc` deriveds, the remembered natural kind (`lastMediaKind`), and the
  two-way switch triggers (`viewAsText` / `viewAsMedia`). Tested in `viewer-media.svelte.test.ts`.
- **`MediaImageView.svelte`**: inline `<img>` from `cmdr-media://`. Fit-by-default, click toggles 100%/fit,
  scroll/`+`/`-` zoom, drag pan, checkerboard behind transparency, spinner + friendly error states, keyboard-reachable.
- **`MediaPdfView.svelte`**: inline `<embed type="application/pdf">` from `cmdr-media://`; WKWebView supplies
  scroll/zoom/ page UI. Same spinner + error treatment.
- **`viewer-tail.svelte.ts`**: `createViewerTail()` composable: listens to `viewer:file-changed:<sid>` events and
  dispatches to reload toasts or a side effect.
- **`ViewerReloadToast.svelte`**: Component content for the persistent reload toast. Reads its session id from
  `setReloadToastContext()` (the toast system mounts without props).

## Must-knows

Each line is a break-if-ignored invariant. The matching DETAILS.md section carries the why.

- **Composables take callback-based deps (getters), never raw `$state`** — passing `$state` directly loses reactivity.
  Effects live on the page but delegate to `run*Effect()` methods. See [DETAILS.md](DETAILS.md) § Architecture.
- **Text-only line paths are data-gated on `media.isMedia`, not just hidden.** Media sessions have empty text fields, so
  don't drop the early-returns in line effects / `openViewerSession` / the keydown router, or the empty line code runs
  and can throw. See [DETAILS.md](DETAILS.md) § "Media rendering".
- **Media↔text two-way switch resets media state BEFORE reopening, and `reset()` PRESERVES `lastMediaKind`.** Both
  handlers route through `reopenSession({ asText })`, which closes the old session by its (different) id and re-attaches
  per-session listeners. Don't reorder the reset-then-reopen or clear `lastMediaKind`. See [DETAILS.md](DETAILS.md) §
  "Media rendering".
- **`cmdr-media://` URLs are built ONLY via `mediaUrl(token)` in `media-view.ts`**, and the `cmdr-media:` scheme is in
  `img-src` + `object-src` CSP (`tauri.conf.json`). A new media `<img>`/`<embed>` src bypassing `mediaUrl` or a CSP edit
  that drops the scheme trips `viewer-media.spec.ts`. See [DETAILS.md](DETAILS.md) § "Media rendering".
- **`user-select: none` on `.file-content` is deliberate** — the viewer owns its own selection model; native selection
  would render a broken competing one that loses its anchor on scroll-out. `.status-bar` opts back in with
  `user-select: text`. Has a webkit2gtk `caretRangeFromPoint` trap (pinned Docker image). See [DETAILS.md](DETAILS.md) §
  Gotchas.
- **Selection / IPC offsets are UTF-16 code units, not bytes or graphemes.** Caret math (`viewer-pointer.ts`) and
  anything crossing `viewer_read_range` must preserve this; the backend converts to UTF-8 and clamps lone surrogates.
  See [DETAILS.md](DETAILS.md) § "Selection model".
- **`closeWindow()` and `windowReady` both defer via `setTimeout(0)`, never rAF.** A synchronous `close()` inside a
  webview event handler stalls other webviews' IPC on the GTK tick; rAF starves in unfocused E2E windows (WKWebView
  throttle) and times out the viewer suite. Don't swap either to rAF or a sync close. See [DETAILS.md](DETAILS.md) §
  Gotchas and `docs/testing.md` § "rAF in unfocused windows".
- **Escape handling depends on listener order: the page's window keydown runs BEFORE `ViewerContextMenu`'s.** The page
  gates on `contextMenuPos !== null` before falling through to `closeWindow()`, or an open context menu's Escape shuts
  the whole window. The menu's `stopImmediatePropagation()` is defense-in-depth. See [DETAILS.md](DETAILS.md) § Gotchas.
- **The height map's wrap width comes from row geometry, never a `.line-text` span.** `.line-text` shrink-wraps, so
  measuring it on a short first line once inflated the map ~7x (unreachable file end). And `heightMap.ready` gates every
  height-map path (uniform-height fallback otherwise). See [DETAILS.md](DETAILS.md) § "Variable-height word wrap" and §
  Gotchas.
- **Tail mode is not persisted, and the viewer window has NO `store:default` capability** (it renders possibly-hostile
  content). Persisted viewer settings (`viewer.wordWrap`, `fileViewer.suppressBinaryWarning`) route through the typed
  restricted-window commands — extend that allowlist, never re-grant store access. See [DETAILS.md](DETAILS.md) § "Tail
  mode" and `lib/settings/DETAILS.md` § "Restricted-window mode".
- **Search error / invalid-query state is a typed `searchStatus` + sibling `searchError` string, never inspected as
  text** (no-error-string-match rule). In regex mode, line spans come from the backend's `searchMatches`, not a JS
  recompile. See [DETAILS.md](DETAILS.md) § "Search modes".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
</content>
