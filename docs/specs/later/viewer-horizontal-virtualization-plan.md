# File viewer: horizontal virtualization for long lines

Status: draft (planning). Worktree: `david/viewer-hviz`. Owner area: `apps/desktop/src/routes/viewer/`.

## Problem

Opening a file with very long lines is slow, and the slowness is in the frontend, not the backend.

Evidence (prod log, 2026-06-11): a 361 KB binary shown as text took ~1.3 s window→render, vs 52 ms for `pnpm-lock.yaml`
of similar line count. Backend decode for that file is ~7 ms (measured: `detect` 0.66 ms + `build_from_bytes` 6.5 ms +
`get_lines` 0.18 ms). So the ~1.3 s is the browser shaping and laying out the rendered lines.

Root cause: in no-wrap mode the viewer hands the DOM the **entire line string** for every visible row (`+page.svelte`
`.line-text` is `white-space: pre`, full `text`), and `.lines-container` is `width: max-content`, so the browser:

1. shapes each visible line's full text (font fallback per script run is the expensive part), and
2. computes the intrinsic width of the widest visible line every time `runContentWidthEffect` reads
   `linesContainerRef.scrollWidth` (`viewer-scroll.svelte.ts:289`).

Both costs scale with **rendered line length**, not file size, encoding, or script. A 500 KB Chinese UTF-16 text file
with normal line lengths is NOT slow (vertical virtualization already caps it to ~50 visible rows of ~40 chars). The
pain is long lines: minified JS / JSON, no-newline blobs, and binaries-shown-as-text. The fix must make per-line cost
flat regardless of line length.

## Goal and non-goals

Goal: in no-wrap mode, render only the horizontally-visible character window of each visible line (plus a buffer),
positioned at the correct x, with a correct horizontal scrollbar extent. Cost per frame is bounded by viewport size, not
line length.

Must preserve (these are the "don't break it" contract):

- The UTF-16-code-unit selection model (`selection.svelte.ts`): offsets are UTF-16 code units over the **full** line.
- Search match highlighting (`viewer-search`, `line-segments.ts`): match spans are full-line UTF-16 ranges.
- Pointer→caret math (`viewer-pointer.ts`): click resolves to `{ line, offset }` in full-line UTF-16 units.
- Vertical virtual scrolling (`viewer-scroll.svelte.ts`): unchanged.
- Word-wrap mode (`viewer-line-heights.svelte.ts` + pretext): unchanged. Horizontal virtualization is a **no-wrap-only**
  concern; in wrap mode there's no horizontal overflow.

Non-goals: a hex/binary view mode; changing the backend; touching ByteSeek/LineIndex line-fetch logic; proportional-font
pixel perfection (see the approach decision).

## Approach decision (read this before implementing)

Three candidate mechanisms were considered. The viewer renders with a **monospace** font (`--font-mono`), which is the
deciding fact.

### Chosen: cell-grid horizontal virtualization (deterministic, monospace-aware)

Model each line as a sequence of fixed-width **cells**. In a monospace font every grapheme occupies an integer number of
cells: ASCII/Latin = 1, CJK/fullwidth & most emoji = 2, combining marks = 0, a tab advances to the next 8-cell stop
(`white-space: pre`, default `tab-size: 8`). Cell width `W` = the font's advance for one ASCII char (measure once).

From a per-line **CU→cell prefix sum** (built once per line, cached) we get, in O(log line) per lookup:

- line pixel width = `totalCells * W` → horizontal scrollbar extent,
- `cellAtX(px)` / `xOfCu(cu)` → which CU window is visible and where to position it,
- the CU range `[sliceStartCu, sliceEndCu)` to render for a given `scrollLeft` + viewport width + buffer.

Render: the row stays `[gutter | text]`. The text element renders only `line.slice(sliceStartCu, sliceEndCu)`,
translated to `xOfCu(sliceStartCu)`; the row reserves the full `totalCells * W` width so the scrollbar is right.

