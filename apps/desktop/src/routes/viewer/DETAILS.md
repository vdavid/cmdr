# Viewer details

Pull-tier docs for the viewer route (`routes/viewer/`): architecture, flows, and decision rationale. Must-know
invariants and gotchas live in `CLAUDE.md`.

The file viewer opens files in a separate Tauri window with virtual scrolling and text search. Backend counterpart:
`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md` for the three backend strategies (`FullLoad`, `ByteSeek`,
`LineIndex`), session orchestration, and background search. Reusable FE primitives live at
`apps/desktop/src/lib/file-viewer/CLAUDE.md`.

## Module map

Per-file inventory for the route. Locate symbols via `codegraph_search`; this is the orientation layer.

- **`+page.svelte`**: top-level component (lifecycle, window management, UI).
- Composables: **`viewer-scroll`** (virtual scroll), **`viewer-search`** (start/poll/cancel/navigate, regex projection),
  **`viewer-line-heights`** (word-wrap height map via DOM measurement, FullLoad only), **`viewer-text-width`**
  (`ResizeObserver` width tracker), **`viewer-tail`** (`viewer:file-changed:<sid>` → reload toasts).
- **`viewer-indexing-poll.ts`**: `viewer_get_status` poll during line-index build.
- **`viewer-keyboard.ts`**: pure key helpers + `createViewerKeyboard`, the keydown router (modifiers, Escape ladder, ⌘A,
  bare-key dispatch, and the twelve selection-extension chords).
- Selection: **`selection.svelte.ts`** (model), **`line-segments.ts`** (pure segmenter), **`viewer-caret-geometry.ts`**
  (pure point → offset search, surrogate-safe), **`viewer-pointer.ts`** (its DOM adapter and the ONE place line text is
  measured: row hit-test, character rects, `caretRectFor`, `measureColumnWidth`), **`viewer-pointer-drag.svelte.ts`**
  (pointer/drag/context-menu controller), **`viewer-word.ts`** (word-boundary via `Intl.Segmenter`: the word under a
  caret, and the next boundary in either direction), **`viewer-selection-granularity.ts`** (pure caret → word/line range
  snapping and the two-range union), **`viewer-caret-motion.ts`** (pure `moveFocus`: the five keyboard motions).
- Text cursor: **`viewer-text-cursor.svelte.ts`** (pure `toSpacerRelative` + the `createViewerTextCursor` measurer),
  **`ViewerTextCursor.svelte`** (the one absolutely-positioned bar). See § "Text cursor".
- **`viewer-search-scroll.ts`**: pure per-axis scroll math. `recenterOffset` centres a search match from its rendered
  rect; `ensureVisibleOffset` nudges a line just into view for keyboard extension. Different coordinate spaces, see §
  "Keyboard motion model".
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
- **The image stage's checkerboard needs an explicit `background-repeat: repeat`.** The `ress` reset in `app-reset.css`
  sets `background-repeat: no-repeat` on `*`, so the four 20px hard-stop gradients paint ONCE each in the top-left
  corner and the whole rest of the stage stays flat base color. The symptom reads as "transparent pixels render opaque",
  and it reproduces only inside the app: lift the rule into a standalone page and the checker draws fine, because
  nothing resets `background-repeat` there. Any other tiled-gradient background in the app has the same trap.
- **Its colors are the dedicated `--color-checkerboard-base` / `--color-checkerboard-tile` tokens (`app.css`), not the
  general surface tokens.** `--color-bg-primary` vs `--color-bg-tertiary` is ~1.2:1 (light) and ~1.5:1 (dark), too faint
  to read as a checker even when it tiles correctly. Keep the pair at roughly 1.6:1 or more per theme (Preview.app /
  Photoshop "light" checker range).
- CSP: the `cmdr-media:` token is in `img-src` + `object-src` (`tauri.conf.json`); `viewer-media.spec.ts` locks "no
  `cmdr-media`/`img-src`/`object-src` violation". WKWebView applies EXIF orientation by default (phone photos upright).

### Variable-height word wrap (progressive enhancement)

`viewer-line-heights.svelte.ts` measures per-line wrapped heights for FullLoad files (<1MB) by laying every line out in
a hidden offscreen probe styled exactly like `.word-wrap .line-text` (`measureLineHeightsViaDom`) and reading each
`getBoundingClientRect().height`. It runs once, deferred via `requestIdleCallback` after first render, then builds a
prefix-sum array for O(1) `getLineTop(n)` and O(log n) `getLineAtPosition(y)`. While the measure pass runs (or for
ByteSeek/LineIndex files), the viewer falls back to the averaged-height approach with zero regression.

