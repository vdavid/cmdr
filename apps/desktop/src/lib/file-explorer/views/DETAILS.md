# File explorer views details

Pull-tier docs for `apps/desktop/src/lib/file-explorer/views/`: architecture, flows, and decision rationale. Must-know
invariants and gotchas live in `CLAUDE.md`.

## Architecture

### Components

- **BriefList.svelte** – Horizontal columns, per-column shrink-wrapped widths (capped), horizontal scrolling. Column
  text widths come from the backend (`get_brief_column_text_widths` IPC); the component adds chrome, clamps, and builds
  prefix sums for variable-width virtual-scroll math.
- **FullList.svelte** – Vertical rows, full metadata display, vertical scrolling
- **virtual-scroll.ts** – Pure math functions for calculating visible windows. Two flavors: uniform
  (`calculateVirtualWindow` / `getScrollToPosition` for FullList) and variable (`calculateVirtualWindowVariable` /
  `getScrollToPositionVariable` for BriefList, driven by a prefix-sum array).
- **file-list-utils.ts** – Shared helpers: entry caching, icon prefetching, sync status
- **brief-list-utils.ts** / **full-list-utils.ts** – Mode-specific rendering logic. `full-list-utils.ts` includes
  dual-size display helpers: `getDisplaySize()` (picks logical/physical/smart), `hasSizeMismatch()`,
  `buildFileSizeTooltip()`, `buildDirSizeTooltip()`, `buildSelectionSizeTooltip()`, and `getDirSizeDisplayState()` — the
  single source of truth for a directory's size-column CONTENT state
  (`'dir' | 'scanning' | 'lower-bound' | 'size' | 'size-stale'`, a pure function of
  `{recursiveSize, complete, stale, updating}` — the "honest sizes" model; see `$lib/indexing/DETAILS.md` § Honest size
  rendering). The in-flux hourglass is the ORTHOGONAL `isDirSizeUpdating` (`indexing || pending`), not a state value. An
  unknown size (not enriched yet, OR an incomplete subtree with nothing known below it: `complete === false` and
  `recursiveSize === 0`) collapses into `'dir'`/`'scanning'` → the familiar `<dir>` placeholder, never a settled-looking
  value, kept distinct from a genuinely-empty `0 bytes` (`complete === true`, `recursiveSize === 0` → `'size'`).
  `FullList.svelte`'s size cell, `SelectionInfo.svelte`'s Brief status bar, and `measure-column-widths.ts` all consume
  these so rendered text and pre-measured column width agree; don't re-inline the decision in any of them. The
  lower-bound prefix glyph is `LOWER_BOUND_GLYPH` (`≥`, a symbol, not copy).
- **measure-column-widths.ts** – `computeFullListColumnWidths()`: pixel-accurate widths for the Ext / Size / Modified
  columns based on the currently loaded entries. Uses `@chenglou/pretext` for canvas-based measurement (no DOM reflow).
  FullList transitions `grid-template-columns` over 300ms so widths refine smoothly as more entries stream in.
- **FullList.svelte** – `staticEntries?: FileEntry[]` overrides the backend-listing path entirely — the entries array is
  mirrored into `cachedEntries` and the cache fetch / soft-refresh / cache-generation paths short-circuit. Used by the
  search-results virtual volume, which feeds full paths as the entries' `name` field; the column-name cell mid-truncates
  via `useShortenMiddle` (snapping to `/` when the name carries one, `.` otherwise). With the prop unset, FullList
  renders identically to before (same grid template, same fetch loop, same DOM).
