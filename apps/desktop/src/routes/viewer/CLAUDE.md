# Viewer

The file viewer opens files in their own Tauri window, with virtual scrolling and text search. Backend:
`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`; FE primitives: `apps/desktop/src/lib/file-viewer/CLAUDE.md`.

## Module map

`+page.svelte` (lifecycle, window, UI) wires the `createViewer*` composables, the selection and caret helpers, and the
`createViewerKeyboard` router; `ViewerRow.svelte` draws one row and the components beside it are presentational.
Inventory: `DETAILS.md` § "Module map".

## Must-knows

Each is break-if-ignored; the named `DETAILS.md` section has the why.

- **Every coordinate here is a ROW**: a long line is several (`src-tauri/src/file_viewer/CLAUDE.md`). `totalLines` is
  the status bar's alone; the gutter number and the continuation marker ride on the row. ❌ Infer neither. The AT
  announcement names the gutter's LINE too, ❌ never a row. (§ "Rows, not lines")
- **Composables take getter deps, ❌ never raw `$state`** (which loses reactivity); effects live on the page and
  delegate to `run*Effect()`. (§ Architecture)
- **Text-only paths gate on `viewMode === 'text'`, ❌ never `media.isMedia`**: Binary/Hex (byte offsets, not rows) and
  media leave text fields empty, which throws. The switch resets media state BEFORE reopening; `reset()` PRESERVES
  `lastMediaKind`. (§ "Media rendering")
- **`cmdr-media://` URLs come ONLY from `mediaUrl(token)`**, and the scheme must stay in `tauri.conf.json`'s `img-src` +
  `object-src` CSP. `viewer-media.spec.ts` guards both. (§ "Media rendering")
- **`user-select: none` on `.file-content` is deliberate** (native selection loses its anchor on scroll-out), so Edit >
  Copy / Select all arrive as events (`viewer-menu-actions.ts`). (§ Gotchas)
- **Point → caret is geometric and presses count off `pointerdown`**: ❌ never the browser caret API (silently wrong
  under `user-select: none`) or a click's `detail`. (§ "Pointer → caret", § "Click cycle")
- **A content pointer gesture claims DOM focus** (`takeFocus`); without it ⌘C copies the search query. (§ Gotchas)
- **Selection / IPC offsets are UTF-16 code units, not bytes or graphemes**, in caret math and across
  `viewer_read_range` alike. (§ "Selection model")
- **A gesture carries a granularity and both endpoints snap to it**, re-derived from the pressed range on every move.
  Shift-click reads `gestureGranularity`, ❌ never `dragGranularity`, which `endDrag` already reset. (§ "Selection
  granularity")
- **Keyboard extension has two placement-sensitive entry points**: unmodified Shift+Arrow / Home / End after the
  `searchInputFocused` guard and before `handleBareKey`; the ⌥/⌃/⌘ branch in `handleModifiedKey`, which runs regardless
  of focus and gates on `!searchInputFocused` itself. (§ "Keyboard motion model")
- **"Caret" is a text POSITION, "text cursor" the optional rendered bar.** Mount the cursor in `.scroll-spacer`, ❌
  never in `.lines-container`, whose child count derives the wrapped-line height. (§ "Text cursor")
- **`closeWindow()` goes through `closeSelfWindow()` (backend hide + 100 ms), and `canClose` / `windowReady` flip via
  `setTimeout(0)`, ❌ never rAF.** Each dodges a different hang or crash. (§ Gotchas; `$lib/child-window-close`)
- **Escape: the page's window keydown runs BEFORE `ViewerContextMenu`'s**, so it gates on `contextMenuPos !== null`
  first, else an open menu's Escape shuts the window. (§ Gotchas)
- **The render window, its prefetch, and eviction are sized in pixels or distance, ❌ never in rows** (a wrapped row is
  hundreds of pixels tall); eviction skips `fullLoad`, whose height map needs every row. A short answer is normal: walk
  by `chunk.end` + `endByteOffset`, cache at `chunk.firstRowNumber`. The walk is `viewer-row-fetch.svelte.ts`: getters
  in, ❌ no geometry. (§ "Virtual scrolling")
- **The copy flow may not guess**: ⌘A with an uncached last row takes the `EOF_ROW` path (❌ never a 0 length),
  `rowMetrics` decides each row's delimiter, a from-the-top selection subtracts its leftover, and the toast measures the
  text it wrote. (§ "Selection model")
- **The height map's wrap width comes from row geometry, never a `.line-text` span** (it shrink-wraps; measuring it
  inflated the map ~7x); `heightMap.ready` gates every height-map path. (§ "Variable-height word wrap")
- **Tail mode isn't persisted, and the viewer window has NO `store:default` capability**: persisted settings go through
  the typed restricted-window commands, ❌ never a re-granted store. (§ "Tail mode"; `lib/settings/DETAILS.md` §
  "Restricted-window mode")
- **Search error state is a typed `searchStatus` + `searchError` string, ❌ never inspected as text**; regex line spans
  come from the backend's `searchMatches`, ❌ not a JS recompile. (§ "Search modes")
- **`scrollToMatch` has two paths: gentle when the row is rendered, rough-scroll + a converge loop when it isn't.** ❌
  Don't collapse them; an unconditional rough-scroll flings an on-screen match to its line top. (§ "Scroll-to-match")

Read `DETAILS.md` before any non-trivial work here: editing, planning, or advising.