**Why DOM measurement, not a canvas predictor.** WebKit's own layout is the source of truth for what the viewer renders.
A `measureText`/canvas predictor (this used `@chenglou/pretext`) can't match it for arbitrary bytes: control,
zero-width, combining, and undefined-glyph characters get advances from the font metrics that diverge from what WebKit
paints, so predicted wrap row counts drift. Measured on a 400 KB binary file, pretext over-counted by ~10 px/line (~18%,
verified 2026-07-19 by reading the live height map vs real `.line` rects across lines 0–338), accumulating to ~16,000 px
(~26 screens) of drift by line ~1,600: the classic "lines vanish and drift as you scroll" bug. Measuring observes the
wrap instead of predicting it, so it's correct for binary, emoji, CJK, RTL, and ligatures alike. Cost is a one-time
offscreen layout+read: ~70 ms for ~2.3k lines (measured 2026-07-19), off the critical path in an idle callback. The
`measure` and `schedule` functions are injectable so `viewer-line-heights.svelte.test.ts` unit-tests the prefix-sum and
positioning math without a real layout engine; the DOM measurer itself is covered by `viewer-wordwrap-scroll.spec.ts`.

**Extending exact wrap to ByteSeek/LineIndex (multi-MB) is unsolved.** We can't measure every line for a file we never
fully load into the DOM, and computing wrap in Rust from a per-line char count is a trap: `overflow-wrap: break-word`
wrapping can't be reconstructed from a scalar (the ragged right edge pushes content into rows a `ceil(chars/width)`
division never predicts). The forward path is measure-on-render (estimate unseen lines, patch the prefix-sum and
compensate `scrollTop` as each line renders), which also generalizes the averaged fallback. Not built yet: those paths
keep the averaged-height approximation.

