# Viewer details

Pull-tier docs for the viewer route (`routes/viewer/`): architecture, flows, and decision rationale. Must-know
invariants and gotchas live in [CLAUDE.md](CLAUDE.md).

The file viewer opens files in a separate Tauri window with virtual scrolling and text search. Backend counterpart:
[`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`](../../../src-tauri/src/file_viewer/CLAUDE.md) for the three backend
strategies (chunked, full-load, pretext), session orchestration, and background search. Reusable FE primitives live at
[`src/lib/file-viewer/CLAUDE.md`](../../lib/file-viewer/CLAUDE.md).

## Module map

Per-file inventory for the route. Locate symbols via `codegraph_search`; this is the orientation layer.

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

## Architecture

The page component creates two composables via `createViewerScroll` and `createViewerSearch`. Both use callback-based
deps (getters) so they can read reactive state from the page without receiving `$state` directly (which would lose
reactivity). The page owns session-level state (`sessionId`, `totalLines`, `backendType`, etc.) and wires the
composables together.

Effects live in the page component but delegate to `run*Effect()` methods on the composables, because `$effect()` only
works in `.svelte` or `.svelte.ts` files at the top level of a component or `createXxx` function scope.

### Media rendering (image / PDF)

`viewer_open` returns `kind` (`text` / `image` / `pdf`) + `mediaToken` / `mediaDimensions` (backend:
`src-tauri/src/file_viewer/`). The `createViewerMedia` composable (`viewer-media.svelte.ts`) owns this state; the page
branches on `media.kind`: text uses the line pipeline; `image` / `pdf` render `MediaImageView` / `MediaPdfView` from
`media.mediaSrc` (`cmdr-media://localhost/<token>`, built ONLY via `mediaUrl(token)` in `media-view.ts`, the single
source for the origin form). `openViewerSession` hands the result to `media.setFromOpenResult(result)`.

- **Text-only paths are data-gated, not just hidden.** Every page `$effect` driving the line machinery early-returns on
  `isMedia` (derived from `media.isMedia`), `openViewerSession` skips the line/index/tail/encoding setup for media, and
  the window keydown router only handles Escape in media mode (image keys live on the focused `MediaImageView` stage;
  the PDF embed owns its own). A media session has empty text fields, so don't undo these guards or the empty line code
  runs and can throw.
