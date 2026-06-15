# Viewer

The file viewer opens files in a separate Tauri window with virtual scrolling and text search.

Backend: [`file_viewer/CLAUDE.md`](../../../src-tauri/src/file_viewer/CLAUDE.md) (three strategies, session
orchestration, background search). Reusable FE primitives:
[`lib/file-viewer/CLAUDE.md`](../../lib/file-viewer/CLAUDE.md).

## Files

Module map; per-file depth and the media flow live in [DETAILS.md](DETAILS.md).

- **`+page.svelte`**: top-level component (lifecycle, window management, UI).
- Composables: **`viewer-scroll`** (virtual scroll), **`viewer-search`** (start/poll/cancel/navigate, regex projection),
  **`viewer-line-heights`** (word-wrap height map via pretext, FullLoad only), **`viewer-text-width`** (`ResizeObserver`
  width tracker), **`viewer-tail`** (`viewer:file-changed:<sid>` → reload toasts).
- **`viewer-indexing-poll.ts`**: `viewer_get_status` poll during line-index build.
- **`viewer-keyboard.ts`**: pure key helpers + `createViewerKeyboard`, the keydown router (modifiers, Escape ladder, ⌘A,
  bare-key dispatch).
- Selection: **`selection.svelte.ts`** (model), **`line-segments.ts`** (pure segmenter), **`viewer-pointer.ts`** (pure
  caret math, surrogate-safe), **`viewer-pointer-drag.svelte.ts`** (pointer/drag/context-menu controller),
  **`viewer-word.ts`** (word-boundary via `Intl.Segmenter`).
- **`viewer-search-scroll.ts`**: pure per-axis scroll-to-match centring (`recenterOffset`, rect-based).
- Copy: **`viewer-copy.ts`** (pure silent/confirm/refuse policy + thresholds), **`viewer-copy.svelte.ts`**
  (`createViewerCopy` + `createViewerCopyOrchestrator`). Autoscroll: **`viewer-autoscroll.ts`** (curve) +
  **`.svelte.ts`** (RAF controller).
- Media: **`media-view.ts`** (pure helpers incl. `mediaUrl(token)`, the ONE `cmdr-media://localhost/` origin, + zoom
  math), **`viewer-media.svelte.ts`** (`createViewerMedia`: state, `isMedia`/`mediaSrc`, `lastMediaKind`, switch
  triggers), **`MediaImageView` / `MediaPdfView`** (inline `<img>` / `<embed>`).
- Presentational: **`ViewerContextMenu`**, **`ViewerToolbar`** (title-bar overlay, owns `data-tauri-drag-region`,
  disabled-not-hidden in media), **`ViewerStatusBar`** (keeps `user-select: text`), **`ViewerCopyDialogs`**,
  **`EncodingPicker`**, **`ViewModePicker`** (two-way media↔text switch), **`ViewerReloadToast`** (session id via
  `setReloadToastContext()`).

## Must-knows

Each line is a break-if-ignored invariant. The matching DETAILS.md section carries the why.

- **Composables take callback-based deps (getters), never raw `$state`** (passing `$state` directly loses reactivity).
  Effects live on the page but delegate to `run*Effect()` methods. See [DETAILS.md](DETAILS.md) § Architecture.
- **Text-only line paths are data-gated on `media.isMedia`, not just hidden.** Media sessions have empty text fields, so
  keep the early-returns in line effects / `openViewerSession` / the keydown router, or the empty-line code runs and can
  throw. See [DETAILS.md](DETAILS.md) § "Media rendering".
- **Media↔text two-way switch resets media state BEFORE reopening, and `reset()` PRESERVES `lastMediaKind`.** Both
  handlers route through `reopenSession({ asText })`. Don't reorder the reset-then-reopen or clear `lastMediaKind`. See
  [DETAILS.md](DETAILS.md) § "Media rendering".
- **`cmdr-media://` URLs are built ONLY via `mediaUrl(token)` in `media-view.ts`**, and the `cmdr-media:` scheme is in
  the `img-src` + `object-src` CSP (`tauri.conf.json`). A media src bypassing `mediaUrl`, or a CSP edit dropping the
  scheme, trips `viewer-media.spec.ts`. See [DETAILS.md](DETAILS.md) § "Media rendering".
- **`user-select: none` on `.file-content` is deliberate**: the viewer owns its own selection model; native selection
  competes and loses its anchor on scroll-out. `.status-bar` opts back in with `user-select: text`. Has a webkit2gtk
  `caretRangeFromPoint` trap (pinned Docker image). See [DETAILS.md](DETAILS.md) § Gotchas.
- **Selection / IPC offsets are UTF-16 code units, not bytes or graphemes.** Caret math (`viewer-pointer.ts`) and
  anything crossing `viewer_read_range` must preserve this; the backend converts to UTF-8 and clamps lone surrogates.
  See [DETAILS.md](DETAILS.md) § "Selection model".
- **`closeWindow()` and `windowReady` both defer via `setTimeout(0)`, never rAF.** A sync `close()` in a webview handler
  stalls other webviews' IPC on the GTK tick; rAF starves in unfocused E2E windows and times out the suite. See
  [DETAILS.md](DETAILS.md) § Gotchas and `docs/testing.md` § "rAF in unfocused windows".
- **Escape handling depends on listener order: the page's window keydown runs BEFORE `ViewerContextMenu`'s.** The page
  gates on `contextMenuPos !== null` before falling through to `closeWindow()`, else an open menu's Escape shuts the
  whole window. The menu's `stopImmediatePropagation()` is defense-in-depth. See [DETAILS.md](DETAILS.md) § Gotchas.
- **The height map's wrap width comes from row geometry, never a `.line-text` span** (`.line-text` shrink-wraps;
  measuring it once inflated the map ~7x). `heightMap.ready` gates every height-map path (uniform-height fallback
  otherwise). See [DETAILS.md](DETAILS.md) § "Variable-height word wrap" and § Gotchas.
- **Tail mode is not persisted, and the viewer window has NO `store:default` capability** (it renders possibly-hostile
  content). Persisted viewer settings (`viewer.wordWrap`, `fileViewer.suppressBinaryWarning`) route through the typed
  restricted-window commands: extend that allowlist, never re-grant store access. See [DETAILS.md](DETAILS.md) § "Tail
  mode" and `lib/settings/DETAILS.md` § "Restricted-window mode".
- **Search error / invalid-query state is a typed `searchStatus` + sibling `searchError` string, never inspected as
  text** (no-error-string-match rule). In regex mode, line spans come from the backend's `searchMatches`, not a JS
  recompile. See [DETAILS.md](DETAILS.md) § "Search modes".
- **`scrollToMatch` centres from the rendered `mark.active` rect, with two paths (gentle if the line row is rendered,
  else rough-scroll + a converge loop).** Don't collapse them into an unconditional rough-scroll: that flings an
  on-screen match to its line top on every Enter. See [DETAILS.md](DETAILS.md) § "Scroll-to-match".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
