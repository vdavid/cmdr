# File explorer views

Virtual-scrolling file list components rendering 100k+ directories without DOM performance issues.

`BriefList.svelte` / `FullList.svelte` are the two views (horizontal columns / vertical rows), with `FullListHeader`,
`full-list-cache.svelte.ts`, `full-list-git-column.svelte.ts`, and `full-list-mouse.ts` beside Full, and
`brief-column-widths.svelte.ts` beside Brief. `virtual-scroll.ts` is pure window math, `measure-column-widths.ts` is
pixel-accurate width measurement via `@chenglou/pretext`, and the `*-utils.ts` trio holds the rendering helpers.

## Must-knows

- **Data lives in Rust `LISTING_CACHE`, ❌ never in Svelte `$state`.** The frontend fetches visible ranges on demand
  (`getFileRange`); only the visible window enters reactivity. Loading 20k+ entries in causes 9+ second freezes.
- **`$state()` cannot live in `.ts` files**: `virtual-scroll.ts` is pure functions; reactive state stays in `.svelte` /
  `.svelte.ts`.
- **Scroll position via `transform: translateY`, ❌ never absolute positioning** (which forces a full layout recalc).
- **`hasParent = true` makes UI indices 1-based**: index 0 is the `..` entry (not in backend cache), so
  `cache_index = ui_index - 1`. Forgetting it lands the cursor one row off.
- **`FullList`'s cache deps are one getter per prop, ❌ not one bag.** A bag read whole subscribes every host `$effect`
  to every prop, refetching the `..`-row stats on each `directory-diff` tick.
- **Row chrome shared by both views lives in `src/app-file-list.css`**, and every selector there keeps a
  `.full-list-container` / `.brief-list-container` prefix, or it loses specificity ties to
  `:global(.file-entry.folder-drop-target)`.
- **`FullList`'s column header sits OUTSIDE the scroll container** and pays the scrollbar's measured width back as right
  padding (`--spacing-scrollbar-width`). ❌ Don't move it back inside for free width sharing (that's the bug), and ❌
  don't reintroduce a header-height shift between `scrollTop` and the spacer offset: the clamp hides row 0 (the `..`
  cursor). Pinned by `FullListHeader.test.ts` + `test/e2e-playwright/full-cursor-page-nav.spec.ts`.
- **`getDirSizeDisplayState()` (`full-list-utils.ts`) is the single source of truth for a directory's size-column
  state**, consumed by both `FullList.svelte`'s size cell and `measure-column-widths.ts`; re-inline that decision in
  either and the rendered text and pre-measured width drift. The hourglass on top is PER ROW: the measurer takes the
  pane's own `isSizeUpdating(entry)`, ❌ never a per-volume flag, which clips the glyph on exactly the rows showing it.
- **Size and Modified render with `font-variant-numeric: tabular-nums`, which canvas/pretext can't measure.**
  `measure-column-widths.ts` substitutes the widest digit (`tabularize`), so the two move together: drop tabular figures
  from a numeric column and drop its `tabularize` call too, or it over-reserves.
- **Paired-constant gotcha in `measure-column-widths.ts`**: `HEADER_CHROME_ACTIVE/INACTIVE` mirror `SortableHeader`'s
  gap + caret. Change that CSS and change the constant too, or header column widths drift (pretext measures with no
  reference element, so nothing derives from the live DOM).
- **Nothing visible in Brief mode may wait on the width IPC** (how the cursor went invisible in prod). ❌ Don't gate
  `is-under-cursor` on widths, fall back to `capPx` (use `provisionalColumnWidth`), infer readiness from
  `rawWidths.length`, make `capPx` a fetch trigger, or bail silently.
- **Index-size refresh (`refresh_listing_index_sizes`) refetches column widths through the existing `cacheGeneration`
  reset path**; a separate trigger double-fetches.
- **A unit test asserting on ROWS must mount through `mountFullList()` (`test-full-list.ts`).** Without a measured
  surface the virtual window is zero rows wide and the list renders nothing, silently, so every negative assertion
  passes for free. Assert the rows are on screen before asserting what isn't on them.
- **`listing.showExtensionInName` must stay in lockstep across the renderer and the measurer**: on, `gridTemplate` drops
  the Ext track and `computeFullListColumnWidths` returns `ext: 0`, so changing one side drifts every column. ❌ Don't
  "clean up" `FullListHeader`'s `.header-name-ext` split: it's the only way left to CLICK sort-by-extension in that
  mode.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
