# Accurate word-wrap scroll via pretext

## Why

When word wrap is on in the file viewer, the virtual scroll assumes every line has the same height
(`avgWrappedLineHeight`). In reality, a 200-char line wraps to 3 visual rows (54px) while a 10-char line stays at 1
(18px). The averaged height causes:

- **Scroll thumb inaccuracy** — the scrollbar position doesn't reflect the true file position
- **Jump-to-line drift** — cumulative height error compounds over thousands of lines
- **Scrollbar size is wrong** — total scroll height is `totalLines * avgHeight`, but the real total is different

This is a known trade-off documented in the CLAUDE.md: "slightly inaccurate (scroll thumb position drifts) but keeps the
O(1) virtual scroll contract." Pretext lets us remove that trade-off for FullLoad files (<1MB) without breaking the O(1)
contract.

## What we're building

1. Integrate the `pretext` library for per-line height calculation when word wrap is on
2. Replace uniform-height virtual scroll with variable-height virtual scroll for FullLoad files
3. Keep the current averaged approach as fallback for ByteSeek/LineIndex files and as initial state before pretext
   finishes preparing

## What we're NOT building

**Variable-height scroll for large files (>1MB).** ByteSeek/LineIndex backends don't have all line text on the frontend.
We'd need to `prepare()` lines incrementally as they arrive, maintain partial height maps, and handle gaps. The
complexity isn't worth the marginal improvement — for huge files, the averaged approach is fine because users scroll
proportionally and rarely notice drift.

**Web worker for preparation.** The `prepare()` call for <1MB files should take ~200ms or less. If benchmarks show this
is fine in `requestIdleCallback`, a worker adds complexity for no gain. We'll note the option in case benchmarks
surprise us, but won't build it preemptively.

## Design decisions

### Progressive enhancement, not blocking

**Why:** Design principle says "blocking the UI or other actions is an absolute no-go." Pretext's `prepare()` is the
slow step (~19ms per 500 texts). We must never delay the initial render waiting for it. The approach:

1. Render immediately with uniform 18px line height (same as today — zero regression)
2. Run `prepare()` in a `requestIdleCallback` after first render
3. When done, `layout()` all lines (fast — ~1ms for 10k lines), build height map
4. Apply the height map in a single frame with scroll compensation

Users open a file and start reading from the top. Top lines are typically short (headers, imports). By the time they
scroll to long-wrapping lines deeper in the file, preparation has finished and exact heights are available.

### Cancellation on word wrap toggle

**Why:** The user can toggle word wrap off while `prepare()` is still running. Without cancellation, we'd waste CPU
finishing preparation for a mode that's no longer active, and risk applying a stale height map.

Use a generation counter: increment on each toggle-on, pass to the async preparation. When preparation finishes, check
if the generation still matches. If the user toggled off (or off-then-on), discard the stale result. The new toggle-on
starts a fresh preparation. This also handles rapid toggling — only the most recent toggle-on's preparation survives.

### Prefix-sum array for O(log n) lookups

**Why:** Variable-height virtual scroll needs two operations: "what line is at scroll position Y?" and "what's the
scroll position of line N?" A prefix-sum array (`cumHeight[i]` = sum of heights 0..i) gives O(1) for line→position and
O(log n) via binary search for position→line. This keeps scroll handling fast on every frame.

### Font string must match CSS exactly

**Why:** Pretext measures text using canvas with a font string. The viewer renders with CSS
`font-family: var(--font-mono)` which resolves to
`ui-monospace, 'SF Mono', SFMono-Regular, Menlo, Monaco, Consolas, monospace` at `12px` with `line-height: 1.5`. If the
canvas font doesn't resolve to the same font the browser picks for rendering, all heights will be wrong. We need to:

- Use the exact same font string as the CSS
- Validate at runtime that canvas and DOM produce the same width for a test string
- Fall back to averaged heights if they diverge (defensive, shouldn't happen in practice)

### Available width must account for the gutter

**Why:** The wrapping width is NOT the container width. It's the container width minus the line-number gutter (dynamic
width based on `String(totalLines).length` in `ch` units + `padding-right: --spacing-sm` + `margin-right: --spacing-sm`

- 1px border) minus the `.line` horizontal padding (2x `--spacing-sm`). The gutter width can change when `totalLines`
  updates (for example, a file near the boundary where line count jumps from 999 to 1000 changes gutter from 3ch to
  4ch).

