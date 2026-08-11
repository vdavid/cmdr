# File explorer views

Virtual-scrolling file list components for rendering 100k+ file directories without DOM performance issues.

## Module map

- `BriefList.svelte` / `FullList.svelte`: the two virtual-scroll views (horizontal columns / vertical rows).
- `FullListHeader.svelte`, `full-list-cache.svelte.ts` (prefetch buffer + refresh policy),
  `full-list-git-column.svelte.ts`, `full-list-mouse.ts` (mousedown plan + drag payload): `FullList`'s siblings.
- `brief-column-widths.svelte.ts`: Brief's raw backend text widths + the fetch/retry/staleness policy.
- `virtual-scroll.ts`: pure window math (uniform for Full, variable + prefix-sum for Brief).
- `file-list-utils.ts`, `brief-list-utils.ts`, `full-list-utils.ts`: shared/mode-specific rendering helpers.
- `measure-column-widths.ts`: pixel-accurate Ext/Size/Modified widths via `@chenglou/pretext` (no DOM reflow).

## Must-knows

- **Data lives in Rust `LISTING_CACHE`, never in Svelte `$state`.** The frontend fetches visible ranges on demand via
  `getFileRange(listingId, start, count, includeHidden)`; only the visible window enters reactivity. Loading 20k+
  entries into it causes 9+ second freezes (Svelte tracks the full array even with virtual scrolling).
- **`$state()` cannot live in `.ts` files.** `virtual-scroll.ts` is pure functions returning plain objects; reactive
  state stays in `.svelte` / `.svelte.ts`.
- **Scroll position via `transform: translateY`, never absolute positioning** (absolute forces a full layout recalc;
  transform composites on the GPU at 60fps).
- **`hasParent = true` makes UI indices 1-based**: index 0 is the `..` entry (not in backend cache). Real files start at
  1, so `cache_index = ui_index - 1`. Forgetting it lands the cursor one row off.
- **`FullList`'s cache deps are one getter per prop, not one bag.** A bag read whole subscribes every host `$effect` to
  every prop: the `..`-row stats refetch on each `directory-diff` tick. See `DETAILS.md` § FullList's siblings.
- **Row chrome shared by both views lives in `src/app-file-list.css`.** Every selector there keeps a
  `.full-list-container` / `.brief-list-container` prefix, or it loses specificity ties to
  `:global(.file-entry.folder-drop-target)`.
- **Don't reintroduce a `scrollTop - headerHeight` shift with a `Math.max(0, …)` clamp** in `FullList`: the sticky
  header lives inside the scroll container, so `scrollTop` and the spacer offset are the same number, and a clamp hides
  row 0 (the `..` cursor) under the header. Pinned by `test/e2e-playwright/full-cursor-page-nav.spec.ts`.
- **`getDirSizeDisplayState()` (`full-list-utils.ts`) is the single source of truth for a directory's size-column
  state**, consumed by both `FullList.svelte`'s size cell and `measure-column-widths.ts`. Re-inline the
  dir/scanning/stale decision in either and the rendered text and pre-measured width drift.
- **The Size and Modified columns render with `font-variant-numeric: tabular-nums`, which canvas/pretext can't
  measure.** `measure-column-widths.ts` models it by substituting the widest digit (`tabularize`) before measuring, so
  the two must move together: drop tabular figures from a numeric column and drop its `tabularize` call too, or the
  column over-reserves. Why: `DETAILS.md` § Gotchas.
- **Paired-constant gotcha in `measure-column-widths.ts`**: `HEADER_CHROME_ACTIVE/INACTIVE` mirror `SortableHeader`'s
  gap + caret (12px active / 0 inactive). Change the CSS and you must change the constant, or header column widths drift
  (pretext measures without a reference element, so nothing is derived from the live DOM).
- **Nothing visible in Brief mode may wait on the width IPC** (how the cursor went invisible in prod). ❌
  Don't gate `is-under-cursor` on widths, fall back to `capPx` (use `provisionalColumnWidth`), infer readiness from
  `rawWidths.length`, make `capPx` a fetch trigger, or bail silently. Failure modes: `DETAILS.md` § Cursor visibility.
- **Index-size refresh (`refresh_listing_index_sizes`) refetches column widths through the existing `cacheGeneration`
  reset path**; a separate trigger double-fetches.
- **`listing.showExtensionInName` must stay in lockstep across the renderer and the measurer**: on, `gridTemplate`
  drops the Ext track and `computeFullListColumnWidths` returns `ext: 0`. Change one side and every column drifts. ❌
  Don't "clean up" `FullListHeader`'s `.header-name-ext` split: it's the only way left to CLICK sort-by-extension in
  that mode. Full contract: `DETAILS.md`.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