**The horizontal coordinate system is cell-space, and this is what bounds the error.** The scrollbar extent
(`totalCells * W`), `scrollLeft`, and each slice's left anchor (`startCell * W`) are all expressed in the same cell
grid. We re-anchor every slice render to `startCell * W`. So any mismatch between a glyph's real advance and `n * W`
does NOT accumulate down the line: it resets at every slice. The only place reality differs from the grid is _inside the
currently-rendered window_ (≤ viewport width + buffer of characters), where the browser shapes the real glyphs starting
from the anchor. The intra-slice mismatch is therefore bounded by `(chars in window) × (per-glyph error)`, never by line
length. We render a **generous horizontal buffer** (about a viewport-width of extra characters each side, sized from the
M0 per-glyph error measurement) so that even when real glyphs run wider than the grid predicts, the visible region is
always covered with no gap. Two corollaries this buffer must satisfy explicitly:

- **No mid-line gap under the cursor (the click-right-of-text trap).** A click in empty row space to the right of the
  rendered slice resolves (via `caretPositionFromPoint` → `sumOffsetWithin`) to the slice's end offset, which is _mid-
  line_ and therefore wrong. The right-side over-render must guarantee the rendered glyphs always extend past the
  viewport's right edge whenever there is more line to the right, so no in-viewport click ever lands in empty space
  mid-line. The only time the slice legitimately ends before the viewport edge is at the true end of the line, where
  resolving to the slice end == line end is correct. Tested in M4.
- **The true end of the line stays reachable by native scroll.** The horizontal scroll extent is the spacer's
  `min-width`. If real glyph advances exceed `n * W` (emoji-dense), the real last glyph can sit past `totalCells * W`,
  so a spacer of exactly `totalCells * W` would clip it. Pad `contentWidthPx` by a slack term derived from the M0 error
  bound (worst-case per-line drift) so native `scrollLeft` always reaches the real end. Decision + magnitude recorded
  from M0; for the dominant exact cases (ASCII/Latin/CJK-at-2W) the slack is zero. The right-side over-render buffer and
  this spacer slack must derive from the **same** M0 per-glyph error number, so the slice never wants to read past where
  `contentWidthPx` lets you scroll.

Consequences of that design:

- **Deterministic and cheap.** No per-glyph canvas measurement in the hot path; the prefix sum is integer arithmetic
  (Unicode East Asian Width lookup), O(CU) once per line, cached. A 250 KB single line is a few ms of plain-JS scan,
  paid once. `W` and the per-glyph error bound come from the existing font probe (see M0), not from measuring each line.
- **Exact for the dominant case, bounded-and-cosmetic for the rest.** After the encoding-detection fix (binaries now
  decode as Windows-1252, one cell per byte), the pain cases are integer-cell-clean where it's a true monospace render:
  minified JS = ASCII (1 cell), Latin-1 binary = 1. CJK is 2 cells only if the fallback CJK font renders at `2×W`, which
  is a font-pairing property we must verify in M0, not assume. Emoji / ZWJ sequences and other exotic glyphs are the
  honest weak spot: their real advance is whatever the fallback font does. For those, the cell grid is approximate, but
  per the re-anchoring above the error stays intra-slice (cosmetic horizontal nudging), never wrong text and never a
  wrong caret.
- **Selection/search/pointer math stay in full-line UTF-16 units.** We only translate at the render boundary (clip +
  shift) and the pointer boundary (add `sliceStartCu`). The selection model never learns about slices, and `offset → x`
  (used for scroll-to-match and the slice anchor) is pure cell math, so it's consistent with the scrollbar and
  `scrollLeft` by construction.

Why clicks stay correct even when the grid is approximate: the _click → offset_ direction never uses cell math. The
browser's `caretPositionFromPoint` reads the real shaped glyph positions inside the rendered slice and returns a node +
offset; we sum to a slice-relative CU and add `sliceStartCu`. So a click always lands on the glyph under the cursor
regardless of grid drift. Only the _offset → x_ direction is grid-based, and its error is bounded as above.

**Bidi caveat (real, not cosmetic):** the cell grid assumes visual order equals logical order, which holds only for LTR.
A right-to-left or mixed-bidi run reorders glyphs visually, so `xOfCu` and click resolution would both be wrong, not
just nudged. Two candidate policies, decided in M4 from what M0 shows: (a) force logical-order rendering with
`direction: ltr; unicode-bidi: bidi-override` on `.line-text` — but M0 must _prove_ this actually yields visual==logical
for the cell math AND leaves combining marks attached and LTR text unharmed, rather than assuming it (bidi-override
forces direction, which is not automatically the same as the logical-order layout the cell model needs); or (b) per line
detect a strong-RTL character and fall that single line back to the non-sliced M1 path (`content-visibility` baseline,
full text in the DOM, native bidi). Either way bidi is in the test matrix (M2/M5), not discovered in the field.