Approach: measure the actual available text width from the DOM. After mount, read the width of a `.line-text` element
(or a probe element styled identically). Pass that to `layout()` as `maxWidth`. When it changes (resize, gutter width
change), trigger a reflow.

### Pretext wrapping vs CSS wrapping: known gap

**Why:** Pretext implements its own line-breaking algorithm (greedy fit with Unicode-aware segmentation). CSS uses
`pre-wrap` + `overflow-wrap: break-word`, which has its own rules for tab stops, break opportunities, and edge cases.
These will agree for the vast majority of content (plain text, code) but may disagree on:

- Tab characters (`\t`): CSS renders at 8ch tab stops by default; pretext handles tabs but may compute different stops
- Edge cases in break-word: CSS allows mid-word breaks as a last resort; pretext's segmentation may split differently

This is acceptable. The height map is an approximation that's far better than uniform averaging. A 1-2px error on a
single line is invisible to the user compared to the current drift of hundreds of pixels over thousands of lines. If we
find specific content where the mismatch is noticeable, we can adjust later.

### Reflow on resize uses cached `prepare()` data

**Why:** `prepare()` is the expensive step and only depends on text + font. `layout()` only depends on `maxWidth` and
`lineHeight`. When the viewport width changes (window resize, pane resize), we call `layout()` again with the new width
— no re-preparation needed. At ~0.09ms per line, reflowing 10k lines takes ~1ms. This means resize feels instant.

### Scope: FullLoad backend only

**Why:** FullLoad files are <1MB, meaning at most ~12k lines with typical code. All text is already on the frontend (the
initial `LineChunk` contains everything). This is the clean case: all data available, bounded cost, biggest user impact
(small-to-medium files are where people notice scroll drift most because they scroll through the whole file).

ByteSeek/LineIndex files continue using the existing averaged approach unchanged.

**Line count guard:** A 1MB file of single-byte lines could theoretically have ~1M lines. At ~0.09ms per `layout()`
call, that's ~90ms for reflow — noticeable during resize. Add a guard: if `totalLines > 50_000`, skip the height map and
fall back to averaged heights. This is conservative; we can raise the limit after benchmarking.

## Implementation

### Step 1: Add pretext dependency and create height map module

**Files:**

- `apps/desktop/package.json` — add `pretext` dependency
- `apps/desktop/src/routes/viewer/viewer-line-heights.svelte.ts` — new module

The height map module encapsulates all pretext interaction:

```
createLineHeightMap(lines: string[], font: string, lineHeight: number)
  → prepare(text, font) for each line — this is the slow step, measures text segments via canvas
  → layout(prepared, maxWidth, lineHeight) for each line — returns { lineCount, height } via pure arithmetic
  → build prefix-sum array from the per-line heights
  → expose: getLineTop(n), getLineAtPosition(scrollY), getTotalHeight(), reflow(newWidth)
```

**Pretext API model:** `prepare()` is called once per line (or per text segment). It's the expensive step that does
canvas measurement and caches results. `layout()` is called once per prepared line with a `maxWidth` and returns the
wrapped height. `layout()` uses only cached arithmetic — no canvas — so it's ~0.09ms per line. For a 10k-line file,
expect ~200–400ms total for all `prepare()` calls (the dominant cost), then ~1ms for all `layout()` calls.

Key behaviors:

- `reflow(newWidth)` calls `layout()` on all prepared lines with the new width, rebuilds prefix sum. Fast (~1ms) because
  `prepare()` data is already cached.
- All preparation happens asynchronously. The module exposes a `ready` state.
- If preparation fails or times out, `ready` stays false — the caller falls back to uniform heights.

### Step 2: Integrate into viewer-scroll.svelte.ts