- **FullList.svelte** – Reads `listing.sizeDisplay` (via `getSizeDisplayMode()`), `listing.sizeMismatchWarning` (via
  `getSizeMismatchWarning()`), and `listing.sizeUnit` (via `getFileSizeUnit()`, paired with `getFileSizeFormat()`)
  settings. Size cells are rendered through `formatSizeForDisplay` from `selection/selection-info-utils.ts`, which
  delegates to triads in bytes mode, a dynamic friendliest-unit string in dynamic mode, and a forced single-unit string
  in `kB`/`MB`/`GB` mode. `measure-column-widths.ts` accepts the same options so the size column shrink-wraps the
  actually-rendered cell text. Renders glyphs via `<Icon>`: `circle-alert` for size mismatch warnings and `hourglass`
  for the index indicators. The hourglass (`size-updating` wrapper class) shows whenever `isDirSizeUpdating` is true:
  the global `indexing` flag (full scan/aggregation, every size in flux) OR the row's own `recursiveSizePending` (live
  delete/copy in flight for that dir, even with no scan running) — orthogonal to the content state, so it rides on top
  of a size, a `≥` lower bound, or the `<dir>` placeholder (the `dir`/`scanning` states, the latter's tooltip "Sizes
  appear as the scan progresses", so a fresh install reads as quietly working rather than `Scanning...` on every row).
  Freshness-stale (`size-stale` content state) is a SEPARATE, muted treatment on an exact-but-older size, no glyph.
  `measure-column-widths.ts` reserves `SIZE_ICON_WIDTH` whenever `isDirSizeUpdating` so the shrink-wrapped column never
  clips the glyph. The per-dir flag rides `DirStats.recursiveSizePending`, copied onto entries by
  `updateIndexSizesInPlace` / `createParentEntry` (backend: `indexing/pending_sizes.rs`). Also renders an optional Git
  status column between Name and Ext when `gitRepoRoot` is set and `showGitColumn` is true (gated by the
  `fileExplorer.git.showStatusColumn` setting in `FilePane`); fetches `fetchStatusMap` and refreshes on
  `git-state-changed` for the active repo
- **dir-size-display.test.ts** – Tests for `getDirSizeDisplayState` / `buildDirSizeTooltip` (functions in
  `full-list-utils.ts`)
- **view-modes.test.ts** – Integration tests for hidden-file filtering and directory listing structure (uses
  `test-helpers.ts` from parent)

### Data flow

```
FilePane (parent)
  ├── listingId: string           (backend cache key)
  ├── totalCount: number           (for scrollbar sizing)
  ├── cursorIndex: number          (selection position)
  └── BriefList / FullList
        ├── cachedEntries: FileEntry[]   (prefetch buffer ~500 items)
        ├── cachedRange: {start, end}    (cached region)
        └── visibleFiles: FileEntry[]    ($derived from virtual window)
```

**Key**: Data lives in Rust `LISTING_CACHE`. Frontend fetches visible ranges on-demand via
`getFileRange(listingId, start, count, includeHidden)`.

### Virtual scrolling

Uses a configurable row height via `getRowHeight()` from `reactive-settings.svelte.ts` (varies by density setting:
compact/comfortable/spacious). The virtual scroll uses an `itemSize` parameter from `VirtualScrollConfig`:

1. Calculate visible window: `startIndex = floor(scrollTop / itemSize)`
2. Add buffer above/below viewport (20 items default, configurable)
3. Render only `visibleFiles = entries.slice(startIndex, endIndex)`
4. Position via `transform: translateY(startIndex * itemSize)`
5. Spacer div maintains scrollbar accuracy: `height: totalCount * itemSize`

**Prefetch buffer**: ~500 items around current position, cached in `cachedEntries`. Reduces IPC calls during scroll.

Native drag auto-scroll uses the same scroll state and fetch path as keyboard/cursor scrolling, but one animation frame
at a time. `FullList.autoScrollDuringDrag(position, elapsedMs)` scrolls `scrollTop` vertically from top/bottom edge
bands; `BriefList.autoScrollDuringDrag(position, elapsedMs)` scrolls `scrollLeft` horizontally from left/right edge
bands. Both call `fetchVisibleRange()` when they move so newly revealed rows/columns can be hit-tested immediately by
the drag controller.

## Key decisions

**Decision**: `FullList`'s column header lives **inside** the scroll container as a `position: sticky; top: 0;` child,
not as a sibling above. **Why**: when the user has "Always show scrollbars" set (System Settings → Appearance),
non-overlay scrollbars steal a ~15 px gutter from the scroll container. A sibling header rendering at the wrapper's full
width then misaligned with the data rows below. Moving the header inside makes it share the row content width
automatically (and therefore the scrollbar gutter), so columns line up at every scrollbar mode without JS measurement.
Virtual-scroll math: the spacer follows the header in natural flow, so the spacer's content origin (row 0) sits
`headerHeight` pixels into the unscrolled document. The sticky header always covers the first `headerHeight` pixels of
the viewport once any scroll has happened, so the effective row area is `containerHeight - headerHeight`. Critically,
`scrollTop` and the spacer's scroll offset are the same number — no translation needed. `FullList` therefore derives
`spacerScrollTop = scrollTop` and `rowAreaHeight = containerHeight - headerHeight` and feeds those into
`calculateVirtualWindow` / `getScrollToPosition` / `firstVisibleGlobalIndex` / `lastVisibleGlobalIndex` /
`getVisibleItemsCountUtil`. `scrollToIndex` writes `getScrollToPosition`'s result straight to
`scrollContainer.scrollTop`. A11y: the listbox role moves off `.full-list` (now a generic scroll container) onto a
`.listbox-region` inner wrapper around `.virtual-spacer` so the sticky header isn't a direct child of the listbox (would
violate `aria-required-children`).

