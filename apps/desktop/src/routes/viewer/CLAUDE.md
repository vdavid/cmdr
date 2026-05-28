# Viewer

The file viewer opens files in a separate Tauri window with virtual scrolling and text search.

Backend counterpart: [`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`](../../../src-tauri/src/file_viewer/CLAUDE.md)
for the three backend strategies (chunked, full-load, pretext), session orchestration, and background search. Reusable
FE primitives live at [`src/lib/file-viewer/CLAUDE.md`](../../lib/file-viewer/CLAUDE.md).

## Files

| File                            | Contents                                                                                                                                        |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `+page.svelte`                  | Top-level component: lifecycle, window management, UI                                                                                           |
| `viewer-scroll.svelte.ts`       | Virtual scroll composable: line cache, fetch debounce, scroll compression, effects                                                              |
| `viewer-search.svelte.ts`       | Search composable: start/poll/cancel/navigate, match highlighting, debounce, `useRegex` and `caseSensitive` toggles, regex-error projection     |
| `viewer-line-heights.svelte.ts` | Height map for accurate word-wrap scrolling via pretext (FullLoad files only)                                                                   |
| `viewer-text-width.svelte.ts`   | `ResizeObserver`-driven tracker for the rendered `.line-text` width                                                                             |
| `viewer-indexing-poll.ts`       | Periodic `viewer_get_status` poll while the backend builds a line index                                                                         |
| `viewer-keyboard.ts`            | Pure helpers `handleNavigationKey` / `handleToggleKey` mapping keys to scroll calls                                                             |
| `selection.svelte.ts`           | Selection model: state + pure helpers (normalise, in-range, segment bounds, byte estimator)                                                     |
| `line-segments.ts`              | Pure shared segmenter: merges search matches + selection bounds into render spans                                                               |
| `viewer-pointer.ts`             | Pure caret-from-point math: `(x, y)` -> `LineOffset` with surrogate-safe sibling-offset sum                                                     |
| `viewer-copy.ts`                | Pure three-band copy policy (silent / confirm / refuse) and threshold constants                                                                 |
| `viewer-copy.svelte.ts`         | Copy composable: state + busy flag + per-call read_id + cancel plumbing + saveAs                                                                |
| `viewer-autoscroll.ts`          | Pure speed curve for drag-past-edge autoscroll                                                                                                  |
| `viewer-autoscroll.svelte.ts`   | Autoscroll RAF controller: start / stop / self-terminate                                                                                        |
| `viewer-word.ts`                | Pure word-boundary finder via `Intl.Segmenter` for double-click selection                                                                       |
| `ViewerContextMenu.svelte`      | Minimal in-app right-click menu (Copy, Select all)                                                                                              |
| `EncodingPicker.svelte`         | `<select>` + `<optgroup>` Unicode / Western. Reactive to backend `EncodingChoice[]`. The detected encoding gets a "(Detected)" suffix.          |
| `ViewModePicker.svelte`         | `<select>` placeholder for future view modes (today: only "Text", disabled).                                                                    |
| `viewer-tail.svelte.ts`         | `createViewerTail()` composable: listens to `viewer:file-changed:<sid>` events and dispatches to reload toasts or a side effect.                |
| `ViewerReloadToast.svelte`      | Component content for the persistent reload toast. Reads its session id from `setReloadToastContext()` (the toast system mounts without props). |
| `viewer-tail-persistence.ts`    | SHA-256-keyed LRU (100 entries) of per-path tail-mode flags. Promoted on read; debounced 5 s persist; flushed on session close.                 |

## Architecture

The page component creates two composables via `createViewerScroll` and `createViewerSearch`. Both use callback-based
deps (getters) so they can read reactive state from the page without receiving `$state` directly (which would lose
reactivity). The page owns session-level state (`sessionId`, `totalLines`, `backendType`, etc.) and wires the
composables together.

Effects live in the page component but delegate to `run*Effect()` methods on the composables, because `$effect()` only
works in `.svelte` or `.svelte.ts` files at the top level of a component or `createXxx` function scope.

### Variable-height word wrap (progressive enhancement)

`viewer-line-heights.svelte.ts` uses `@chenglou/pretext` to compute per-line wrapped heights for FullLoad files (<1MB).
It runs `prepare()` asynchronously via `requestIdleCallback` after first render, then builds a prefix-sum array for O(1)
`getLineTop(n)` and O(log n) `getLineAtPosition(y)`. While preparation runs (or for ByteSeek/LineIndex files), the
viewer falls back to the existing averaged-height approach with zero regression.

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

Per-path persistence: the user's last tail-mode choice is stored under a SHA-256-truncated key in `viewer-tail.json`.
Cap 100 entries; LRU is access-promoted (reading promotes recency). Writes debounce 5 s in memory; the session also
flushes on close for crash safety. Cross-mount aliases (same content via two paths) keep separate entries on purpose:
from Cmdr's point of view those are two files.

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
  listener-order change. See `tryConsumeEscapeForCopy` in `+page.svelte` and `handleKey` in `ViewerContextMenu.svelte`.
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
- `closeWindow()`'s `setTimeout(() => …, 0)` before `currentWindow.close()` is load-bearing — not decoration. Calling
  `close()` synchronously from inside a webview event handler runs webkit2gtk's destruction on the same GTK main-loop
  tick, stalling other webviews' IPC for an undefined duration. The settings page (`routes/settings/+page.svelte`'s
  Escape handler) mirrors this exact pattern for the same reason; see the Gotcha note in `lib/settings/CLAUDE.md` and
  commit `46481b29` for the original post-mortem. The defer used to be two nested `requestAnimationFrame`s; that flaked
  on macOS E2E because WKWebView throttles rAF for windows that opened without focus, pushing the deferred close past
  the test's confirmation budget. `setTimeout(0)` achieves the same "next event-loop tick" guarantee without the
  throttling pitfall.