**File:** `apps/desktop/src/routes/viewer/viewer-scroll.svelte.ts`

Changes:

- Accept a new dep: `getAllLines: () => string[] | null` — returns all cached lines for FullLoad, null otherwise
- Accept a new dep: `getTextWidth: () => number` — the available text width (container minus gutter minus padding)
- When `wordWrap` is toggled on AND `getAllLines()` returns non-null, create the height map with current generation
- When word wrap is toggled off, increment generation (discards in-flight preparation)
- When the height map becomes `ready`:
  - Replace `visibleFrom` calculation: `heightMap.getLineAtPosition(scrollTop)` (binary search in prefix sum) instead of
    `Math.floor(scrollTop / scrollLineHeight)`
  - Replace `visibleTo` calculation: `heightMap.getLineAtPosition(scrollTop + viewportHeight)` (second binary search)
    instead of `Math.ceil((scrollTop + viewportHeight) / scrollLineHeight)`. Both still add `BUFFER_LINES`.
  - Replace spacer height: `heightMap.getTotalHeight()` instead of `totalLines * scrollLineHeight`
  - Replace line container translateY: `heightMap.getLineTop(visibleFrom)` instead of `visibleFrom * scrollLineHeight`
  - Apply scroll compensation: read the first visible `.line` element's `data-line` attribute from the DOM to find which
    line the user is actually looking at (don't trust the uniform model's math — it's the thing that's wrong), then set
    `scrollTop` to `heightMap.getLineTop(thatLine)`
- When text width changes and height map exists, call `heightMap.reflow(newWidth)`. This needs careful handling to avoid
  a feedback loop — `reflow` changes heights, which changes the spacer, which can trigger a browser scroll event, which
  re-derives `visibleFrom`. The fix: in the same synchronous call that does the reflow, also compute and set the
  compensated `scrollTop` (find which line is at top via DOM `data-line`, look up in new prefix sum). This way, when the
  browser fires the scroll event, the `scrollTop` is already correct and `visibleFrom` derives to the right line. No
  intermediate frame with wrong values.
