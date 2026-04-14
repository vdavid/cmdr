# Dialog number formatting and mid-path truncation

Two UX polish issues in file operation dialogs (Copy, Move, Delete, Trash):

1. **Raw numbers are hard to read** — file/dir counts like `194667` should display as `194,667`
2. **Dialog width jitters** — the current-file path changes every second, resizing the dialog. Should be fixed width
   with smart mid-path truncation

## Milestone 1: `formatNumber` in all file operation dialogs

### Intention

`formatNumber(n)` already exists in `selection-info-utils.ts` (uses `toLocaleString('en-US')` → comma-separated). The
file operation dialogs just don't use it. This is a straightforward import-and-apply.

### Changes

1. **TransferProgressDialog.svelte** — import `formatNumber`, apply to:
   - `filesDone` / `filesTotal` in progress detail (line 936: `{filesDone} / {filesTotal}`)
   - `scanFilesFound` / `scanDirsFound` in scan-wait stats (lines 766, 771)

2. **TransferDialog.svelte** — import `formatNumber`, apply to:
   - `filesFound` / `dirsFound` in scan stats (lines 457, 462)

3. **DeleteDialog.svelte** — import `formatNumber`, apply to:
   - `filesFound` / `dirsFound` in scan stats (lines 283, 288)
   - `overflowCount` in file list overflow (line 247)
   - `fileCount` in `formatItemSize` (line 188)

