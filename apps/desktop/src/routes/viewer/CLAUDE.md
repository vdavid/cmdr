# Viewer

The file viewer opens files in their own Tauri window, with virtual scrolling and text search. Backend:
`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`; reusable FE primitives:
`apps/desktop/src/lib/file-viewer/CLAUDE.md`.

## Module map

`+page.svelte` (lifecycle, window, UI) wires the `createViewer*` composables, the selection / caret / granularity /
motion helpers, and the `createViewerKeyboard` keydown router. Toolbar, status bar, context menu, pickers, and dialogs
are presentational siblings. Inventory: `DETAILS.md` § "Module map".

## Must-knows

Each is a break-if-ignored invariant; the named `DETAILS.md` section has the why.

- **Composables take callback-based deps (getters), never raw `$state`** (passing `$state` loses reactivity). Effects
  live on the page and delegate to `run*Effect()`. (§ Architecture)
- **Media sessions need two guards.** Text-only line paths are data-gated on `media.isMedia` (their text fields are
  empty, so empty-line code otherwise runs and can throw), and the media↔text switch resets media state BEFORE
  reopening, with `reset()` PRESERVING `lastMediaKind`. (§ "Media rendering")
- **`cmdr-media://` URLs come ONLY from `mediaUrl(token)`**, and the scheme must stay in `tauri.conf.json`'s `img-src` +
  `object-src` CSP. `viewer-media.spec.ts` guards both. (§ "Media rendering")
- **`user-select: none` on `.file-content` is deliberate**: the native selection loses its anchor on scroll-out. (§
  Gotchas)
- **Point → caret is geometric and presses are counted off `pointerdown`**: ❌ never the browser caret API (silently
  wrong under `user-select: none`) or a click's `detail`. (§ "Pointer → caret", § "Click cycle")
- **A content pointer gesture claims DOM focus** (`takeFocus`); without it ⌘C copies the search query. (§ Gotchas)
- **Selection / IPC offsets are UTF-16 code units, not bytes or graphemes**, in caret math and across
  `viewer_read_range` alike. (§ "Selection model")
- **A gesture carries a granularity; both selection endpoints are edges of ranges snapped to it**, re-derived from the
  pressed range on every move (so a twitch can't collapse a double-press). Shift-click reads `gestureGranularity`, ❌
  never `dragGranularity`, which `endDrag` already reset. (§ "Selection granularity")
- **Keyboard extension has two placement-sensitive entry points.** Unmodified Shift+Arrow / Home / End goes after the
  `searchInputFocused` guard and before `handleBareKey`; the ⌥/⌃/⌘ branch sits in `handleModifiedKey`, which runs
  regardless of focus and gates on `!searchInputFocused` itself. (§ "Keyboard motion model")
- **"Caret" is a text POSITION, "text cursor" the optional rendered bar** — `caretRectFor` stays caret vocabulary though
  it measures that bar. Mount the cursor in `.scroll-spacer`, ❌ never in `.lines-container`, whose child count derives
  the wrapped-line height. (§ "Text cursor")
- **`closeWindow()` defers via `deferWindowClose()` (100 ms, NOT 0), `windowReady` via `setTimeout(0)`; never rAF.**
  Each dodges a different failure: stalled IPC in other webviews, a macOS WebKit segfault mid-teardown, starved rAF in
  unfocused E2E windows. (§ Gotchas; `$lib/window-close-defer`)
- **Escape: the page's window keydown runs BEFORE `ViewerContextMenu`'s**, so it gates on `contextMenuPos !== null`
  first, else an open menu's Escape shuts the window. (§ Gotchas)
- **The height map's wrap width comes from row geometry, never a `.line-text` span** (it shrink-wraps; measuring it
  inflated the map ~7x); `heightMap.ready` gates every height-map path. (§ "Variable-height word wrap")
- **Tail mode isn't persisted, and the viewer window has NO `store:default` capability.** Persisted viewer settings go
  through the typed restricted-window commands: extend that allowlist, never re-grant store access. (§ "Tail mode";
  `lib/settings/DETAILS.md` § "Restricted-window mode")
- **Search error state is a typed `searchStatus` + `searchError` string, ❌ never inspected as text**; regex line spans
  come from the backend's `searchMatches`, not a JS recompile. (§ "Search modes")
- **`scrollToMatch` has two paths: gentle when the row is rendered, rough-scroll + a converge loop when it isn't.** ❌
  Don't collapse them; an unconditional rough-scroll flings an on-screen match to its line top. (§ "Scroll-to-match")

Read `DETAILS.md` before any non-trivial work here: editing, planning, or advising.