### Rejected as primary

- **Per-glyph measured virtualization (canvas / pretext).** The only mechanism that's pixel-perfect for _proportional_
  fonts. Rejected because (a) the viewer is monospace, so it buys nothing the cell model doesn't already get exactly,
  and (b) building a per-glyph x prefix-sum needs O(graphemes) canvas `measureText` calls — for a 250 KB line that's the
  very spike we're removing. **pretext does not rescue this**: despite being our measurement engine for wrap-mode
  height, its public API is line-break-oriented and deliberately does **not** expose per-grapheme x positions ("segment
  widths are browser-canvas widths for line breaking, not exact glyph-position data for … x-coordinate reconstruction").
  So pretext can't drive intra-line horizontal slicing. Keep pretext where it's the right tool (wrap height); don't
  force it here. This path stays documented as the future upgrade if the viewer ever goes proportional.
- **`content-visibility: auto` chunking.** Split each line into inline-block chunks and let WebKit skip off-screen ones.
  Simpler math (full text stays in the DOM, so selection/caret offsets are untouched; only segment rendering splits at
  chunk boundaries). Rejected as the core mechanism because it bets correctness-of-performance on WebKit reliably
  occluding **horizontally** off-screen inline-blocks inside a scroll container — `content-visibility` was designed for
  vertical document flow, and the behavior in the Tauri system-WebKit on our min target is unverified. We don't want the
  "rock solid" guarantee to depend on a browser heuristic. It survives as the **cheap baseline** (M1) layered on top,
  and as the fallback if the cell model hits an unforeseen wall.

## Architecture

All new code lives in `apps/desktop/src/routes/viewer/`. The split mirrors the existing pure-helper + `.svelte.ts`
composable pattern.

- `viewer-cell-model.ts` (pure, new): the monospace cell model. No DOM, no `$state`.
  - `graphemeCellWidth(grapheme: string): 0 | 1 | 2` — Unicode East Asian Width (Wide/Fullwidth → 2, combining/
    zero-width → 0, else 1), with emoji treated as 2. Compact range table; cite Unicode EAW.
  - `buildCellIndex(line: string, tabStop = 8): CellIndex` — walks graphemes via `Intl.Segmenter`, returns a structure
    with: `totalCells`, and the data to answer `cellAtCu`/`cuAtCell` in O(log n). Store cumulative cells at grapheme
    boundaries (Int32Array) plus the grapheme CU starts (Int32Array), so both directions binary-search. Tabs advance to
    the next `tabStop` multiple.
  - `sliceForWindow(index, firstCell, lastCell): { startCu, endCu, startCell }` — the CU window to render and its start
    cell (→ x via `* W`).
  - Rationale to capture in the file: why cells (monospace), why CU-indexed (the whole FE speaks UTF-16 CU), why tabs
    need the running column (not a fixed width).
- `viewer-hscroll.svelte.ts` (composable, new): horizontal state + glue.
  - Owns `scrollLeft` (read from `contentRef.scrollLeft` in `handleScroll`) and `cellWidth` `W`. `W` and the per-glyph
    error bound are obtained by **reusing the existing hidden-probe + canvas/DOM-agreement pattern** already in
    `viewer-line-heights.svelte.ts` (`resolveAndValidateFont`), not a bespoke new probe; re-measure on the same
    `onDebouncedScaleChange` hook `viewer-scroll.svelte.ts` already imports, so we don't thrash mid-drag.
  - Per-line `CellIndex` cache: keyed by **line number**, NOT by the full line string (a binary-as-text file would
    otherwise retain hundreds of MB of strings-as-keys). Capped by **total cached CU**, not line count; only visible
    lines are inserted (principle 5). Invalidation: there is no per-line version signal today — the viewer invalidates
    bluntly via `scroll.lineCache.clear()` on encoding switch (`+page.svelte:179`), reload (`:529`), and tail (`:108`).
    So expose a `cacheGeneration` counter from `viewer-scroll` that bumps inside every `lineCache.clear()`, and have
    `viewer-hscroll` clear its `CellIndex` cache when that counter changes (a cheap `$effect` or a shared clear). Don't
    invent a per-line version that doesn't exist; reuse the existing clear points.
  - `lineWidthPx(line)`, `xOfCu(line, cu)`, and `sliceFor(line, viewportWidth)` returning `{ startCu, endCu, startX }`
    for the current `scrollLeft`, with the generous buffer described above.
  - `contentWidthPx` = a **monotonic** max of `lineWidthPx` held in composable state. It never resets, so scrolling
    backward past the widest line doesn't shrink the scrollbar. For ByteSeek/LineIndex the true widest line is
    unknowable, so the extent grows as wider lines scroll into view (same limitation as today's `scrollWidth`, but now
    non-shrinking); documented as a known seek-backend behavior.
  - Horizontal `MAX_SCROLL_WIDTH` scaling that mirrors the existing vertical `MAX_SCROLL_HEIGHT`/`scrollScale` logic
    (WebKit caps element size at ~2^25 px; a multi-MB single line × `W` can exceed it). `scrollLeft`↔contentX maps
    through the same kind of scale factor. Unit-test the interaction with the vertical scale (they're independent axes).
- `line-segments.ts` (existing, pure): add `clipSegmentsToWindow(segments, startCu, endCu)` — clamp/drop full-line
  render segments to the visible CU window and shift their offsets by `-startCu`. Pure, unit-tested.
- `viewer-pointer.ts` (existing): `caretFromPoint` adds the row's slice start to the summed offset:
  `offset = sumOffsetWithin(...) + Number(lineNode.dataset.sliceStart ?? 0)`. `sumOffsetWithin` is unchanged. The shift
  reads `data-slice-start` off the `[data-line]` row ancestor it already locates (`findLineAncestor`), so it composes
  with the existing `caretPositionFromPoint` / `caretRangeFromPoint` fallback untouched.
- `+page.svelte` (existing): render the slice instead of the full line; sticky gutter; CSS.
  - **Exact DOM shape (load-bearing for the caret math):** `data-slice-start={startCu}` sits on the `[data-line]` row.
    The horizontal translate (`translateX(startX)`) is applied to `.line-text` itself (or a wrapper _inside_ it), so
    `findLineTextNode` (`querySelector('.line-text')`) and the `sumOffsetWithin` tree-walk still see exactly the sliced
    text nodes and the `<mark>` / `.selected` spans as descendants. Never wrap `.line-text` such that the translate
    element sits between the row and `.line-text` without `.line-text` still matching the selector.
  - `.line-text` renders `clipSegmentsToWindow(getHighlightedSegments(lineNumber, text, …), startCu, endCu)` over
    `text.slice(startCu, endCu)`; the row reserves `lineWidthPx` width so the scrollbar extent is right.
  - `direction: ltr; unicode-bidi: bidi-override` on `.line-text` (or the per-line RTL fallback) per the bidi policy
    above.
  - Gutter (`.line-number`) becomes `position: sticky; left: 0` so line numbers stay visible during horizontal scroll (a
    UX improvement that falls out naturally; flag to David, current behavior scrolls them away). Note: sticky on a flex
    item inside a transformed (`translateY`) `max-content` ancestor is fragile in some WebKit builds; verify in M0.
  - `content-visibility: auto` + `contain-intrinsic-size` gated to no-wrap only (`.file-content:not(.word-wrap) .line`)
    as the M1 baseline, so it can never interfere with the wrap-mode height map.
- `viewer-search.svelte.ts` (existing): `scrollToMatch` currently sets only `contentRef.scrollTop`, and in
  ByteSeek-no-index mode it targets a **fractional** position from `match.byteOffset` (it doesn't know the integer line
  number). Add a horizontal axis that **degrades safely**: only when the match's integer line is known AND its text is
  in `lineCache`, compute a target `scrollLeft` from `hscroll.xOfCu(line, match.column)` (centered) and set
  `contentRef.scrollLeft`. When the line isn't cached yet (you just scrolled there) or the line number is unknown
  (ByteSeek-no-index), **no-op the horizontal scroll** (leave `scrollLeft`) or defer until the line is fetched and
  rendered — never guess. `xOfCu` must tolerate a missing line by returning a sentinel the caller skips on.
  `match.column` is UTF-16 code units (per `viewer/CLAUDE.md`), which is exactly the cell model's CU input, so no unit
  conversion. Without this fix, jumping to a match on a long line leaves the highlight at the correct logical spot but
  outside the visible window.
- `viewer-scroll.svelte.ts` (existing): retire `runContentWidthEffect`/`contentWidth` `scrollWidth` read; the spacer
  `min-width` now comes from `viewer-hscroll`'s monotonic `contentWidthPx`. Track `scrollLeft` alongside `scrollTop` in
  `handleScroll`.

## Milestones

Each milestone is independently shippable and leaves the viewer correct. Run `pnpm check --fast` after each unit of work
and `pnpm check` before committing each milestone; `pnpm check --include-slow` before declaring the feature done (it
runs the Playwright viewer specs).

### M0 — Spike + baseline (de-risk, ~½ day, throwaway)

Goal: confirm the cell-model assumptions in the _actual_ Tauri webview and get a real before-number, before writing
production code.

- Generate fixtures with a throwaway script (the fixtures themselves are NOT committed; the bench _note_ is): a 500 KB
  minified-JSON single-ish-line file, a 500 KB CJK UTF-16 file with normal line lengths, a long-CJK-line file, and an
  emoji/ZWJ-dense long line. Record the baseline in `docs/notes/viewer-long-line-bench.md` (committed, linked from
  DETAILS.md).
- Open each in a dev build via MCP (`pnpm dev` + `mcp__tauri__*` / `scripts/mcp-call.sh`); capture window→render timing
  from the viewer debug logs.
- In the webview console, verify against the **actual deployed `--font-mono` plus system fallback** (not a cherry-picked
  font):
  1. ASCII advance `W` is integral and stable.
  2. **Cumulative** check, not single-glyph: measure a long (~10k char) CJK line's real total width vs `totalCells * W`,
     and the same for an emoji/ZWJ line. This quantifies the per-glyph error bound and confirms the buffer size needed
     so the re-anchored slice always covers the viewport. If CJK fallback isn't ≈`2 * W`, decide the policy (measure-
     once-per-distinct-grapheme cache, or accept the bounded intra-slice nudge) before M2.
  3. A tab advances to the 8-cell stop with the CSS `tab-size` we'll pin (see M1).
  4. **The load-bearing caret test, on BOTH webviews** (macOS WKWebView and the pinned Linux `ubuntu:26.04` webkit2gtk
     E2E image, per the `caretRangeFromPoint` `offset:0` gotcha in `viewer/CLAUDE.md`): `caretPositionFromPoint` /
     `caretRangeFromPoint` must return the **correct offset** (not merely non-null) when the text node is `translateX`'d
     by hundreds of px, tested at the **left and right edges** of the slice. If this fails, the approach is dead — stop
     and reconsider.
  5. `position: sticky; left: 0` on the gutter actually pins inside the `translateY`-transformed `max-content` container
     in both webviews (N4 risk).
- Output: a go/no-go note + the measured numbers (W, error bound, buffer size), linked from this plan. No production
  code, no tests.

### M1 — `content-visibility` baseline (cheap, safe, ~1 h)

Goal: a real win for every file with zero risk, independent of the cell model.

- Add `content-visibility: auto; contain-intrinsic-size: auto calc(18px * var(--font-scale))` to `.line`, **gated to
  no-wrap** (`.file-content:not(.word-wrap) .line`) so it can never touch the wrap-mode height-map path. Pin
  `tab-size: 8` on `.line-text` in the same change, with a paired-constant comment (mirrors the existing
  `getLineHeight()` / `18px` pairing) so the cell model's `tabStop = 8` can't silently drift from CSS.
- Re-verify the named pins from `viewer/CLAUDE.md`: `viewer-text-width.svelte.test.ts` and
  `viewer-wordwrap-scroll.spec.ts` must stay green. `content-visibility` can zero out `getBoundingClientRect` on
  off-screen descendants, which would feed a 0 gutter width into the text-width tracker and re-trigger the ~7x
  height-map inflation gotcha. Note the text-width tracker (`viewer-text-width.svelte.ts`) runs in BOTH wrap modes and
  measures the gutter `.line-number`, so gating `content-visibility` to no-wrap isn't a complete defense for it: confirm
  the tracker measures a _visible_ row's gutter (or reads `.line-number` `min-width`, which `content-visibility` doesn't
  zero) and add that check to the re-verify list, not just the two pins.