- **Two-way switch between rendered media and raw text.** A viewer window shows exactly one file for its life, so the
  file's natural media kind stays recoverable on the frontend: `media.setFromOpenResult` stamps `lastMediaKind` on any
  media open, and `reset()` PRESERVES it across the switch to text. "View as text" (`media.viewAsText()`) resets the
  media state up front (so a failed re-open can't leave a dangling image), then calls the page's `reopenAsText`. The
  reverse "View as image / PDF" (`media.viewAsMedia()`, a no-op unless `kind === 'text' && lastMediaKind !== null`)
  calls `reopenNatural`. Both page handlers share `reopenSession({ asText })`: it opens a fresh session via
  `viewer_open_as_text` (text) or `viewer_open` (re-classifies → media), swaps to it, and closes the old session
  EXPLICITLY (different id). The page tears down per-session listeners first because `openViewerSession` re-attaches
  them. No backend change: `viewer_open` re-classification is what re-derives the media kind.
- CSP: the `cmdr-media:` token is in `img-src` + `object-src` (`tauri.conf.json`); `viewer-media.spec.ts` locks "no
  `cmdr-media`/`img-src`/`object-src` violation". WKWebView applies EXIF orientation by default (phone photos upright).

### Variable-height word wrap (progressive enhancement)

`viewer-line-heights.svelte.ts` uses `@chenglou/pretext` to compute per-line wrapped heights for FullLoad files (<1MB).
It runs `prepare()` asynchronously via `requestIdleCallback` after first render, then builds a prefix-sum array for O(1)
`getLineTop(n)` and O(log n) `getLineAtPosition(y)`. While preparation runs (or for ByteSeek/LineIndex files), the
viewer falls back to the existing averaged-height approach with zero regression.

**Extending exact wrap to ByteSeek/LineIndex (multi-MB) is unsolved, and the obvious fix is a trap.** Counting chars per
line in Rust and dividing by column width (`ceil(chars * charWidth / availWidth)`) does NOT give the wrapped row count:
the viewer wraps at word boundaries (`overflow-wrap: break-word` on `.line-text`), so the ragged right edge pushes
content into rows the division never predicts — shipping it would regress the FullLoad path pretext gets right. It's a
pick-two-of-three (exact word wrap / a compact per-line scalar / instant local resize reflow); word-boundary wrapping
can't be reconstructed from a per-line count, so you can't have all three. Computing the wrap in Rust (exact + compact,
reflow needs an IPC recompute) is the most promising path. This breadcrumb replaces a deleted speculative plan.

**Integration flow:** The scroll composable creates the height map and exposes `runHeightMapInitEffect` (triggers
preparation when word wrap + lines + textWidth are available) and `runHeightMapReflowEffect` (re-layouts on width change
with synchronous scroll compensation). The page component wires these as `$effect`s and tracks `textWidth` via a
`ResizeObserver` on `.file-content`. The search composable uses `getLineTop(n)` instead of `n * scrollLineHeight` for
scroll-to-match positioning.

**Key invariant:** `heightMap.ready` gates all height-map paths. When false, every calculation falls through to the
existing uniform-height code. The `scrollScale` (for MAX_SCROLL_HEIGHT compression) multiplies height map values at the
scroll layer (the height map stores unscaled positions).

## Selection model

The viewer owns its own selection model (`selection.svelte.ts`) instead of relying on the browser's `Selection` API. The
browser API can't survive virtualisation: as soon as the anchor or focus scrolls out of the visible buffer, its DOM node
is recycled and the selection collapses. The custom model tracks two `LineOffset` endpoints (`{ line, offset }`) in
logical coordinates, independent of which lines happen to be rendered.

- **Range semantics**: half-open `[start, end)`. The start line is included from `start.offset` to its end, intermediate
  lines are included in full, the end line is included from offset 0 up to but not including `end.offset`.
- **Offset units**: UTF-16 code units (matches `String.length` and the search column units the search engine already
  emits, so the whole frontend speaks one unit). The backend converts to UTF-8 bytes at the IPC boundary, clamping
  offsets that land between the high and low surrogate of an astral codepoint.
- **Render**: the page calls `getLineSegmentBounds(selection, lineNumber, lineLength)` and passes the bounds to
  `search.getHighlightedSegments(...)`. The shared `segmentLine()` function (in `line-segments.ts`) merges search-match
  spans with selection bounds and emits non-overlapping `LineSegment`s tagged `highlight` / `active` / `selected`. The
  template renders each segment as a `<mark>` (search) or `<span class="selected">` or plain text.
- **Visual collision**: when a search hit and the selection overlap on the same span, search wins on the background
  (`var(--color-highlight)`) and selection wins on the foreground (`var(--color-selection-fg)`, gold). Matches the
  "selected = gold" language from the file list (design-system.md § File list).

## Title-bar overlay toolbar

The viewer window opens with `titleBarStyle: 'overlay'` and `trafficLightPosition: { x: 9, y: 17 }` (see
`lib/file-viewer/open-viewer.ts` — kept in sync with the main window's `tauri.conf.json`). The toolbar at the top of
`+page.svelte` reserves 80 px of left padding for the macOS traffic lights and lets the empty space remain draggable via
`data-tauri-drag-region`. The pickers and indexing status sit on the right; the file name occupies the flexible middle.

The encoding picker fetches its options once via `commands.viewerGetEncodingOptions(sessionId)` on open; the list is
backend-authoritative (no FE-side encoding catalog). Switching encoding calls `commands.viewerSetEncoding`, clears the
line cache, and triggers `scroll.fetchVisibleNow()` so the user sees re-decoded lines immediately. If the swap requires
a rebuild, `indexingPoll.start()` runs the same status-poll the initial ByteSeek → LineIndex upgrade uses; the toolbar
shows "Reindexing…" while `isIndexing` is true.

## Tail mode

`F` (unmodified) toggles tail mode. A toggle button in the title-bar overlay mirrors the state with `role="switch"` and
`aria-checked`. When on, the backend extends its line index in response to filesystem `Grew` events and the viewport
auto-refetches. When off, every external change surfaces a persistent toast ("File changed on disk. Reload?") with an
inline Reload button that calls `viewer_reload(sessionId)`.

A `Shrunk` / `Replaced` event always shows the toast ("File replaced on disk. Reload to see the new content."),
regardless of tail mode: the backend has already reopened against the new file, and the toast tells the user why their
cursor jumped.

Toast deduplication: ids include the kind (`viewer-file-changed-<sid>-grew`, `…-rotated`). Rapid same-kind events
coalesce into one toast. A rotated event explicitly dismisses any open grew toast: the older "reload to catch up"
message is no longer accurate.

Tail mode is **not persisted** across sessions: it defaults off on every viewer open and the user re-enables it per
session. The viewer window has no `store:default` capability by security design (it renders arbitrary, possibly-hostile
file content), so it can't write a per-path store. Viewer settings that DO persist (`viewer.wordWrap`,
`fileViewer.suppressBinaryWarning`) route through the typed restricted-window commands (`get_restricted_window_settings`
/ `persist_restricted_window_setting`) — never re-grant store access to the window; extend that allowlist instead. See
`src-tauri/capabilities/CLAUDE.md` § viewer and `lib/settings/DETAILS.md` § "Restricted-window mode".

## Search modes

`createViewerSearch` owns two mode flags besides the query text: `useRegex` (default off) and `caseSensitive` (default
on). Both are exposed as toggle buttons in the search bar (`Aa` and `.*`) with `aria-pressed` reflecting the state, plus
keyboard shortcuts `⌘⌥C` and `⌘⌥R` handled by `viewer-keyboard.ts::handleSearchToggleKey`. The chord is gated on
meta+alt (or ctrl+alt) so it can't collide with the input's native `⌘A` / `⌘C`.

Toggling either flag while a query is active cancels the in-flight search and re-runs it with the new mode. The
backend's `SearchStatus::InvalidQuery { message }` is projected to a flat `searchStatus === 'invalidQuery'` plus a
sibling `searchError: string | null`, kept as plain text and rendered in a `role="alert"` span. The composable never
inspects the message text (per the no-error-string-match rule).

In regex mode, `getLineMatches` reads spans straight from the backend's authoritative `searchMatches` array instead of
recomputing them client-side; the regex compile already happened in the backend, and re-running it per line in JS would
either duplicate work or risk a different result.

## Scroll-to-match

`scrollToMatch` (on `findNext` / `findPrev`, not the initial auto-select) centres the active match on both axes from its
real rendered `mark.active` rect. The per-axis math is `recenterOffset` (pure, in `viewer-search-scroll.ts`,
unit-tested): it returns the centring target, or `null` when the match is already within a 10% edge margin unless
`forceCenter` overrides. Working from the rendered rect, not a `column * charWidth` estimate, keeps it exact for
word-wrapped rows and wide-CJK / astral glyphs (the arithmetic approach drifted on both). Horizontal centring is skipped
in word-wrap mode; the `mark.active` lookup is scoped to the target line's row so a stale `.active` elsewhere isn't
picked up after a cross-line jump.

Two paths, by whether the match's line row is in the DOM (a wrapped line is one tall element, so any on-screen part
means the whole line is rendered):

