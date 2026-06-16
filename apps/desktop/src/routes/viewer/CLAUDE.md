# Viewer

The file viewer opens files in a separate Tauri window with virtual scrolling and text search.

Backend: [`file_viewer/CLAUDE.md`](../../../src-tauri/src/file_viewer/CLAUDE.md) (three strategies, session
orchestration, background search). Reusable FE primitives:
[`lib/file-viewer/CLAUDE.md`](../../lib/file-viewer/CLAUDE.md).

## Module map

`+page.svelte` is the top-level component (lifecycle, window management, UI); it wires `createViewer*` composables for
scroll, search, line-heights, text-width, tail, media, copy, and autoscroll, plus selection/caret/segment helpers and
the `createViewerKeyboard` keydown router. Media renders inline via `MediaImageView` / `MediaPdfView`; presentational
parts are `ViewerToolbar`, `ViewerStatusBar`, `ViewerContextMenu`, `ViewModePicker`, `EncodingPicker`,
`ViewerCopyDialogs`, and `ViewerReloadToast`. Full per-file inventory and the media flow: [DETAILS.md](DETAILS.md) §
"Module map". Locate symbols via `codegraph_search`, not this list.

## Must-knows

Each line is a break-if-ignored invariant; the named [DETAILS.md](DETAILS.md) section carries the why.

- **Composables take callback-based deps (getters), never raw `$state`** (passing `$state` loses reactivity). Effects
  live on the page but delegate to `run*Effect()` methods. (§ Architecture)
- **Text-only line paths are data-gated on `media.isMedia`, not just hidden.** Media sessions have empty text fields, so
  keep the early-returns in line effects / `openViewerSession` / the keydown router, else the empty-line code runs and
  can throw. (§ "Media rendering")
- **Media↔text two-way switch resets media state BEFORE reopening, and `reset()` PRESERVES `lastMediaKind`.** Both
  handlers route through `reopenSession({ asText })`. Don't reorder the reset-then-reopen or clear `lastMediaKind`. (§
  "Media rendering")
- **`cmdr-media://` URLs are built ONLY via `mediaUrl(token)` in `media-view.ts`**, and the `cmdr-media:` scheme is in
  the `img-src` + `object-src` CSP (`tauri.conf.json`). A src bypassing `mediaUrl`, or a CSP edit dropping the scheme,
  trips `viewer-media.spec.ts`. (§ "Media rendering")
- **`user-select: none` on `.file-content` is deliberate**: the viewer owns its selection model; native selection
  competes and loses its anchor on scroll-out. `.status-bar` opts back in with `user-select: text`. Has a webkit2gtk
  `caretRangeFromPoint` trap (pinned Docker image). (§ Gotchas)
- **Selection / IPC offsets are UTF-16 code units, not bytes or graphemes.** Caret math (`viewer-pointer.ts`) and
  anything crossing `viewer_read_range` must preserve this; the backend converts to UTF-8 and clamps lone surrogates. (§
  "Selection model")
- **`closeWindow()` and `windowReady` both defer via `setTimeout(0)`, never rAF.** A sync `close()` in a webview handler
  stalls other webviews' IPC on the GTK tick; rAF starves in unfocused E2E windows and times out the suite. (§ Gotchas;
  `docs/testing.md` § "rAF in unfocused windows")
- **Escape handling depends on listener order: the page's window keydown runs BEFORE `ViewerContextMenu`'s.** The page
  gates on `contextMenuPos !== null` before falling through to `closeWindow()`, else an open menu's Escape shuts the
  window. The menu's `stopImmediatePropagation()` is defense-in-depth. (§ Gotchas)
- **The height map's wrap width comes from row geometry, never a `.line-text` span** (`.line-text` shrink-wraps;
  measuring it once inflated the map ~7x). `heightMap.ready` gates every height-map path (uniform-height fallback
  otherwise). (§ "Variable-height word wrap", § Gotchas)
- **Tail mode is not persisted, and the viewer window has NO `store:default` capability** (it renders possibly-hostile
  content). Persisted viewer settings (`viewer.wordWrap`, `fileViewer.suppressBinaryWarning`) route through the typed
  restricted-window commands: extend that allowlist, never re-grant store access. (§ "Tail mode";
  `lib/settings/DETAILS.md` § "Restricted-window mode")
- **Search error / invalid-query state is a typed `searchStatus` + sibling `searchError` string, never inspected as
  text** (no-error-string-match rule). In regex mode, line spans come from the backend's `searchMatches`, not a JS
  recompile. (§ "Search modes")
- **`scrollToMatch` centres from the rendered `mark.active` rect, with two paths (gentle if the line row is rendered,
  else rough-scroll + a converge loop).** Don't collapse them into an unconditional rough-scroll: that flings an
  on-screen match to its line top on every Enter. (§ "Scroll-to-match")

Read [DETAILS.md](DETAILS.md) before any non-trivial work here: editing, planning, reorganizing, or advising.