- `scrollByLines` / `scrollByPages` / `scrollToEnd` need to use height map positions instead of uniform arithmetic
- `runWrappedLineHeightEffect` is no longer needed when the height map is ready (it still runs as fallback when the
  height map isn't available — ByteSeek/LineIndex files, or before prepare() finishes)
- `runScrollCompensationEffect` needs a branch: if height map is ready, compensate by line position lookup; if not, use
  the existing ratio approach
- Expose a `getLineTop(n): number` function that returns `heightMap.getLineTop(n)` when ready, or `n * scrollLineHeight`
  as fallback. The search composable needs this for navigating to match positions (see below).

**Search navigation:** The search composable (`viewer-search.svelte.ts`) scrolls to match positions via
`getScrollLineHeight`. With variable heights, scrolling to line N needs `getLineTop(N)`, not `N * scrollLineHeight`. Add
a `getLineTop` dep to the search composable's `ScrollDeps` that delegates to the height map when available.

**Intention:** The height map is an overlay on the existing scroll system. When it's ready, it provides better numbers.
When it's not, everything works exactly as before. This means zero regression risk for large files and zero regression
during the preparation window for small files.

### Step 3: Wire up in +page.svelte

**File:** `apps/desktop/src/routes/viewer/+page.svelte`

Changes:

- The spacer div height expression changes to use the height map's total when available
- The `translateY` for `.lines-container` uses the height map's line-top lookup when available
- Add a `$effect` to trigger reflow when `.file-content` width changes (via ResizeObserver or `bind:clientWidth`)
- The FullLoad backend's initial lines are all of them (the entire file). After `openViewerSession`, if the backend is
  FullLoad, kick off the height map preparation

**Intention:** The page component is the orchestrator. It knows the backend type, has the line cache, and owns the DOM
refs. It decides when to create the height map and provides width changes to trigger reflow.

### Step 4: Font string resolution

**File:** `apps/desktop/src/routes/viewer/viewer-line-heights.svelte.ts` (part of step 1, but deserves its own callout)

The viewer's CSS uses `var(--font-mono)` at `var(--font-size-sm)` (12px) with `line-height: 1.5` (18px). For pretext, we
need the resolved font string. Approach:

1. Create a hidden element with the viewer's font styles
2. Read `getComputedStyle(el).font` to get the resolved font string
3. Pass that to `prepare()`
4. For `layout()`, use `lineHeight = 18` (the CSS line-height: 1.5 \* 12px)

Validation: measure a test string (e.g. "ABCDabcd1234") via both canvas `measureText` and a DOM element's `scrollWidth`.
If they differ by more than 1px, log a warning and skip the height map (fall back to averaged heights).

### Step 5: Testing

**Unit tests** for the height map module:

- Prefix-sum correctness: `getLineTop(0) === 0`, `getLineTop(n) === sum of heights 0..n-1`
- `getLineAtPosition` is inverse of `getLineTop` (within rounding)
- `reflow` with different widths produces different heights for long lines, same for short lines
- Empty input, single line, all-same-length lines (should degenerate to uniform)
- Generation counter: stale preparations are discarded

**Font validation test** (integration, runs in Tauri WebView):

- Create the hidden probe element, read computed font
- Verify canvas measureText and DOM scrollWidth agree within 1px for a test string
- Verify the fallback path activates if we deliberately pass a wrong font

**Integration tests** — these are best tested manually and via E2E:

- Open a <1MB file with mix of short and long lines, toggle word wrap
- Scroll to bottom — verify the scrollbar thumb is in the right place
- Jump to a specific line — verify it's actually visible
- Resize the window while word wrap is on — verify no flicker or jump
- Toggle word wrap off and on rapidly — verify no crash or stale state
- Search for a term on a long-wrapping line, navigate to it — verify the match is visible

**E2E test** (Playwright/Tauri):

- Open a test file with known content (some short lines, some very long lines)
- Toggle word wrap on
- Wait for height map to be ready (could expose a status in the DOM for testing, or a debug log)
- Scroll to bottom, verify last line is visible
- Scroll to a specific position, verify approximate correctness

### Step 6: Update CLAUDE.md docs

**Files:**

- `apps/desktop/src/lib/file-viewer/CLAUDE.md` — update the word-wrap decision to mention pretext for FullLoad, keep
  averaged note for ByteSeek/LineIndex
- `apps/desktop/src/routes/viewer/CLAUDE.md` — add `viewer-line-heights.svelte.ts` to the files table, note the
  progressive enhancement pattern

## Risks and mitigations

### Font mismatch

**Risk:** Canvas measures a different font than CSS renders, producing wrong heights. **Mitigation:** Runtime validation
(step 4). If mismatch detected, fall back to averaged heights. Log a warning so we can investigate.

### prepare() too slow for edge cases

**Risk:** A file with extremely long lines or complex Unicode could make `prepare()` take >500ms. **Mitigation:** Wrap
in `requestIdleCallback` so it doesn't block interaction. Consider a timeout (say 2 seconds) — if `prepare()` hasn't
finished, abandon it and stay on averaged heights. The user experiences zero degradation because the averaged path is
the same as today.

### Scroll jumps when height map activates

**Risk:** User has scrolled during the ~200ms preparation window. When the height map activates, the computed position
for their current `scrollTop` could point to a different line under the new height model. **Mitigation:** On activation,
read the first visible `.line` element's `data-line` attribute from the actual DOM to find which line the user is
looking at (don't trust the uniform model — it's the thing that's wrong). Then set `scrollTop` to
`heightMap.getLineTop(thatLine)`. Since preparation finishes quickly and users are usually near the top of the file they
just opened, the correction is typically zero or a few pixels.

### MAX_SCROLL_HEIGHT interaction

**Risk:** The current code caps scroll height at 30M pixels. With variable heights, a file with many wrapping lines
could exceed this. **Mitigation:** Same scaling approach as today.
`scrollScale = min(1, MAX_SCROLL_HEIGHT / totalHeight)`. All height map lookups multiply by `scrollScale`. The
prefix-sum stores unscaled values; scaling is applied at the scroll layer.