- Tests: the two pins above plus the existing viewer Playwright specs stay green (no behavior change intended). No new
  unit test (pure CSS).
- This milestone can land on its own even if M2+ slips.

### M2 — Pure cell model (TDD, ~1 day)

Goal: the measurement core, fully unit-tested, no rendering yet.

- **TDD (real red→green):** write `viewer-cell-model.test.ts` first and watch it fail, then implement
  `viewer-cell-model.ts`. This is exactly the "risky logic" the `tdd-red-green` rule targets — off-by-one cell math and
  CU↔cell mapping are bug-prone.
- Test cases: ASCII width 1; CJK fullwidth width 2; combining mark width 0; ZWJ-emoji sequence is one grapheme with a
  single (policy-defined) width, not summed components; surrogate-pair CU accounting (astral char = 2 CU, correct cell
  width and CU boundaries, slice never splits a pair); tab to next 8-stop from various starting columns (and a tab after
  a wide char, so column ≠ CU count); **CRLF: a trailing `\r` contributes 0 cells** (the backend keeps `\r` in the line
  string per `range_read.rs`; an EAW table that returns 1 for `\r` would drift the final slice — assert `totalCells` and
  `xOfCu` of the last real char); other C0 controls (Windows-1252 binary case) get a defined width; `cellAtCu` /
  `cuAtCell` round-trips; `sliceForWindow` boundaries; empty line; a strong-RTL char triggers the bidi policy (per-line
  fallback flag or override, whichever M4 picks — assert the model reports it).
