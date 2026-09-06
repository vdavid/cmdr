# Viewer

The file viewer opens files in their own Tauri window, with virtual scrolling and text search. Backend:
`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`; reusable FE primitives:
`apps/desktop/src/lib/file-viewer/CLAUDE.md`.

## Module map

`+page.svelte` (lifecycle, window, UI) wires the `createViewer*` composables (scroll, search, line-heights, text-width,
tail, media, copy, autoscroll), the selection / caret / segment helpers, and the `createViewerKeyboard` keydown router.
Media renders inline; toolbar, status bar, context menu, pickers, and dialogs are presentational siblings. Inventory and
media flow: `DETAILS.md` § "Module map".

## Must-knows

Each line is a break-if-ignored invariant; the named `DETAILS.md` section has the why.

- **Composables take callback-based deps (getters), never raw `$state`** (passing `$state` loses reactivity). Effects
  live on the page and delegate to `run*Effect()`. (§ Architecture)
- **Text-only line paths are data-gated on `media.isMedia`, not just hidden.** Media sessions have empty text fields:
  keep the early-returns in line effects, `openViewerSession`, and the keydown router, else empty-line code runs and can
  throw. (§ "Media rendering")
- **Media↔text two-way switch resets media state BEFORE reopening, and `reset()` PRESERVES `lastMediaKind`.** Both route
  through `reopenSession({ asText })`. (§ "Media rendering")
- **`cmdr-media://` URLs are built ONLY via `mediaUrl(token)` in `media-view.ts`**, and the `cmdr-media:` scheme is in
  the `img-src` + `object-src` CSP (`tauri.conf.json`). `viewer-media.spec.ts` guards both. (§ "Media rendering")
- **`user-select: none` on `.file-content` is deliberate**: the viewer owns its selection model; the native one loses
  its anchor on scroll-out. `.status-bar` opts back in with `user-select: text`. (§ Gotchas)
- **Point → caret is geometric and presses are counted off `pointerdown`**: ❌ never the browser caret API (silently
  wrong under `user-select: none` on some WebKits) or a click's `detail`. (§ "Pointer → caret", § "Click cycle")
- **A content pointer gesture claims DOM focus** (`takeFocus`); without it ⌘C copies the search query. (§ Gotchas)
- **Selection / IPC offsets are UTF-16 code units, not bytes or graphemes.** Caret math (`viewer-pointer.ts`) and
  anything crossing `viewer_read_range` must preserve this; the backend converts to UTF-8 and clamps lone surrogates. (§
  "Selection model")
- **Keyboard extension has two placement-sensitive entry points.** Unmodified Shift+Arrow / Home / End goes after the
  `searchInputFocused` guard and before `handleBareKey`; the ⌥/⌃/⌘ branch sits in `handleModifiedKey`, which runs
  regardless of focus and gates on `!searchInputFocused` itself. (§ "Keyboard motion model")
- **`closeWindow()` defers via `deferWindowClose()` (100 ms, NOT 0), `windowReady` via `setTimeout(0)`; never rAF.** A
  sync `close()` stalls other webviews' IPC, a `0`-delay close segfaults macOS WebKit mid-teardown, and rAF starves in
  unfocused E2E windows. Don't lower it. (§ Gotchas; `$lib/window-close-defer`)
- **Escape: the page's window keydown runs BEFORE `ViewerContextMenu`'s**, so it gates on `contextMenuPos !== null`
  before falling through to `closeWindow()`, else an open menu's Escape shuts the window. (§ Gotchas)
- **The height map's wrap width comes from row geometry, never a `.line-text` span** (`.line-text` shrink-wraps;
  measuring it once inflated the map ~7x). `heightMap.ready` gates every height-map path. (§ "Variable-height word
  wrap")
- **Tail mode is not persisted, and the viewer window has NO `store:default` capability** (it renders possibly-hostile
  content). Persisted viewer settings go through the typed restricted-window commands: extend that allowlist, never
  re-grant store access. (§ "Tail mode"; `lib/settings/DETAILS.md` § "Restricted-window mode")
- **Search error / invalid-query state is a typed `searchStatus` + sibling `searchError` string, never inspected as
  text.** In regex mode, line spans come from the backend's `searchMatches`, not a JS recompile. (§ "Search modes")
- **`scrollToMatch` centres from the rendered `mark.active` rect via two paths: gentle if the row is rendered, else
  rough-scroll + a converge loop.** ❌ Don't collapse them; an unconditional rough-scroll flings an on-screen match to
  its line top. (§ "Scroll-to-match")

Read `DETAILS.md` before any non-trivial work here: editing, planning, or advising.