**Don't reintroduce a `scrollTop - headerHeight` shift with a `Math.max(0, …)` clamp**: `scrollTop ∈ [0, headerHeight]`
then collapses to the same spacer state. PageDown × 2 → PageUp × 2 lands at `scrollTop === headerHeight`, hiding row 0
(including the `..` cursor) under the sticky header. The pinned regression is
`test/e2e-playwright/full-cursor-page-nav.spec.ts`.

**Decision**: Virtual scroll in frontend, data in backend **Why**: Sending 50k entries over IPC = 17.4MB, ~4s transfer.
Virtual scroll fetches only visible ~50 items on demand. Backend-driven caching eliminates serialization overhead.

**Decision**: Uniform row height per density setting (no variable height) **Why**: Variable height requires measuring
every row, defeating performance gains. Uniform height allows pure math: `scrollTop / itemSize = startIndex`.

**Decision**: Prefetch buffer (~500 items) **Why**: Smooth scrolling requires data ready before user sees blank space.
Buffer balances memory (small) vs. IPC latency (reduces fetches).

**Decision**: Cache invalidation via `cacheGeneration` prop **Why**: Changing sort, toggling hidden files, or resizing
window requires fresh data. Parent bumps `cacheGeneration`, triggering re-fetch. Uses `$effect()` to react.

**Decision**: Icon prefetching only for visible entries **Why**: With 50k files, prefetching all icons = 50k IPC calls.
Virtual scrolling renders only ~50 items, so prefetch only visible. Re-fetch on scroll. The same visible-range pass in
`fetchVisibleRange` also drives Tier-C custom-folder icons: it collects the visible directory rows' paths and calls
`prefetchCustomFolderIcons` (→ backend `get_custom_folder_icon_ids`), which runs the `kHasCustomIcon` `getxattr` only
for that bounded on-screen set and returns `path:{dir}` ids to fetch. The bulk listing never pays the per-entry syscall;
packages already arrive as `pkg:` ids from `get_icon_id`. `FilePane` evicts a directory's `path:*` / `pkg:*` icons via
`evictPerPathIconsForDir(loadedPath)` when its listing ends (navigation away / unmount), so a folder re-iconed while
away is re-detected next time it's shown.