- **Gentle (line rendered):** after `tick()`, one `recenterOffset` pass with `forceCenter` off, so an already-visible
  match isn't touched and stepping between on-screen hits doesn't jump.
- **Ensure (line off-screen):** rough-scroll toward the line so it renders, then a `requestAnimationFrame` loop
  force-centres and re-reads the rect each frame until stable (a tall wrapped line's layout is still settling on the
  first frame, so a single post-scroll read mislands).

**Guardrail:** don't collapse the two into an unconditional rough-scroll: rough-scrolling a match whose line is already
on screen flings the view to the line top on every Enter.

## Gotchas

- `$state(false)` in `.svelte.ts` triggers `@typescript-eslint/no-unnecessary-condition` because the linter doesn't know
  the value is mutated via Svelte reactivity. Use an inline eslint-disable comment with a reason.
- **`user-select: none` on `.file-content` is deliberate.** The viewer owns its own selection model (above); the
  browser's native selection would render a competing-and-broken one on top of ours that loses its anchor as soon as the
  line scrolls out. `.status-bar` opts back in with `user-select: text` so users can still copy the file name or line
  count. `.line-number` keeps the global default (`none`), it's aria-hidden chrome.
  - Trap: webkit2gtk 2.50.4 (Ubuntu 24.04) has a bug where `caretRangeFromPoint` returns `offset: 0` for every x-coord
    inside `user-select: none` text, which breaks the pointer → caret path in `viewer-pointer.ts:resolveCaret`.
    webkit2gtk 2.52.3 (Ubuntu 25.10+) doesn't have it. The Docker E2E image is pinned to `ubuntu:26.04` to avoid this;
    see `apps/desktop/test/e2e-linux/CLAUDE.md` § Gotchas. If you ever need to support a webview version that still has
    the bug (e.g. an older Linux distro target), replace this code path with a `Range.getClientRects()`-based binary
    search that doesn't depend on the browser caret API.
- **Selection offsets are UTF-16 code units, not bytes or grapheme clusters.** When you add features that compute
  offsets from a click position (caret math in `viewer-pointer.ts`) or accept them across the IPC boundary
  (`viewer_read_range`), preserve the UTF-16 convention. The backend handles the conversion to UTF-8 bytes, clamping
  lone surrogates to the nearest codepoint boundary.
- **Drag autoscroll uses `setPointerCapture` + window `blur` fallback** because the Tauri webview can lose `pointerup`
  events to other macOS windows. Without capture, dragging past the webview's edge leaves the RAF loop running forever
  with no way to stop. Capture is wrapped in try/catch because some webviews refuse it on non-focusable targets; the
  blur listener is the safety net for the "no pointer event but focus left" case.