**Integration flow:** The scroll composable creates the height map and exposes `runHeightMapInitEffect` (triggers the
measure pass when word wrap + lines + textWidth are available) and `runHeightMapReflowEffect` (re-measures on width
change, debounced to the resize-settle by `REFLOW_DEBOUNCE_MS` since a full re-measure is ~70 ms, then preserves the
scroll fraction against the container's real `scrollHeight`). The page component wires these as `$effect`s and tracks
`textWidth` via a `ResizeObserver` on `.file-content`. The search composable uses `getLineTop(n)` instead of
`n * scrollLineHeight` for scroll-to-match positioning.

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
- **Selecting to the end of a file with no line count**: in ByteSeek mode before the line index lands, ⌘A can't name a
  last line, so `makeSelectToEof()` mints `focus.line = EOF_LINE` (`Number.MAX_SAFE_INTEGER`). `toRangeEnds` turns that
  into `RangeEnd::Eof` so the backend resolves the real end itself, and `isWholeFileSelection` reads it to hand the copy
  flow the known file size instead of walking lines that were never fetched. Gotcha/Why: `EOF_LINE` is minted directly
  and never derived. A `totalLines - 1` derivation lands one line short of it, every consumer's literal comparison
  silently stops matching, and that is how the `Eof` variant went unemitted while looking wired up.
- **Render**: the page calls `getLineSegmentBounds(selection, lineNumber, lineLength)` and passes the bounds to
  `search.getHighlightedSegments(...)`. The shared `segmentLine()` function (in `line-segments.ts`) merges search-match
  spans with selection bounds and emits non-overlapping `LineSegment`s tagged `highlight` / `active` / `selected`. The
  template renders each segment as a `<mark>` (search) or `<span class="selected">` or plain text.
- **Visual collision**: when a search hit and the selection overlap on the same span, search wins on the background
  (`var(--color-highlight)`) and selection wins on the foreground (`var(--color-selection-fg)`, gold). Matches the
  "selected = gold" language from the file list (design-system.md § File list).
- **Words come from `viewer-word.ts`**, the ONE caller of `Intl.Segmenter`'s word granularity: `findWordBoundsAt` for
  the word under a double-click, `findWordEndAfter` / `findWordStartBefore` for the next boundary in either direction
  (Option+Shift+Arrow). `Intl.Segmenter` gives the boundaries and `isWordSegment()` decides which segments are words, so
  all three share one workaround. Gotcha/Why: ❌ never go back to `Intl.Segmenter`'s own `isWordLike`. JavaScriptCore
  returns `false` for every segment ICU classifies as numeric, which is any word ENDING in a digit (`123`, `3.14`, `v2`,
  `sha256`, `abc123`), so a double-click on `"1292507278647433"` selected the JSON key before it. The boundaries
  themselves are correct on every engine; only the flag lies. Node's ICU gets the flag right, so a plain unit test can't
  see this: `viewer-word.test.ts` stubs a JSC-shaped segmenter to hold the line. (Verified on macOS 26.5.2 WKWebView vs.
  Node 24 and Playwright WebKit 26.5, offscreen WKWebView probe, 2026-08-13.)

### Pointer → caret

Resolving `(clientX, clientY)` to a `LineOffset` is geometric, top to bottom: `viewer-pointer.ts` binary-searches the
rendered `[data-line]` rows by `y` (they're in ascending order, so it reads ~log2(rendered rows) rects), then hands the
hit row's `.line-text` to `findOffsetByGeometry` in `viewer-caret-geometry.ts`, which binary-searches the character
boxes for the point. `measure(offset)` builds one `Range` per probe over the line's text nodes and reads
`getClientRects()`, so a 100k-character line costs the same ~17 probes a short one does.

Decision/Why: the browser's own `caretPositionFromPoint` / `caretRangeFromPoint` are deliberately unused. Both return a
non-null, plausible-looking result on text under `user-select: none` (which `.file-content` is) while some WebKit builds
silently answer offset 0 for every x. That's undetectable at runtime, so there's no safe fast path to be had: one
geometric path serves every webview.

What falls out of doing it ourselves, and what the tests pin:

- **Every point over `.file-content` resolves; only points outside it return `null`.** Keep it that way:
  `handlePointerDown` bails on a `null` caret before it arms the drag, so a dead zone (the gutter, the row padding, the
  blank area under a short file) doesn't cost the anchor, it costs the whole gesture: nothing selected, and a silent
  no-op ⌘C after it.
- **Left of a row's text** (gutter, padding) → the start of that visual row. **Right of it** → the end of that visual
  row, which on a wrapped line is the wrap point, not the end of the logical line.
- **Below the last rendered row** → the end of that line. **Above the first** → offset 0.
- **Word wrap**: character boxes run in reading order, so the search compares row (`y`) first and `x` only within a row.
  A wrapped logical line spans several rows and each resolves independently.
- **Offsets stay on codepoint boundaries.** `measure` snaps onto the start of the codepoint covering the probe and
  reports its two-unit span, so an astral character is one box and the caret can't land between its surrogates.
- **Drag and autoscroll aim through `caretFromPointClamped`.** Past the top or bottom edge it snaps horizontally to that
  side too, so a drag out of the viewport sweeps whole visual rows as they scroll by instead of freezing.

Known gap: **a click inside a right-to-left run resolves to a wrong-but-plausible offset.** `findOffsetByGeometry`
binary-searches the character boxes assuming reading order is monotone, which holds for LTR text but not inside an RTL
run (Hebrew or Arabic file content), where x runs the other way as the offset grows. The damage is contained: the
comparison only misorders when the point falls inside the RTL run's own x range, so the LTR parts of a mixed line still
resolve correctly, and every row-level behavior above is unaffected. The fix, if it's ever worth the work, is to find
the bidi run rect containing the point via `Range.getClientRects()` (one rect per run) and binary-search within that run
in the run's own direction.

### Click cycle (word and line selection)

A second press selects the word, a third the whole logical line (offset 0 to the line's UTF-16 length, so a word-wrapped
line still selects in full), and a fourth starts the cycle over at a plain click, which is what editors do.
`viewer-multi-click.ts` holds the counting as a pure function of the previous press: a press restarts the cycle when it
comes later than `MULTI_CLICK_INTERVAL_MS` (500 ms, matching the macOS default), lands further than
`MULTI_CLICK_SLOP_PX` (4 px) from the one before it, or arrives with an earlier timestamp. Each press is compared
against its predecessor, not against the first, so a gesture that creeps a couple of pixels per press stays one gesture.
A shift-click is an extend gesture and resets the count.

Decision/Why: the count comes from the controller's own `pointerdown` stream, and the page binds no `click` handler at
all. Before that, the same handler read a `click` event's `detail`, and triple-click selected nothing in the app while
double-click worked; since the two branches shared every line but the `detail` comparison, `detail` never reached 3
there. The whole gesture vocabulary now rests on the one event stream the drag already depends on, which also makes it
reachable from a test that dispatches plain `PointerEvent`s (`viewer-selection-gestures.spec.ts`): a synthetic `click`
with a hand-set `detail` would have passed against the broken code.

Gotcha/Why: which part of the native pipeline dropped that count is NOT established, so don't reach back for `detail` on
the theory that some documented rule explains it. `preventDefault()` on `pointerdown` suppresses the compatibility
`mousedown` / `mouseup` per the Pointer Events spec, but a plain WKWebView still fires `click` with `detail` counting 1,
2, 3, 4, including with pointer capture held, with the pressed node replaced mid-gesture, and with DOM focus moved
inside the handler. In other words the obvious suspect is innocent, and an isolated probe can't reproduce the app's
failure. (Verified on macOS 26.6.2 WKWebView, offscreen WKWebView probe driving synthesized `NSEvent`s at clickCount
1-4, 2026-09-02.)

### Selection granularity

A gesture carries a granularity — `character`, `word`, or `line` — and both endpoints of the selection are edges of
ranges snapped to it (`viewer-selection-granularity.ts`). A press counted 2 or 3 arms the drag at `word` / `line` and
remembers the pressed range; every `pointermove` re-derives the whole selection as the union of that anchor range and
the range under the pointer, direction preserved (dragging left of the anchor word anchors at the range's far edge, so a
reversed drag reads correctly and `normaliseSelection` still orders it). Autoscroll's `reAimAfterAutoscroll` routes
through the same call, so a word drag past the viewport edge keeps its granularity.

Decision/Why: re-deriving from the anchor range is what makes a twitch after a double-press harmless. A pointer that
hasn't left the pressed word yields the same union, so there is nothing to collapse. ❌ Don't defend the twitch by
leaving a press counted 2 or 3 unarmed instead: the drag has to stay armed, or double-click-and-sweep selects one word
forever, which is what users reported.

Character granularity keeps calling `setFocus`, since a plain drag genuinely moves one endpoint; `word` and `line` call
`setRange`, which sets both at once.

Gotcha/Why: the controller holds **two** pieces of granularity state with different lifetimes, and folding them into one
silently breaks shift-click. `dragGranularity` describes the drag in progress and `endDrag` resets it;
`gestureGranularity` plus the remembered anchor range describe the gesture and only a plain (count 1, non-shift) press
resets them. `endDrag` fires on the double-press's own `pointerup`, so a shift-click reading the drag's granularity
would always see `character`.

Shift-click therefore extends at the gesture's granularity from the remembered anchor range, matching native, and still
doesn't advance the click cycle.

Edge case: during a fast autoscroll into unfetched lines, `getLineText` returns `undefined`, so the range collapses on
that line. The character path has the same hole and the row renders empty anyway, so the selection matches what the user
sees.

### Keyboard motion model

Extending a selection from the keyboard is "keep the anchor, move the focus", so `viewer-caret-motion.ts` answers only
the focus half. `moveFocus({ from, motion, getLineText, getTotalLines, desiredColumn })` takes one of five motions —
`char`, `word`, `line`, `lineEdge`, `docEdge`, each with a `direction` of `-1` or `+1` — and returns
`{ focus, targetLine, desiredColumn }`. The union makes the key map an exhaustive switch a test can enumerate, and the
module is pure: it holds the line cache and the line count, and knows nothing about what's on screen.

Three result shapes, and the caller has to handle all three:

- **`focus` set**: the motion landed; `targetLine` matches it.
- **`focus: null` with a real `targetLine`**: the line the motion wants isn't in the cache. The caller consumes the key,
  leaves the selection alone, and **still** scrolls to `targetLine`. Decision/Why: that scroll is what puts the line in
  the render window and triggers the fetch, so the next press succeeds. A bare `null` would make the press a permanent
  no-op, because nothing would ever fetch the line it wanted. ❌ Never guess an offset for an unfetched line: it crosses
  the IPC boundary into `viewer_read_range`. Only reachable by out-running the 100 ms fetch debounce with key repeat.
- **`focus` equal to `from`**: the motion hit the edge of the file. Vertical motions clamp rather than jumping to the
  file edge; `docEdge` is the gesture that goes there on purpose.

Rules the model encodes:

- **Vertical motion moves one LOGICAL line, not one visual row.** Native moves by visual row under word wrap, but every
  other navigation here (`scrollByLines`, the height map, the search jump) counts logical lines, so a visual row would
  be the viewer's only second coordinate system. Upgrading later means teaching the model about the height map; don't
  build it now.
- **Vertical motion keeps a desired column** so walking down through a short line and back returns to the original
  column. It's a logical UTF-16 offset, matching the rule above; horizontal motions return `desiredColumn: null`, and
  the caller clears it on any fresh gesture too (a pointer press or ⌘A), through `resetDesiredColumn`.
- **`char` steps one grapheme**, so an emoji, a ZWJ sequence like 👨‍👩‍👧, or a base letter plus a combining mark is one
  press. Offsets stay UTF-16 code units throughout; grapheme stepping only decides how many of them a press covers.
  `viewer-pointer.ts` keeps caret geometry on codepoint boundaries, and grapheme boundaries strictly refine those, so
  that invariant still holds.
- **`word` follows macOS**: right lands on the END of the next word, left on the START of the previous one, via
  `findWordEndAfter` / `findWordStartBefore`. When only punctuation and whitespace are left, the stop is the line edge;
  once there, the motion crosses to the neighbouring line. An empty or wordless line is a stop of its own, so a blank
  line between paragraphs isn't skipped.
- **`docEdge` down has three branches**: the last line is cached → its exact end, one press; it isn't (the common case
  on a large file, since the cache only holds fetched windows) → `{ focus: null, targetLine: totalLines - 1 }`, so the
  caller's scroll fetches it and a second press lands it; there's no line count at all (ByteSeek before the index) → the
  `EOF_LINE` sentinel, which `toRangeEnds` maps to `RangeEnd::Eof`. That branch's `targetLine` is the sentinel too, and
  it names no scrollable row; `scroll.ensureLineVisible` reads it as the end of the file, and ❌ nothing may pass it to
  line arithmetic. ❌ Don't reach for the sentinel merely because the last line isn't cached: that makes it a live,
  movable focus, which is the wedge the precondition below exists to block. And ❌ don't call `selectToEof()` here — it
  sets BOTH endpoints and would silently destroy the user's anchor.
- **Consequence worth knowing rather than rediscovering**: ⌘+Shift+Down from mid-file, then copy, shows the "unknown
  size" confirm on a large file. `isWholeFileSelection` bails on a start past `(0, 0)`, so `estimateSelectionBytes`
  walks lines and returns `null` at the first one with an unknown byte length. It terminates safely and it's defensible.

Gotcha/Why: **`moveFocus` never receives the `EOF_LINE` sentinel as its `from`, and throws on one.** ⌘A in
ByteSeek-no-index mode parks the focus on the sentinel, and it names a line that can never be cached, so one Shift+Up
from there would ask to step onto it forever. The refusal can't live inside the module: it knows nothing about what's on
screen, so the only `targetLine` it could hand back is the sentinel itself, and the caller would slam the view to the
bottom on every press with no way to shrink the selection. `resolveFrom` in `viewer-keyboard.ts` replaces a sentinel
focus with the last line the scroll composable is rendering (at that line's cached length) before calling in, and treats
the press as a no-op when nothing is rendered. That works because reaching the sentinel always scrolled the view to the
bottom, so the last rendered line is the practical end of the file. Keeping the module total is what makes its
exhaustive-switch test worth anything.

#### The key map

Twelve extend chords, all routed by `extendMotionFor` in `viewer-keyboard.ts`:

- **Shift+Left / Right** → `char`, one grapheme, crossing line boundaries.
- **Shift+Up / Down** → `line`, one logical line, keeping the desired column.
- **⌥⇧Left / Right** and **⌃⇧Left / Right** → `word`. Same motion twice: ⌥ is macOS, ⌃ is Linux and Windows (macOS
  usually eats ⌃⇧Arrow at the system level, which costs nothing to support).
- **Shift+Home / End** → `lineEdge`.
- **⌘⇧Up / Down** → `docEdge`; down is the two-press case above.

Beside them, **unmodified Left / Right scroll horizontally by one column** (`scroll.scrollByColumns`), a natural no-op
under word wrap where nothing overflows. Unmodified Home / End still scroll to the file edges.

Decision/Why: **Shift+Home means the LINE edge while bare Home means the FILE edge**, and that's deliberate. Unmodified
Home / End are scroll-view navigation, which is what macOS does in a document view and what the viewer already did; a
selection gesture works on the line in every editor, and ⌘ promotes it back to the whole file. Don't harmonize them.

Decision/Why: **unmodified arrows always scroll rather than moving a cursor.** The viewer's job is looking at a file,
not editing one, and a key map that changes meaning when a setting flips is worse than a slightly non-editor-like one.

Decision/Why: **extending with no selection at all is a no-op** (the key is still consumed, so the view doesn't scroll
out from under the gesture). A plain click already leaves a collapsed selection at the click point, so
click-then-Shift+Arrow is the discoverable path; seeding an anchor from the top of the viewport would start a selection
the user can't see.

#### Routing, and the two traps

Gotcha/Why: the two entry points sit in different halves of `handleKeyDown`, and each is placement-sensitive.

- **Unmodified Shift+Arrow / Home / End** arrive on the bare-key path. The extend branch goes **after the
  `if (searchInputFocused) return` guard and before `handleBareKey`**. Before that guard it steals the search input's
  own Shift+Arrow; after `handleBareKey` Shift+Up keeps scrolling instead of extending.
- **⌥⇧ / ⌃⇧ / ⌘⇧ chords** arrive in `handleModifiedKey`, which runs **regardless of focus**. The branch sits after the
  search chords (so ⌘⌥R / ⌘⌥C still win) and before the `if (!e.altKey && !e.shiftKey)` bail, and **gates on
  `!searchInputFocused` itself** — without that gate it steals ⌥⇧← / ⌥⇧→ from a focused search input.

Gotcha/Why: ❌ never write `e.shiftKey && e.key === 'ArrowLeft'`. `cmdr/no-raw-key-match` is an **error** here and fires
on exactly that shape (a required modifier read sharing a boolean expression with a literal key test while leaving a
modifier unconstrained). `extendMotionFor` uses the guard-then-branch shape the rule deliberately doesn't catch:
`switch (e.key)` first, modifier flags read in a separate statement inside the branch.

#### Following the focus with the view

Every extend press ends in a scroll, on both the landed and the uncached path:

- **Vertically**, `scroll.ensureLineVisible(line)` moves as little as it can, leaving an already-visible line alone, and
  it takes the `EOF_LINE` sentinel too: the ONE branch reading that as "the end of the file" lives there, so the
  keyboard hands it `targetLine` unexamined. Gotcha/Why: the branch looks redundant, because the sentinel currently
  survives `getLineTop` through integer overflow plus the browser clamping an absurd `scrollTop`. Don't lean on that,
  and above all don't "tidy" the arithmetic with `Math.min(line, totalLines - 1)`: the line count is `null` exactly when
  the sentinel appears, so the clamp yields `NaN`, and `scrollTop = NaN` throws the view to the TOP of the file.
  `viewer-scroll.svelte.test.ts` pins both halves. It wraps `ensureVisibleOffset` in `viewer-search-scroll.ts`.
  Gotcha/Why: that file's other export, `recenterOffset`, speaks **viewport-relative rendered-rect** coordinates while
  `ensureVisibleOffset` speaks **content-relative scaled** ones (the space `getLineTop` and `scrollTop` live in,
  compressed by `scrollScale` on files over `MAX_SCROLL_HEIGHT`). They are not interchangeable, and feeding either a
  `line × lineHeight` estimate mislands it under word wrap, where a wrapped line is one tall row. A line taller than the
  viewport is left alone while any of it is on screen, so a long wrapped paragraph doesn't get yanked around on every
  press.
- **Horizontally**, `scroll.ensureColumnVisible(focus)` measures the focus character with `caretRectFor` and recentres
  through `recenterOffset` against the content box, exactly as `scrollToMatch` does for a search hit. Without it,
  repeated Shift+Right on a long unwrapped line walks the focus past the right edge with nothing following it:
  `handleScroll` tracks only `scrollTop` / `viewportHeight`, and the viewer's only other horizontal scroll is search's.
  Skipped under word wrap.

`viewer-pointer.ts` is the one module allowed to measure line text geometry, so both new measurements live there:
`caretRectFor(content, point)` (a zero-width rect on an EDGE of the character box — see its doc comment for why an edge
pick rather than a fallback ladder) and `measureColumnWidth(content)` (one column's advance, cached by the scroll
composable and dropped when the text scale settles).

## Text cursor

An optional thin blinking bar at the selection's focus, off by default (`viewer.showTextCursor`). Purely a render layer
over state that already exists: nothing about it feeds back into the selection, the keyboard, or the scroll composable,
which is what lets it be a setting rather than a mode.

**Vocabulary boundary, because the two words are one letter apart in meaning.** **Caret** is a resolved text POSITION
(`LineOffset`, `caretFromPoint`, `viewer-caret-geometry.ts`, `caretRectFor`) and predates this feature. **Text cursor**
is the rendered ELEMENT, and it owns the setting id, the `.text-cursor` class, `ViewerTextCursor.svelte`, and
`viewer-text-cursor.svelte.ts`. `caretRectFor` returning the box the text cursor paints is exactly where a future agent
starts calling the rendered bar "the caret" and then wonders why `viewer-caret-geometry.ts` renders nothing.

- **Where it mounts**: inside `.scroll-spacer`, as a SIBLING of `.lines-container`. Gotcha/Why: ❌ never inside
  `.lines-container`. `runWrappedLineHeightEffect` computes `avgWrappedLineHeight` as that container's height divided by
  `children.length`, so one extra child shrinks the average and corrupts `scrollLineHeight`, `visibleFrom` /
  `visibleTo`, and `spacerHeight` in word-wrap mode until the height map is ready. A cursor that quietly breaks virtual
  scrolling is the worst bug this feature could ship.
- **How it's placed**: `caretRectFor(content, focus)` measured live, then `toSpacerRelative` subtracts the spacer's
  `getBoundingClientRect()`. Gotcha/Why: that subtraction is the WHOLE conversion. ❌ Never add
  `translateY(linesOffset)` on top of it: `caretRectFor` bottoms out in `Range.getClientRects()`, so its rect is a
  VIEWPORT rect read off the rendered row and already carries the container's transform and the scroll position.
  Applying the transform twice puts the cursor 10⁵-10⁷ px off screen on a large file.
- **When it's hidden**: media mode, the setting off, no selection, a focus line that isn't rendered (including the
  `EOF_LINE` sentinel, which has no row of its own), or a rect that measures no height.
- **Re-measured** in an `$effect` the page wires, after `tick()`, keyed on the selection focus plus everything that can
  move a rendered row under an unchanged focus: the scroll position, `linesOffset`, the rendered line set, and the wrap
  flag. A superseded run is dropped by a generation counter rather than racing the current one.
- **Blink**: a CSS `step-end` animation, added only under `@media (prefers-reduced-motion: no-preference)` so reduced
  motion gets a solid bar, and re-keyed with `{#key}` on every focus change. Without the re-key, a keypress landing in
  the blink's "off" half looks like the cursor vanished (design principle 3).
- **`aria-hidden`**: a visual echo of a selection the page's own `aria-live` region already announces, so a second
  announcement would be noise. It's its own component so `viewer.a11y.test.ts` can mount it; that file mounts individual
  components by design and never mounts `+page.svelte`.
- **The setting is read reactively** through `getViewerShowTextCursor()` (`lib/settings/reactive-settings.svelte.ts`),
  not a one-shot `getSetting` at mount, because the viewer has no control of its own for it: a flip in Settings has to
  reach an already-open viewer. `viewer.wordWrap` reads once at mount only because `W` is its primary control.
  Restricted windows already receive live updates over the cross-window `settings:changed` event, so this costs nothing
  extra. Plumbing: `lib/settings/DETAILS.md` § "Restricted-window mode".
- **The viewer READS this setting and never writes it**, so it is in the `get_restricted_window_settings` snapshot and
  ❌ deliberately NOT in `RestrictedWindowPersistableSetting` / `PERSIST_ALLOWLIST`, unlike `viewer.wordWrap` (`W`
  writes it) and `fileViewer.suppressBinaryWarning` (the banner's button does). Its only control is the Settings row in
  the main window, which has full store access. Adding a persist entry to match the two settings beside it would hand
  the app's highest-risk webview a write nothing uses; `restricted-settings.test.ts` pins the read-only half.

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

## User-facing copy (i18n)

Every user-facing viewer string lives in the `viewer.*` message catalog
(`apps/desktop/src/lib/intl/messages/en/viewer.json`), resolved through `$lib/intl` (`t()` / `tString()` /
`getMessage()`, and `<Trans>` for the inline-component binary-warning banner). The base-en output is a parity-protected
MOVE of the original copy; `viewer-i18n-parity.test.ts` asserts byte-identical en rendering.
`cmdr/no-raw-user-facing-string` is enforced for `lib/file-viewer/` and `routes/viewer/`, so a new hardcoded string in a
known sink (text node, `title`/`label`/`placeholder`/`aria-label`, `addToast` first arg) fails lint: add a catalog key
instead. Two literals are deliberately not copy and carry an eslint-disable: the `Aa` and `⌘C`/`⌘A` typographic/shortcut
glyphs (the a11y labels and tooltips carry the real copy). The runtime works in the capability-restricted viewer window
(pure `Intl` + static-imported JSON, no IPC).

## A read that didn't come back

`viewerGetLines` throws the backend's typed `ViewerError` with its fields copied onto the `Error`, so the deadline is a
VARIANT (`kind: 'timedOut'`) rather than a flag beside a sentence. Both surfaces read it the same way, through
`asViewerError(e)?.kind`: `viewer-scroll.svelte.ts` routes `timedOut` to `deps.onTimeoutError()` and logs everything
else by kind, and `+page.svelte`'s `openFailureCopy` maps `timedOut` / `extractTooLarge` / `archive` to their own
catalog keys and falls back to `viewer.error.readFailed` for anything that never reached the typed path. Nothing renders
the backend's own words. The wider split: `docs/guides/error-handling.md`.

`extractTooLarge`'s key is `viewer.error.tooLargeToPreview`, deliberately not named after archives: the preview cap is
reached by a `.zip` entry AND by a blob in a repository's virtual `.git` snapshot, so the copy says "from here" and
names neither. `archive` next to it IS archive-only (an encrypted, corrupt, or unsupported-codec archive entry), so
`viewer.error.archiveUnreadable` keeps its name.

## Gotchas

- `$state(false)` in `.svelte.ts` triggers `@typescript-eslint/no-unnecessary-condition` because the linter doesn't know
  the value is mutated via Svelte reactivity. Use an inline eslint-disable comment with a reason.
- **`user-select: none` on `.file-content` is deliberate.** The viewer owns its own selection model (above); the
  browser's native selection would render a competing-and-broken one on top of ours that loses its anchor as soon as the
  line scrolls out. `.status-bar` opts back in with `user-select: text` so users can still copy the file name or line
  count. `.line-number` keeps the global default (`none`), it's aria-hidden chrome.
  - The browser's caret-from-point APIs are wrong on `user-select: none` text in some WebKit builds, so we don't use
    them at all (§ "Pointer → caret"). ❌ Don't reintroduce one as a "fast path": the wrong answer looks exactly like a
    right one.
- **Selection offsets are UTF-16 code units, not bytes or grapheme clusters.** When you add features that compute
  offsets from a click position (caret math in `viewer-pointer.ts`) or accept them across the IPC boundary
  (`viewer_read_range`), preserve the UTF-16 convention. The backend handles the conversion to UTF-8 bytes, clamping
  lone surrogates to the nearest codepoint boundary.
- **A pointer gesture in the content claims DOM focus, and ⌘C follows focus.** `handlePointerDown` calls
  `preventDefault()` to keep the native selection out of the way, which also suppresses the native focus move: focus
  stays wherever it was, and the search bar is where it usually was (`openSearch()` focuses AND selects the input). So
  the handler calls the `takeFocus` dep first, which the page wires to `scroll.containerRef.focus()` — the same element
  it focuses after a session opens, so the viewer has one focus home. The keyboard router has the matching second half:
  with the search input focused it hands ⌘A and ⌘C to the input's native handler, EXCEPT when the input holds a bare
  caret, where nothing in it can be copied and the gesture belongs to the file selection (`searchInputHasSelection()` in
  `viewer-keyboard.ts`). Both halves are pinned: `viewer-pointer-drag.svelte.test.ts`, `viewer-keyboard.test.ts`, and
  the "⌘C copies the dragged selection while the search bar is open" case in `viewer.spec.ts`.
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
  build the screen-reader announcement. ⌘A in ByteSeek-no-index mode sets `focus.line = EOF_LINE` (the sentinel that
  maps to `RangeEnd::Eof` at the IPC boundary), so an uncapped loop would iterate 9e15 times. The
  `MAX_ANNOUNCE_LINES = 10_000` cap short-circuits to "Selected from line N to the end of the file" without touching the
  line-length lookup at all.
- **Drag autoscroll honours `prefers-reduced-motion`.** Under reduced motion, `createViewerAutoscroll().start()` does a
  single synchronous snap step and exits without queuing a RAF. The page's `pointermove` calls `start()` on every move,
  so the user still progresses through the file in discrete jumps. Override via the `prefersReducedMotion` dep for
  tests.
- `getLineHeight()` (returns `18px × effective scale`) and the CSS rule
  `.line { height: calc(18px * var(--font-scale)) }` in `+page.svelte` must stay paired. Both read the same scale: the
  JS function for virtualization math, the CSS rule for layout. If you change the 18 base, change both.
- `runHeightMapInitEffect` guards with `if (heightMap.ready) return` to avoid re-measuring when only `textWidth`
  changes. Width-only changes are handled by `runHeightMapReflowEffect` via `reflow()` instead of re-running the
  `prepareLines` pipeline. Without this guard, both effects would race on width changes.
- **The height map's wrap width comes from the row geometry, never from a `.line-text` span.**
  `viewer-text-width.svelte.ts` computes it as the scroll container's `clientWidth` minus the `.line` padding and the
  gutter (`.line-number` width + margin), and the offscreen measurer wraps at exactly that width. `.line-text` is a flex
  item with no `flex-grow`, so it shrink-wraps to its own content: measuring it on a file whose first line is short ("#
  Cmdr", ~44px) once fed a 44px wrap width to the height map, inflating it ~7x (blank space below ~line 60, end of the
  file unreachable). The `.line` row is no better: in no-wrap mode the `.lines-container` is `max-content`, so the row
  is as wide as the widest line. Pinned by `viewer-text-width.svelte.test.ts` and `viewer-wordwrap-scroll.spec.ts`
  (E2E).
- **An empty line measures 0 px; `buildPrefixSum` clamps each line to `getLineHeight()`.** The DOM renders every `.line`
  row at least one line tall (the gutter number keeps the row open), so without the clamp the height map under-counts by
  one row per empty line and the scroll mapping drifts on files with many blank lines.
- **The offscreen measurer mirrors the real `.line` flex layout, including `.line-text`'s `min-width:0`, NOT a bare
  block.** A flex item's default `min-width:auto` (= min-content) changes whether `overflow-wrap:break-word` can break
  an unbreakable run (no break opportunities, e.g. a long base64 blob): with `min-width:0` on `.word-wrap .line-text`
  the run wraps to fit; without it, it overflows on one row. The probe must reproduce whatever `.line-text` does, or it
  over/under-counts and drifts the scroll. Keep the probe's flex row + `min-width:0` in lockstep with
  `.word-wrap .line-text` in `+page.svelte`. Pinned by the no-space-run line in `viewer-wordwrap-scroll.spec.ts` (E2E).
- `closeWindow()`'s `deferWindowClose()` wrapper around `currentWindow.close()` is load-bearing — not decoration, and
  the delay is a real `100` ms rather than `0`. It defends two different failures. (a) Calling `close()` synchronously
  from inside a webview event handler runs webkit2gtk's destruction on the same GTK main-loop tick, stalling other
  webviews' IPC for an undefined duration; a next-tick defer covers this. (b) On macOS, destroying a content-heavy
  webview while a layer-tree commit from its web content process is in flight makes WebKit's
  `RemoteLayerTreeDrawingAreaProxy::commitLayerTree` fault on freed state and kill the whole app with a `SIGSEGV`; a
  next-tick defer does NOT cover this, because that commit arrives on WebKit's own IPC run loop. Measured: `0` ms
  crashed on the 36th close, `100` ms survived 80 closes. Don't lower it, and keep both windows on the shared
  `$lib/window-close-defer` constant. The settings page (`routes/settings/+page.svelte`'s Escape handler) mirrors this
  exact pattern; see `lib/settings/DETAILS.md`, `docs/notes/child-window-close-webkit-crash.md`, and commit `46481b29`
  for the original post-mortem. `setTimeout` also avoids the rAF throttling that WKWebView applies to unfocused windows.
  **The same trap applies to `windowReady`** (the `data-window-ready` attribute every viewer E2E spec waits on): it's
  set via `setTimeout(0)` after session open, NOT rAF — an rAF there starved in unfocused E2E windows and timed out the
  whole viewer suite whenever a human was using the machine. Canonical rule + recurrence history: `docs/testing.md` §
  "`requestAnimationFrame` in unfocused windows".