**Decision**: Finder tag dots (`TagDots.svelte`) ride the same visible-range pass as custom-folder icons, and reserve
their cluster width in the column math **Why**: Tags (`com.apple.metadata:_kMDItemUserTags`) are a per-file `getxattr`,
too costly to read inline in the bulk listing (~6× an `lstat`), so the backend defers them. `fetchVisibleRange` calls
`commands.enrichTags(listingId, visiblePaths)` right beside `prefetchCustomFolderIcons` (gated by the `listing.showTags`
setting); the backend patches the cache and emits a coalesced `directory-diff`, which re-fetches the range and
re-renders the dots. `FilePane.handleListingComplete` additionally kicks off a low-priority **background sweep**
(`sweepTagsForListing`, 500-path chunks) so off-screen rows get tags too; it's cancelable — each chunk re-checks the
pane wasn't destroyed and the listing is still current (`loadGeneration` / `listingId`). The dots cluster at the right
edge of the Name cell: in **Full** mode the Name column is `1fr`, so flexbox gives the dynamic-space behavior for free
(name `flex: 1; min-width: 0` truncates, `TagDots` is `flex-shrink: 0`); in **Brief** mode columns are
width-constrained, so `brief_columns.rs` adds a per-row `tag_cluster_width` suffix (a pure function of the colored-tag
count, mirroring `tagClusterWidthPx` in `tag-dots-utils.ts` — keep the two in sync) before taking the per-column max, or
the dots would clip the next column. Tags arrive after first paint, so the column grows once when the tag batch lands:
one accepted "settle" per directory (D10). Only colored tags (index 1-7) draw a dot; colourless tags (index 0) are
dotless but still listed in the cluster's accessible label.

**Decision**: Brief columns shrink-wrap to the widest filename in each column, with the backend measuring widths and the
frontend rendering to those measurements **Why**: Long filenames deserve their full width while short ones let the user
scan more columns at once. The Rust backend owns the text data and the font metrics cache, so it computes the widest
filename's text width per column in one IPC call
(`get_brief_column_text_widths(listingId, itemsPerColumn, hasParent, fontId, includeHidden)`). The FE adds CSS chrome
(icon + gaps + padding), clamps to `[MIN_COLUMN_WIDTH, capPx]` where
`capPx = min(containerWidth, MAX_BRIEF_COLUMN_WIDTH)`, and stores the result as `columnWidths: number[]`. A `prefixSums`
array (`$derived`) drives all virtual-scroll math: `totalSize` is the final prefix sum, `calculateVirtualWindowVariable`
binary-searches `prefixSums` for the visible range, and `getScrollToPositionVariable` looks up exact column edges.
Scrollbar size and cursor visibility now agree with what's actually rendered. `transition: width 300ms ease` still
animates width changes within a directory; nav resets snap via the `skipTransition` 2-rAF trick. While widths are in
flight (first paint, after `FontMetricsNotReady`), every column renders at `capPx` as a fallback, and the cursor
highlight is suppressed until `columnWidths.length > 0` so the user doesn't see a full-pane-wide cursor stripe for a
frame. The initial fetch after a dir change skips the 50 ms coalesce so that gap is as short as possible; re-fetches
during resize keep the coalesce.

**Decision**: A single `$effect` keeps the cursor in view, depending on `cursorIndex`, `containerWidth`,
`containerHeight`, and `columnWidths.length` **Why**: With exact prefix-sum math, every input that could move the
cursor's column out of view is a state read, so one consolidated effect replaces the old height-only effect plus the
implicit width-resize gap. It re-runs naturally after `columnWidths` arrives (the reassignment retriggers the
dependency), so a fast resize-drag → fetch → widths-arrive sequence ends with `scrollToIndex(cursorIndex)` settling the
view exactly once. PageUp/PageDown step distance is content-dependent, derived from `prefixSums` directly (not from the
container width), so a "page" of skinny columns moves more files than a page of wide ones. Intentional UX: the step
matches what's visible.

