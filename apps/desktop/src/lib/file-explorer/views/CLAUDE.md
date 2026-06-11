# File explorer views

Virtual-scrolling file list components for rendering 100k+ file directories without DOM performance issues.

## Module map

- `BriefList.svelte` / `FullList.svelte`: the two virtual-scroll views (horizontal columns / vertical rows).
- `virtual-scroll.ts`: pure window math (uniform for Full, variable + prefix-sum for Brief).
- `file-list-utils.ts`, `brief-list-utils.ts`, `full-list-utils.ts`: shared + mode-specific rendering helpers.
- `measure-column-widths.ts`: pixel-accurate Ext / Size / Modified widths via `@chenglou/pretext` (no DOM reflow).

## Must-knows

- **Data lives in Rust `LISTING_CACHE`, never in Svelte `$state`.** The frontend fetches visible ranges on demand via
  `getFileRange(listingId, start, count, includeHidden)`. Loading 20k+ entries into reactivity causes 9+ second freezes
  (Svelte tracks the full array even with virtual scrolling). Keep entries out of reactivity; only the visible window
  enters it.
- **`$state()` cannot live in `.ts` files.** `virtual-scroll.ts` is pure functions returning plain objects; reactive
  state stays in `.svelte` / `.svelte.ts`.
- **Scroll position via `transform: translateY`, never absolute positioning** (absolute forces full layout recalc;
  transform uses the GPU compositor for 60fps).
- **`hasParent = true` makes UI indices 1-based**: index 0 is the `..` entry (not in backend cache). Real files start at
  1, so `cache_index = ui_index - 1`. Forgetting it lands the cursor one row off.
- **Don't reintroduce a `scrollTop - headerHeight` shift with a `Math.max(0, …)` clamp** in `FullList`. The sticky
  header lives inside the scroll container, so `scrollTop` and the spacer offset are the same number. A clamp collapses
  `scrollTop ∈ [0, headerHeight]` to one state and hides row 0 (including the `..` cursor) under the header. Pinned by
  `test/e2e-playwright/full-cursor-page-nav.spec.ts`.
- **`getDirSizeDisplayState()` (in `full-list-utils.ts`) is the single source of truth for a directory's size-column
  state.** Both `FullList.svelte`'s size cell and `measure-column-widths.ts` consume it; don't re-inline the
  dir/scanning/stale decision in either, or the rendered text and pre-measured width drift.
- **The Size and Modified columns render with `font-variant-numeric: tabular-nums`** (equal-width digits, so dates and
  right-aligned sizes line up into columns without a monospace font). Canvas/pretext can't measure that feature, so
  `measure-column-widths.ts` models it by substituting every digit with the widest one (`tabularize`) before measuring.
  Keep the CSS and the measurer in sync: if you drop tabular figures from a numeric column, drop the `tabularize` call
  for it too, or the column over-reserves width.
- **Paired-constant gotcha in `measure-column-widths.ts`**: `HEADER_CHROME_ACTIVE/INACTIVE` mirror `SortableHeader`'s
  gap + caret (12px active / 0 inactive). Change the CSS and you must change the constant, or header column widths drift
  (pretext measures without a reference element, so nothing is derived from the live DOM).
- **Index-size refresh (`refresh_listing_index_sizes`) refetches column widths through the existing `cacheGeneration`
  reset path, not a separate trigger.** Adding one double-fetches.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
