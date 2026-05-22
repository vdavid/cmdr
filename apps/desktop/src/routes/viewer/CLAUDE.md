# Viewer

The file viewer opens files in a separate Tauri window with virtual scrolling and text search.

## Files

| File                            | Contents                                                                                    |
| ------------------------------- | ------------------------------------------------------------------------------------------- |
| `+page.svelte`                  | Top-level component: lifecycle, window management, UI                                       |
| `viewer-scroll.svelte.ts`       | Virtual scroll composable: line cache, fetch debounce, scroll compression, effects          |
| `viewer-search.svelte.ts`       | Search composable: start/poll/cancel/navigate, match highlighting, debounce                 |
| `viewer-line-heights.svelte.ts` | Height map for accurate word-wrap scrolling via pretext (FullLoad files only)               |
| `viewer-text-width.svelte.ts`   | `ResizeObserver`-driven tracker for the rendered `.line-text` width                         |
| `viewer-indexing-poll.ts`       | Periodic `viewer_get_status` poll while the backend builds a line index                     |
| `viewer-keyboard.ts`            | Pure helpers `handleNavigationKey` / `handleToggleKey` mapping keys to scroll calls         |
| `selection.svelte.ts`           | Selection model: state + pure helpers (normalise, in-range, segment bounds, byte estimator) |
| `line-segments.ts`              | Pure shared segmenter: merges search matches + selection bounds into render spans           |
| `viewer-pointer.ts`             | Pure caret-from-point math: `(x, y)` -> `LineOffset` with surrogate-safe sibling-offset sum |
| `viewer-copy.ts`                | Pure three-band copy policy (silent / confirm / refuse) and threshold constants             |
| `viewer-copy.svelte.ts`         | Copy composable: state + busy flag + per-call read_id + cancel plumbing                     |

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

## Gotchas

- `$state(false)` in `.svelte.ts` triggers `@typescript-eslint/no-unnecessary-condition` because the linter doesn't know
  the value is mutated via Svelte reactivity. Use an inline eslint-disable comment with a reason.
- **`user-select: none` on `.file-content` is deliberate.** The viewer owns its own selection model (above); the
  browser's native selection would render a competing-and-broken one on top of ours that loses its anchor as soon as the
  line scrolls out. `.status-bar` opts back in with `user-select: text` so users can still copy the file name or line
  count. `.line-number` keeps the global default (`none`), it's aria-hidden chrome.
- **Selection offsets are UTF-16 code units, not bytes or grapheme clusters.** When you add features that compute
  offsets from a click position (M3a's caret math) or accept them across the IPC boundary (M2's `viewer_read_range`),
  preserve the UTF-16 convention. The backend handles the conversion to UTF-8 bytes, clamping lone surrogates to the
  nearest codepoint boundary.
- `getLineHeight()` (returns `18px × effective scale`) and the CSS rule
  `.line { height: calc(18px * var(--font-scale)) }` in `+page.svelte` must stay paired. Both read the same scale: the
  JS function for virtualization math, the CSS rule for layout. If you change the 18 base, change both.
- `runHeightMapInitEffect` guards with `if (heightMap.ready) return` to avoid re-preparing when only `textWidth`
  changes. Width-only changes are handled by `runHeightMapReflowEffect` via `reflow()` (instant) instead of re-running
  the async `prepareLines` pipeline. Without this guard, both effects would race on width changes.
- `closeWindow()`'s two `requestAnimationFrame`s before `currentWindow.close()` are load-bearing — not decoration.
  Calling `close()` synchronously from inside a webview event handler runs webkit2gtk's destruction on the same GTK
  main-loop tick, stalling other webviews' IPC for an undefined duration. The settings page
  (`routes/settings/+page.svelte`'s Escape handler) mirrors this exact pattern for the same reason; see the Gotcha note
  in `lib/settings/CLAUDE.md` and commit `46481b29` for the post-mortem.