- **`viewer_read_range` cancel id is FE-allocated, not BE-allocated**. The frontend's `createViewerCopy()` composable
  uses a monotonic per-session counter. This avoids an extra round-trip (call to "start read", await `read_id`, then
  another call to "wait for read"); the FE just sends the id with the read request, and the backend keys the cancel flag
  off that id. Uniqueness within the session is the only invariant.
- **`ViewerContextMenu` Escape stops propagation AND the page checks `contextMenuPos`.** The page's
  `<svelte:window on:keydown>` listener is registered first (the menu mounts later), so the page's handler runs before
  the menu's. If the page didn't gate on `contextMenuPos !== null` first, Escape would fall through to `closeWindow()`
  and shut the whole viewer window. The menu's `stopImmediatePropagation()` is defense-in-depth for any future
  listener-order change. See `tryConsumeEscapeForCopy` in `viewer-keyboard.ts` (`createViewerKeyboard`) and `handleKey`
  in `ViewerContextMenu.svelte`.
- **AT announcement caps line iteration.** `describeSelectionForAt` in `selection.svelte.ts` walks per-line lengths to
  build the screen-reader announcement. ⌘A in ByteSeek-no-index mode sets `focus.line = Number.MAX_SAFE_INTEGER` (the
  sentinel that maps to `RangeEnd::Eof` at the IPC boundary), so an uncapped loop would iterate 9e15 times. The
  `MAX_ANNOUNCE_LINES = 10_000` cap short-circuits to "Selected from line N to the end of the file" without touching the
  line-length lookup at all.
- **Drag autoscroll honours `prefers-reduced-motion`.** Under reduced motion, `createViewerAutoscroll().start()` does a
  single synchronous snap step and exits without queuing a RAF. The page's `pointermove` calls `start()` on every move,
  so the user still progresses through the file in discrete jumps. Override via the `prefersReducedMotion` dep for
  tests.
- `getLineHeight()` (returns `18px × effective scale`) and the CSS rule
  `.line { height: calc(18px * var(--font-scale)) }` in `+page.svelte` must stay paired. Both read the same scale: the
  JS function for virtualization math, the CSS rule for layout. If you change the 18 base, change both.
- `runHeightMapInitEffect` guards with `if (heightMap.ready) return` to avoid re-preparing when only `textWidth`
  changes. Width-only changes are handled by `runHeightMapReflowEffect` via `reflow()` (instant) instead of re-running
  the async `prepareLines` pipeline. Without this guard, both effects would race on width changes.
- **The height map's wrap width comes from the row geometry, never from a `.line-text` span.**
  `viewer-text-width.svelte.ts` computes it as the scroll container's `clientWidth` minus the `.line` padding and the
  gutter (`.line-number` width + margin). `.line-text` is a flex item with no `flex-grow`, so it shrink-wraps to its own
  content: measuring it on a file whose first line is short ("# Cmdr", ~44px) once fed a 44px wrap width to pretext,
  inflating the height map ~7x (blank space below ~line 60, end of the file unreachable). The `.line` row is no better:
  in no-wrap mode the `.lines-container` is `max-content`, so the row is as wide as the widest line. Pinned by
  `viewer-text-width.svelte.test.ts` and `viewer-wordwrap-scroll.spec.ts` (E2E).
- **Pretext reports height 0 for empty lines; `buildPrefixSum` clamps each line to `getLineHeight()`.** The DOM renders
  every `.line` row at least one line tall (the gutter number keeps the row open), so without the clamp the height map
  under-counts by one row per empty line and the scroll mapping drifts on files with many blank lines.
- `closeWindow()`'s `setTimeout(() => …, 0)` before `currentWindow.close()` is load-bearing — not decoration. Calling
  `close()` synchronously from inside a webview event handler runs webkit2gtk's destruction on the same GTK main-loop
  tick, stalling other webviews' IPC for an undefined duration. The settings page (`routes/settings/+page.svelte`'s
  Escape handler) mirrors this exact pattern for the same reason; see the Gotcha note in `lib/settings/CLAUDE.md` and
  commit `46481b29` for the original post-mortem. `setTimeout(0)` achieves a "next event-loop tick" guarantee without
  the rAF throttling that WKWebView applies to unfocused windows. **The same trap applies to `windowReady`** (the
  `data-window-ready` attribute every viewer E2E spec waits on): it's set via `setTimeout(0)` after session open, NOT
  rAF — an rAF there starved in unfocused E2E windows and timed out the whole viewer suite whenever a human was using
  the machine. Canonical rule + recurrence history: `docs/testing.md` § "rAF in unfocused windows". </content> </invoke>