**Decision**: Shrink-wrap Ext / Size / Modified columns from the rows **currently on screen**, not the prefetch buffer
or the full directory **Why**: The name column should keep every spare pixel, so columns track live content. Pretext's
canvas measurement is fast enough to recompute on every scroll row-crossing and window resize. The 300ms
`grid-template-columns ease` transition (on both `.header-row` and `.file-entry`) smooths the resulting width changes.
Dir switches snap instead of animating (see Gotcha below). The `..` row's (often huge) recursive size only contributes
when that row is actually on screen. Otherwise the size column would stay oversized after scrolling past it.
`SelectionInfo` keeps using `measureDateColumnWidth` (worst-case sampling) because it renders a single-entry snapshot
with no "visible set" to measure from.

**Decision**: The date column renders as one segment list with tabular figures, no split. **Why**: Earlier the column
split into a fixed-width date half plus a time half so the times lined up across rows despite proportional digits. With
`font-variant-numeric: tabular-nums` on `.col-date` every digit takes the same advance, and every token format (`YYYY`=4
digits, the rest zero-padded to 2) emits a fixed character count, so all dates are the same width and align on their
own. The contract: `formatDateForDisplay` (in `lib/settings/format-utils.ts`) returns a `FormattedDate` whose `segments`
is the ordered segment list; `computeFullListColumnWidths` measures the joined string once per row (tabular-aware, see
the digit gotcha below); `FullList` walks the segments, wrapping any with a non-null `ageClass` in an age-tier span.
Tooltips/MCP/status bar see the joined string via `FormattedDate.text`.

**Decision**: Column-width measurers (canvas in `full-list-utils.ts`, pretext in `measure-column-widths.ts`) cache their
measurer/context per text scale and rebuild on the **debounced** "settled" scale event from
`lib/text-size.svelte::onDebouncedScaleChange`, not on every reactive read. **Why**: the CSS layer reflows immediately
via `--font-scale`, so users see text grow live. Recomputing per-column widths on every slider step would thrash pretext
rebuilds. Coalescing to the same 1 s + idle window the font-metrics IPC uses keeps the UI smooth during drag and snaps
to correct widths once the user releases. `FullList` tracks the settle event via a local `scaleSettleTick` `$state` it
bumps from the subscription, then reads inside the column-width `$effect`. `BriefList`'s Brief-column widths come from
the backend `get_brief_column_text_widths` IPC, which uses the live font ID. The same `onDebouncedScaleChange` callback
triggers a refetch.

**Decision**: `listing.showExtensionInName` (default off) folds the extension back into the Name column and hides the
Ext column. **Why**: the Norton/Total Commander Name/Ext split confuses users who expect to see `launch.json` whole, not
`launch` with the `json` parked two columns over. Off keeps today's split. On: the Name cell renders `file.name`
verbatim (via `getNameColumnText`), and the Ext column header + cells aren't rendered. The renderer and the
width-measurer are one contract: `FullList`'s `gridTemplate` drops the Ext track and `computeFullListColumnWidths`
returns `ext: 0` in this mode, so the grid has no orphaned track and the Name column (`1fr`) absorbs the freed space.
The shared `getNameColumnText(name, isDirectory, showExtensionInName)` in `full-list-utils.ts` is the single name-text
decision both the cell and (implicitly, since name is `1fr` and unmeasured) the layout agree on. Sort-by-extension isn't
stranded by hiding the header: the `sort.byExtension` command stays in the command palette and is shortcut-bindable, so
the only loss is the click-the-header affordance. Brief view is unaffected (it already renders `file.name` whole). The
inline rename editor's column span shrinks in this mode (`.col-rename.no-ext-col`) so it doesn't bleed into the Size
column now that the Ext track is gone.

## Gotchas

**Gotcha**: The gutter that keeps the cursor and selection fills off the pane edges lives at a DIFFERENT level in each
view, and can't be hoisted to `FilePane`'s `.content`. **Why**: the column header has to keep spanning edge to edge. In
Full view the header is a `position: sticky` child of the scroll container, so the gutter sits on the inner
`.listbox-region` (rows only) and `.header-row` carries double horizontal padding instead, keeping its grid aligned with
the rows while its background stays full-bleed. In Brief view the header is a sibling ABOVE the scroll container, so the
gutter can sit on `.brief-list` itself. Padding it any further out (`.content`, `.full-list`) insets the header
background and leaves bare strips at both ends.