- Checks: `pnpm check --fast` (covers Svelte ESLint, type-drift) then `pnpm check svelte`.

### M3 — `clipSegmentsToWindow` (TDD, ~½ day)

Goal: render-segment clipping, pure and tested, before wiring into the template.

- **TDD:** extend `line-segments.test.ts` first (red), then add `clipSegmentsToWindow` to `line-segments.ts`.
- Test cases: segment fully inside window; fully outside (dropped); straddling start; straddling end; window inside one
  segment; offset shift correctness; a search `<mark>` + a selection span overlapping at the window edge (the existing
  collision case, clipped).
- Checks: `pnpm check svelte`.

### M4 — `viewer-hscroll` composable + render wiring (~1.5 days)

Goal: the feature visible and working in no-wrap mode.

- Implement `viewer-hscroll.svelte.ts` (state, `W` via the reused probe, line-number-keyed CU-capped cache, `sliceFor`,
  monotonic `contentWidthPx`, horizontal scale). Unit-test the monotonic-max behavior (scrolling back doesn't shrink
  it), the scale math and its independence from the vertical scale, and `sliceFor` (the parts that don't need the DOM);
  the `W` probe and `scrollLeft` wiring are covered by E2E.
- Wire `+page.svelte` to the exact DOM shape pinned in Architecture: render `text.slice(startCu,endCu)` with
  `clipSegmentsToWindow`, `translateX(startX)` on `.line-text`, row width = `lineWidthPx`, `data-slice-start={startCu}`
  on the row. Make the gutter sticky **only if M0 step 5 confirmed sticky pins inside the transformed container in both
  webviews**; otherwise keep today's scroll-away gutter (the slice mechanism works either way).
- Add horizontal keyboard navigation (design-principles "keyboard-first"): no h-scroll keys exist today
  (`viewer-keyboard.ts` Home/End are vertical), so a keyboard-only user couldn't reach clipped content once it's
  virtualized. Add ArrowLeft/ArrowRight (and a horizontal Home/End or Shift variant) that move `contentRef.scrollLeft`.
  If deferred, record it as a deliberate follow-up, not an omission.
- **Decide the bidi policy here** (forced-LTR override vs per-line RTL fallback) based on what M0/M2 showed, and wire
  it.
- Update `viewer-pointer.ts` to add `data-slice-start`; extend `viewer-pointer.test.ts` (TDD, real red→green: a click on
  a scrolled-right slice with non-zero `data-slice-start` resolves to the correct full-line offset, including a click at
  the slice's right edge, and with a `<mark>` span in the slice so the sibling-offset sum is exercised).
- **Wire horizontal scroll-to-match in `viewer-search.svelte.ts`** (C1): `scrollToMatch` sets `scrollLeft` from
  `hscroll.xOfCu(line, match.column)` in addition to `scrollTop`, no-opping when the line is uncached or unknown (per
  Architecture). This is more than a one-liner: extend `SearchDeps` with an `xOfCu` getter and thread it through the
  page's `createViewerSearch({...})` call. List that plumbing as an explicit step so it isn't discovered mid-build.
- Retire `runContentWidthEffect` and the `contentWidth` `$state`; route `scroll-spacer` `min-width` to the monotonic
  `contentWidthPx`. The `scroll.contentWidth = 0` reset in `toggleWordWrap` (`viewer-scroll.svelte.ts:403`) becomes
  unnecessary (the monotonic value plus the wrap-mode template guard `scroll.wordWrap ? 0 : …` handle it) — remove it
  rather than leave it dangling. Add `scrollLeft` to `handleScroll`.
- Add `viewer-hscroll.destroy()` (clears the `CellIndex` cache, unsubscribes the scale hook) and wire it in the page's
  `onDestroy` alongside `scroll.destroy()`. The `cacheGeneration` counter handles mid-session invalidation; teardown is
  `destroy()`'s job, not the counter's.
- Keep word-wrap mode untouched: when `wordWrap` is on, the hscroll layer is inert (no slicing, `overflow-x: hidden` as
  today).
- Confirm copy / select-all still return the **full** line (they read the selection model, not the DOM — verify the
  byte-estimator path in `selection.svelte.ts` and copy flow don't read rendered text).
- Checks: `pnpm check` (full), plus targeted Vitest for the new composable.

### M5 — E2E + docs + perf confirmation (~1 day)

Goal: prove it end-to-end and lock it against regression.

- Playwright spec(s) in `apps/desktop/test/e2e-playwright/` (see that dir's CLAUDE.md): open a long-line file; assert
  (1) only a bounded substring is in the DOM per row (query `.line-text` text length « full line), (2) horizontal scroll
  reveals later content, (3) click-to-caret at a scrolled-right position lands on the right offset, (4) search next/prev
  to a match that's initially off-screen **scrolls it into the horizontal viewport** and the `<mark>` is within the
  viewport's left/right bounds (not merely present in the DOM — that would pass while C1 is broken), (5) select-all +
  copy still yields the full line (selection model untouched), (6) a long bidi/RTL line behaves per the chosen policy
  (no wrong-offset clicks). Use `expect.poll(...).toBeTruthy()`, never bare `pollUntil` (the `bare-poll` rule).
- Re-run the M0 fixtures; record after-numbers in `docs/notes/viewer-long-line-bench.md`. Target: long-line open within
  a small constant of a normal-text open of the same line count.
- Docs: update `routes/viewer/CLAUDE.md` (architecture: add the no-wrap horizontal-virtualization paragraph + the
  must-knows: offsets stay full-line UTF-16, slice translate at render only, the `data-slice-start` caret-shift
  contract, cell model is monospace-exact/exotic-glyph-bounded, the cell-width and `content-visibility` gotchas). The
  viewer has **no `DETAILS.md` today** (it documents everything in `CLAUDE.md`); create `routes/viewer/DETAILS.md` per
  the repo's CLAUDE.md/DETAILS.md split contract and put the A/B/C decision + rationale there, linked to the committed
  bench note. Don't assume DETAILS.md exists.
- Checks: `pnpm check --include-slow`.

## What can run in parallel

Mostly sequential is fine (we're not in a hurry). The only safe parallelism: M1 (pure CSS baseline) and M2 (pure cell
model) are independent and could be done in either order or concurrently. M3 is independent of M2 (different file) but
both feed M4, so finish both before M4. M4 and M5 are strictly sequential.

## LoC estimate

Implementation ~360–520 LoC, tests ~470–650 LoC, total ~830–1170 LoC. Breakdown:

- `viewer-cell-model.ts`: ~150–230. The EAW width table is the bulk and is easy to under-budget; a correct compact table
  must cover Wide/Fullwidth ranges, combining marks, ZWJ, regional indicators, and variation selectors. Prefer pulling a
  vetted table (e.g. an `eastasianwidth`-style helper) over hand-rolling — check license with `cargo deny`'s JS
  equivalent / the `dependencies` rule before adding, and run `pnpm dedupe` after. Test ~150–250.
- `viewer-hscroll.svelte.ts`: ~160–230 · test ~90–130 (monotonic max + scale + slice math).
- `line-segments.ts` `clipSegmentsToWindow`: ~30–50 · test ~80.
- `viewer-pointer.ts` slice shift: ~10–20 · test ~70–90.
- `viewer-search.svelte.ts` horizontal scroll-to-match: ~15–30 · test ~30 (mostly E2E).
- `+page.svelte` render + CSS (sticky gutter, translate, content-visibility, bidi override): ~50–80.
- `viewer-scroll.svelte.ts` retire `runContentWidthEffect`, add `scrollLeft`: net ~0 (≈ −15 / +15).
- E2E spec: ~90–140.

## Risks and mitigations

- **`caretPositionFromPoint` on a translated, sliced node** (the load-bearing assumption for click-to-caret). Mitigated
  by M0 spike step 4, run on BOTH webviews, asserting correct offset at slice edges. If it fails, the approach is dead
  and we reconsider before building. Also covered by an M5 E2E assertion.
- **Exotic-glyph x error is bounded, not cumulative.** Because the scrollbar, `scrollLeft`, and each slice anchor are
  all cell-space and every slice re-anchors to `startCell * W`, a glyph whose real advance ≠ `n * W` only nudges text
  within the current window; the error never accumulates down the line. Generous horizontal buffer guarantees the
  viewport is always covered despite the nudge. Text and caret stay correct. If pixel-perfect proportional support ever
  matters, the per-glyph measured path (rejected approach A) is the escape hatch. M0 step 2 quantifies the bound and
  sizes the buffer.
- **Bidi breaks visual==logical.** Real (wrong clicks), not cosmetic. Handled by the M4 bidi policy (forced-LTR override
  or per-line fallback), tested in M2/M5.
- **Horizontal scroll-to-match.** `viewer-search.scrollToMatch` is vertical-only today; a long-line match would render
  off-screen. Fixed in M4 (set `scrollLeft` too), asserted by the M5 horizontal-bounds check.
- **Per-line cache memory.** Keyed by line number (not the line string), cleared via the `cacheGeneration` counter, and
  capped by total cached CU, so a binary-as-text file can't balloon memory (principle 5).
- **Screen-reader reads a sliced line.** With only a substring in `.line-text`, AT walking the rendered DOM reads
  partial text (the selection-announce path uses the full-line model, so copy/announce is unaffected). Decide in M4:
  acceptable for a long-line viewer, or expose the full line to AT (e.g. an `aria-label`/visually-hidden full text on
  the row). At minimum, record it as a conscious accessibility tradeoff, not an oversight.
- **Horizontal element-size cap (~2^25 px).** Handled by mirroring the existing vertical `MAX_SCROLL_HEIGHT` scaling;
  unit-tested in M4.
- **Sticky-gutter behavior change.** Line numbers staying put during horizontal scroll is a UX change (improvement).
  Confirm with David in review; trivial to drop if unwanted (gutter scrolls away as today, slice still works).
- **Tab columns across a slice boundary.** A tab's width depends on its absolute column, so the cell index must carry
  running columns (it does); slicing mid-tab-run must use the precomputed cell positions, not recompute from the slice
  start. Covered by an M2 test.
- **Pathological multi-MB single line prefix-sum cost.** O(CU) once, cached — tens of ms for ~10 MB. If ever a problem,
  a hard CU cap with a "line truncated" affordance is the backstop (a real compromise, only for the absurd case; not in
  scope now).

## Open questions for David

1. Sticky gutter during horizontal scroll: yes (recommended) or keep current scroll-away behavior?
2. Confirm the cell-model compromise (exotic-glyph x is approximate but bounded per-slice and never affects text or
   caret) is acceptable, given the viewer is monospace and the alternative (per-glyph measured) is much heavier and
   blocked by pretext's missing per-glyph-x API.
3. Bidi policy for long RTL/mixed lines: force logical LTR order (`unicode-bidi: bidi-override`, simplest, "lossy"
   consistent with the viewer) vs per-line fallback to the non-sliced `content-visibility` path (preserves visual bidi
   but two code paths)? Recommend forced-LTR unless real RTL log/code viewing matters to you.