4. **Style guide** (`docs/style-guide.md`) — add a rule under "Punctuation, capitalization, numbers":
   - **Format large numbers with thousands separators.** Use `formatNumber()` for all user-facing counts (file counts,
     dir counts, item counts). Byte values use `formatBytes()` / `formatFileSize()` which already handle this. (Don't
     reference the file path — code search finds the function. Style guide shouldn't be coupled to file locations.)

5. **TransferErrorDialog.svelte** — check if the error summary shows file counts. If so, apply `formatNumber` there too.
   Also check the completion toast messages for raw numbers.

### Testing

No new tests needed — `formatNumber` is already tested in `selection-info-utils.test.ts`. The changes are purely
template-level (swapping `{count}` → `{formatNumber(count)}`).

### Checks

`./scripts/check.sh --svelte` (covers Svelte compilation, linting, formatting).

## Milestone 2: `shortenMiddle` utility with pretext

### Intention

Create a reusable text truncation utility that uses `@chenglou/pretext` for pixel-accurate font-aware measurement. This
replaces the DOM-based approach in `SelectionInfo.svelte` (throwaway `<span>` + `offsetWidth`) with a canvas-based
approach (no reflow). The utility should work for two use cases:

- **File paths** (progress dialog): prefer breaking at `/` so the user sees root context + filename
- **Filenames** (file list views): plain mid-split preserving the file extension

### API

```ts
// Core — injectable measurement, fully testable without canvas
function shortenMiddle(
  text: string,
  maxWidthPx: number,
  measureWidth: (text: string) => number,
  options?: {
    preferBreakAt?: string // '/' for paths — snap truncation to nearest break char
    startRatio?: number // 0-1, how much width budget goes to the start. Default: 0.5
    ellipsis?: string // Default: '…'
  },
): string

// Factory — pretext-backed, for production use
function createPretextMeasure(font: string): (text: string) => number
```

The core function takes an injectable `measureWidth` so it's testable with a mock (no canvas needed). The factory wires
it to pretext for production use.

### Location

`apps/desktop/src/lib/utils/shorten-middle.ts` — pure utility, no Svelte/DOM deps.

### Algorithm

1. `measureWidth(text)`. If ≤ maxWidth, return as-is.
2. `measureWidth(ellipsis)` → ellipsis width. If ellipsis alone exceeds maxWidth, return ellipsis anyway (container is
   too narrow for anything meaningful).
3. Split remaining budget: `startBudget = (maxWidth - ellipsisWidth) * startRatio`,
   `endBudget = (maxWidth - ellipsisWidth) * (1 - startRatio)`.
4. **Find start cut**: binary search over character indices — find the longest prefix where
   `measureWidth(prefix) ≤ startBudget`.
5. **Find end cut**: binary search over character indices from the end — find the longest suffix where
   `measureWidth(suffix) ≤ endBudget`.
6. **Snap to break char** (if `preferBreakAt` is set): look for the nearest `preferBreakAt` character within the start
   prefix (snap inward = left, so the result doesn't exceed the budget). Same for end suffix (snap inward = right). Only
   snap if the snapped version still uses at least 40% of its budget — this threshold is a starting point, to be tuned
   during visual testing.
7. Return `startPart + ellipsis + endPart`.

**Performance note**: The binary search calls `measureWidth()` O(log n) times. For the progress dialog (one path at a
time, ~60 chars), this is negligible. For future file-list use (hundreds of entries), the factory can be swapped for a
raw `canvas.measureText()` wrapper if needed.

### `createPretextMeasure` implementation

`measureNaturalWidth` was added in `@chenglou/pretext@0.0.5` (not in our current 0.0.3). Update to 0.0.5 as part of this
work. The factory becomes:

```ts
function createPretextMeasure(font: string): (text: string) => number {
  return (text: string) => measureNaturalWidth(prepareWithSegments(text, font))
}
```

Pretext caches segment measurements by (segment, font), so repeated calls with overlapping substrings are efficient.

### Why pretext, not raw canvas

- **Caching**: pretext caches segment measurements by (text, font). For file lists with hundreds of entries sharing the
  same font, repeated measurements are near-free.
- **Correctness**: handles emoji width correction (macOS Chrome/Firefox inflate emoji widths), CJK, grapheme clusters.
- **Consistency**: we already use pretext in the viewer for line height measurement. Using the same measurement engine
  everywhere avoids subtle discrepancies.
- **No DOM reflow**: canvas-based measurement is synchronous and doesn't trigger layout recalculation.

### Why not use pretext's layout/line-breaking API directly

Pretext's `layout()` / `layoutWithLines()` implement CSS word-wrap semantics (break at whitespace, overflow-wrap). File
paths and filenames have no whitespace — they're single long segments. Using `layout()` and checking `lineCount === 1`
would work as a "does it fit?" check, but the binary search for truncation points needs character-level granularity that
pretext's line-breaking API doesn't expose. So we use pretext for measurement (`prepareWithSegments` +
`measureNaturalWidth`) and implement the truncation logic ourselves.

### Test cases (TDD — write these first)

File: `apps/desktop/src/lib/utils/shorten-middle.test.ts`

**Canvas in jsdom**: jsdom doesn't provide a real canvas implementation — `OffscreenCanvas` and `<canvas>.getContext()`
return null. Pretext's `getMeasureContext()` will throw. Two strategies:

1. **Inject a `measureWidth` function** instead of hardcoding the pretext dependency. The `shortenMiddle` function
   accepts a `measureWidth: (text: string) => number` parameter. In production, this is wired to pretext via
   `createPretextMeasure(font)`. In tests, it's a simple mock (for example, `text.length * 8` for uniform 8px-per-char).
   This makes the truncation logic fully testable without canvas.

This separation also cleanly solves the performance concern — callers that want raw canvas measurement for hot paths can
inject their own `measureWidth` without going through pretext. See the API section above for the exact signatures.

**Test cases** (TDD — write these first):

```
Group: "returns text unchanged when it fits"
  - Short text that fits within maxWidth → returned as-is
  - Empty string → returned as-is
  - Text exactly at maxWidth → returned as-is

Group: "truncates in the middle for plain text"
  - Long text exceeding maxWidth → start + '…' + end
  - Result starts with the beginning of the original text
  - Result ends with the end of the original text
  - Measured width of result ≤ maxWidth (using mock measureWidth)

Group: "respects startRatio"
  - startRatio: 0.7 → more characters from start than end
  - startRatio: 0.3 → more characters from end than start

Group: "snaps to preferBreakAt character"
  - Path '/aaa/bbb/ccc/ddd/eee/fff.txt' with preferBreakAt: '/'
    → cuts at '/' boundaries, not mid-segment
  - Text with no matching break char → degrades to plain mid-split
  - Path where snapping would waste too much budget → falls back to raw cut position

Group: "handles edge cases"
  - Text shorter than ellipsis → returned as-is
  - Single character → returned as-is
  - All same character → clean mid-split
  - Ellipsis character itself in input → still works
```

**Mock `measureWidth` for tests**: `(text: string) => text.length * 8` — treats every character as 8px wide. This makes
tests deterministic and fast while exercising all the truncation logic (binary search, snapping, ratio). The pretext
integration is tested via the factory function in a separate test that can be skipped if canvas is unavailable.

### Checks

`cd apps/desktop && pnpm vitest run -t "shortenMiddle"` for unit tests, then `./scripts/check.sh --svelte`.

## Milestone 3: Svelte action for auto-truncation

### Intention

Wrap `shortenMiddle` in a Svelte action that handles measuring the container width, reading the computed font, and
re-truncating on resize. This makes it a drop-in replacement for the manual DOM measurement code in SelectionInfo.svelte
and easy to add to new components.

### API

```ts
// Svelte action — auto-reads font from element, observes width
function useShortenMiddle(
  node: HTMLElement,
  params: {
    text: string
    preferBreakAt?: string
    startRatio?: number
  },
): ActionReturn<typeof params>
```

### Location

`apps/desktop/src/lib/utils/shorten-middle-action.ts` — Svelte action, depends on `shorten-middle.ts`.

### Behavior

1. On mount: read font from element's computed style. **Gotcha**: `getComputedStyle(node).font` shorthand can return
   empty string in some browsers. Fall back to constructing the font string from individual properties (`fontSize`,
   `fontFamily`, `fontWeight`). Read `node.clientWidth` for available width.
2. Create `measureWidth` via `createPretextMeasure(font)`.
3. Call `shortenMiddle(params.text, width, measureWidth, options)`
4. Set `node.textContent = result`
5. Set `node.title = params.text` (full text on hover)
6. Set up `ResizeObserver` — on width change, re-run step 3-4
7. On `update` (params change): **compare `params.text` to previous value** to avoid unnecessary re-truncation (Svelte 5
   creates a new params object on every render, so the `update` function fires even when nothing changed). If text
   changed, re-run step 3-4.
8. On `destroy`: disconnect observer

### Why a separate action file

The pure function (`shorten-middle.ts`) stays testable without DOM/Svelte deps. The action is the thin bridge. This
matches the project's pattern of extracting pure logic from Svelte components.

### Testing

Action behavior is integration-level — verified manually + E2E. The pure function carries the unit tests.

## Milestone 4: Apply to file operation dialogs

### Changes

1. **TransferProgressDialog.svelte** — replace the current `current-file` div:

   ```svelte
   <!-- Before -->
   <div class="current-file" use:tooltip={{ text: currentFile, overflowOnly: true }}>
       {currentFile}
   </div>

   <!-- After -->
   <div class="current-file" use:useShortenMiddle={{ text: currentFile, preferBreakAt: '/' }}>
   </div>
   ```

   Remove the `text-overflow: ellipsis` CSS since the action handles truncation. The action sets `title` automatically
   (replaces the tooltip for this element).

2. **Dialog width** — change `containerStyle` from `min-width: 420px; max-width: 500px` to `width: 500px` on
   TransferProgressDialog. This prevents the dialog from resizing based on content. Same for DeleteDialog and
   TransferDialog (they already use this pattern, but verify they don't jitter).

3. **Consider**: refactoring `SelectionInfo.svelte`'s existing DOM-based truncation to use the new `useShortenMiddle`
   action. This is a natural follow-up but can be a separate PR to limit blast radius.

### Testing

Manual testing with the running app — trigger a delete/copy of a large directory and observe:

- Numbers show thousands separators
- Dialog width stays fixed
- Current file path truncates with mid-path ellipsis, showing meaningful start + filename
- Tooltip on hover shows full path

### Checks

`./scripts/check.sh --svelte` and `./scripts/check.sh --check stylelint`.

## Execution order

Sequential is fine — each milestone builds on the previous:

1. Milestone 1 (formatNumber) — quick, self-contained
2. Milestone 2 (shortenMiddle pure function + tests) — TDD, core logic
3. Milestone 3 (Svelte action) — thin wrapper
4. Milestone 4 (apply to dialogs) — integration

## Open questions

- **SelectionInfo refactor**: Should we also migrate SelectionInfo.svelte's DOM-based truncation to use the new
  `shortenMiddle` utility in this PR, or defer to a follow-up? Recommend: defer (separate concern, separate PR).
- **40% snap threshold**: The break-char snap threshold (40% minimum budget usage) is a starting point. Will need visual
  testing with real paths to tune — too aggressive wastes space, too conservative never snaps.