**Gotcha**: BOTH views measure their scroll surface with `bind:clientWidth` / `clientHeight`, which report the content
box PLUS the element's own padding — so any layout math that asks "how much fits" must subtract the gutter, or the
rightmost column and the last row of each column render clipped. Brief view derives `usableWidth` / `usableHeight` and
feeds those to the per-column cap, the virtual window, scroll-into-view, and items-per-column; the raw
`containerWidth` / `containerHeight` survive only as `> 0` liveness checks.

**Gotcha**: Full view's vertical gutter shifts the virtual spacer inside the scroll content, so container `scrollTop`
and spacer offset differ by `GUTTER_PX`. **Why**: `.listbox-region`'s block padding sits ABOVE the spacer. Both
conversions are corrected — `spacerScrollTop` subtracts the gutter, and `scrollToIndex` adds it back when writing
`scrollContainer.scrollTop` (special-casing `0` so scrolling to the first row still shows the top gutter). Skipping
either drifts the cursor-into-view by a gutter at the list's ends. Each view keeps its own `GUTTER_PX`, which must stay
in sync with its element's `padding`.

**Gotcha**: Both views' header rows open with a dead `.header-icon` spacer, and their left inset is the scroll
surface's gutter PLUS `.file-entry`'s own padding. **Why**: without it the first heading sits above the file ICONS
instead of the file NAMES. Full view gets the alignment from sharing `gridTemplate` with the rows; Brief view's header
is a plain flex row, so it reproduces the row's geometry by hand (inset, icon-width spacer, `--spacing-sm` gap).
`SortableHeader`'s negative horizontal margin is what makes the LABEL, not the button box, land on the alignment point,
so don't remove it while "fixing" a 4px offset.

**Gotcha**: Both scroll containers carry a `data-file-list-surface` attribute (`.brief-list` and `.full-list`) — don't
drop it. **Why**: the pane's double-click-to-parent gesture (`pane/pane-background-dblclick.ts`, gated by
`behavior.doubleClickPaneNavigatesToParent`) hit-tests on it to tell "empty list background" from a row or the Full-view
sort header. It can't key off `[role="listbox"]`: in Full view the listbox spans only the rows, so the empty space below
a short listing falls outside it and the gesture silently no-ops there (the original bug). The surface fills the pane in
both views, so it covers that gap. Remove the attribute and double-click-to-parent quietly dies with no view-level test
catching it (the contract is covered in `pane/pane-background-dblclick.test.ts`).

**Gotcha**: `$state()` cannot live in `.ts` files **Why**: `virtual-scroll.ts` is pure functions. Reactive state must be
in `.svelte` or `.svelte.ts`. Math functions return plain objects consumed by `$derived` in components.

**Gotcha**: File watcher diffs shift indices while scrolled **Why**: If 20 files added before cursor, visible range
shifts by 20. Must recalculate virtual window when `totalCount` changes.

**Gotcha**: When `hasParent = true`, UI indices are 1-based **Why**: Index 0 is ".." parent entry (not in backend
cache). Real files start at index 1. Adjust: `cache_index = ui_index - 1`.

**Gotcha**: The ".." row shows the CURRENT folder's recursive size, not the parent folder's **Why**: The `..` row's size
column is otherwise wasted space. Showing the total for the folder the user is browsing (sum of everything visible plus
unloaded entries) answers "how much is in here?", more useful than "how big is the place I'd go if I pressed Backspace."
Implementation: `createParentEntry(parentPath, stats?)` in `file-list-utils.ts` takes optional stats;
`BriefList`/`FullList` fetch them via `getDirStatsBatch([currentPath])` on dir change and via
`updateIndexSizesInPlace(cachedEntries, currentPath)` on index refresh (single batch IPC call).

**Gotcha**: Scroll position must use `transform`, not absolute positioning **Why**: Absolute positioning causes full
layout recalc. `transform` uses GPU compositor for 60fps.

**Gotcha**: Cache re-fetch during scroll uses range expansion **Why**: If visible range is [100, 150] but cached is [0,
200], don't re-fetch. If scrolled to [250, 300], expand fetch to [0, 550] to include buffer. `shouldResetCache()`
handles this.

**Gotcha**: The Size and Modified columns render with `font-variant-numeric: tabular-nums`, but canvas/pretext can't
measure that OpenType feature (the canvas `font` shorthand has no slot for it). **Why**: `measure-column-widths.ts`
models it by substituting every digit with the font's widest digit (`tabularize`) before measuring, so the
shrink-wrapped column matches what the DOM draws. Without it, a row of narrow digits (`11/11/1111`) renders wider than
measured and ellipsizes. If you drop tabular figures from a numeric column, drop its `tabularize` call too.

**Gotcha**: `HEADER_CHROME_ACTIVE/INACTIVE` in `measure-column-widths.ts` are tied to `SortableHeader`'s flex gap +
caret glyph (4px gap + 8px caret = 12px active, 0px inactive). The button keeps 4px horizontal padding for hover-state
breathing room, but an equal negative margin (`margin: 0 calc(-1 * var(--spacing-xs))`) pulls it back out so the label
still lines up with the data cells below. Only gap+caret count toward the track width. **Why**: If you change those CSS
values or the caret size/markup, update the two constants or column widths drift. The values aren't derived from the
live DOM because pretext measurement runs without a reference element. Everything is computed from the pre-known chrome
formula.

**Gotcha**: Width transitions would "slide" on dir switches, because the header (FullList) and columns (BriefList)
persist across navs **Why**: When `shouldResetCache` fires, both lists set a `skipTransition` flag and clear it after
two `requestAnimationFrame` ticks (one to paint with `transition: none`, one more before re-enabling). FullList also
holds widths while `cachedEntries` is empty so the brief post-nav gap doesn't collapse to header-only floors. Combined,
nav = snap; within-dir scroll/resize/stream-in = animated.

**Gotcha**: CJK / complex-script filenames may be slightly mis-measured **Why**: The frontend canvas measurer
(`$lib/font-metrics/measure.ts`) iterates explicit Unicode ranges covering Latin, BMP-printable characters, and common
emoji (U+1F300–U+1FAFF). The backend stores those widths per code point and falls back to the cached `average_width` for
anything outside the measured set, so column widths for CJK, Arabic, and rare-symbol filenames are approximate. Emoji is
fine (measured). Latin is fine (measured). Expanding the measured set is a follow-up.

**Gotcha**: Index-size refresh (`refresh_listing_index_sizes`) triggers a column-width refetch through the existing
cache-reset path, not a separate trigger **Why**: When `recursive_size` enrichment lands, the listing may re-sort; the
existing `cacheGeneration` bump propagates into BriefList's reset-cache effect, which clears `columnWidths`, bumps
`widthsGeneration`, and kicks off `fetchColumnWidths()`. Don't add a separate trigger: it would double-fetch.

**Gotcha**: No `will-change: transform` on `.virtual-window` (`FullList.svelte`). **Why**: it force-promoted a permanent
GPU compositor layer that WebKit kept re-backing on every scroll/content change, ballooning `IOAccelerator` (GPU) memory
to 1+ GB under heavy re-render; the `translateY` scroll still composites on demand. Don't re-add it, and don't reach for
per-row `contain: layout paint` either (it backfires: one retained backing store per row). The full
GPU/compositor-memory investigation — findings, the reclaimable-not-a-leak conclusion, the measurement methodology and
its gotchas, and kick-off context for any future high-memory report — is in
`docs/notes/high-memory-gpu-compositor-investigation-2026-07.md`.
