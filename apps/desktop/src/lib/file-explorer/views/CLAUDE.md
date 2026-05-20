# File explorer views

This directory contains the virtual-scrolling file list components and utilities for rendering 100k+ file directories
without DOM performance issues.

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
  `buildFileSizeTooltip()`, `buildDirSizeTooltip()`, `buildSelectionSizeTooltip()`
- **measure-column-widths.ts** – `computeFullListColumnWidths()`: pixel-accurate widths for the Ext / Size / Modified
  columns based on the currently loaded entries. Uses `@chenglou/pretext` for canvas-based measurement (no DOM reflow).
  FullList transitions `grid-template-columns` over 300ms so widths refine smoothly as more entries stream in.
- **FullList.svelte** – Reads `listing.sizeDisplay` (via `getSizeDisplayMode()`), `listing.sizeMismatchWarning` (via
  `getSizeMismatchWarning()`), and `listing.humanFriendlySizeUnits` (via `getHumanFriendlySizeUnits()`, paired with
  `getFileSizeFormat()`) settings. Size cells are rendered through `formatSizeForDisplay` from
  `selection/selection-info-utils.ts`, which delegates to triads in raw-bytes mode and to a single tier-tagged
  human-friendly string when the toggle is on. `measure-column-widths.ts` accepts the same options so the size column
  shrink-wraps the actually-rendered cell text. Uses Lucide icons (via `unplugin-icons`): `~icons/lucide/circle-alert`
  for size mismatch warnings and `~icons/lucide/hourglass` for stale index indicators. Also renders an optional Git
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

## Key decisions

**Decision**: `FullList`'s column header lives **inside** the scroll container as a `position: sticky; top: 0;` child,
not as a sibling above. **Why**: when the user has "Always show scrollbars" set (System Settings → Appearance),
non-overlay scrollbars steal a ~15 px gutter from the scroll container. A sibling header rendering at the wrapper's full
width then misaligned with the data rows below. Moving the header inside makes it share the row content width
automatically (and therefore the scrollbar gutter), so columns line up at every scrollbar mode without JS measurement.
The trade-off is virtual-scroll math: row positions are now `headerHeight` pixels into the scrollable content, so
`FullList` derives `spacerScrollTop = max(0, scrollTop - headerHeight)` and
`rowAreaHeight = containerHeight - headerHeight` and feeds those into `calculateVirtualWindow` / `getScrollToPosition` /
`firstVisibleGlobalIndex` / `lastVisibleGlobalIndex` / `getVisibleItemsCountUtil`. `scrollToIndex` adds `headerHeight`
back when writing to `scrollContainer.scrollTop`. A11y: the listbox role moves off `.full-list` (now a generic scroll
container) onto a `.listbox-region` inner wrapper around `.virtual-spacer` so the sticky header isn't a direct child of
the listbox (would violate `aria-required-children`).

**Decision**: Virtual scroll in frontend, data in backend **Why**: Sending 50k entries over IPC = 17.4MB, ~4s transfer.
Virtual scroll fetches only visible ~50 items on demand. Backend-driven caching eliminates serialization overhead.

**Decision**: Uniform row height per density setting (no variable height) **Why**: Variable height requires measuring
every row, defeating performance gains. Uniform height allows pure math: `scrollTop / itemSize = startIndex`.

**Decision**: Prefetch buffer (~500 items) **Why**: Smooth scrolling requires data ready before user sees blank space.
Buffer balances memory (small) vs. IPC latency (reduces fetches).

**Decision**: Cache invalidation via `cacheGeneration` prop **Why**: Changing sort, toggling hidden files, or resizing
window requires fresh data. Parent bumps `cacheGeneration`, triggering re-fetch. Uses `$effect()` to react.

**Decision**: Icon prefetching only for visible entries **Why**: With 50k files, prefetching all icons = 50k IPC calls.
Virtual scrolling renders only ~50 items, so prefetch only visible. Re-fetch on scroll.

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

**Decision**: Date column may split into two aligned sub-columns via a `|` in the format string **Why**: Time digits
across rows zigzag horizontally when date widths vary (e.g., locale formats, custom strings). The split makes the right
halves line up. The contract: `formatDateForDisplay` (in `lib/settings/format-utils.ts`) returns a `FormattedDate` whose
`parts: { left: DateSegment[], right: DateSegment[] | null }` carries both halves as ordered segment lists;
`computeFullListColumnWidths` measures each half separately (via `joinSegments`) and exposes a `dateLeft` width;
`FullList` walks each half's segments (wrapping any with a non-null `ageClass` in an age-tier span and emitting the rest
as plain text) into `.date-left` (inline-block, fixed width, right-aligned) followed by `.date-right`
(`margin-left: var(--spacing-xs)`). Tooltips/MCP/status bar still see joined strings via `FormattedDate.text` (exposed
as the `formatDateTime` shortcut).

**Decision**: Column-width measurers (canvas in `full-list-utils.ts`, pretext in `measure-column-widths.ts`) cache their
measurer/context per text scale and rebuild on the **debounced** "settled" scale event from
`lib/text-size.svelte::onDebouncedScaleChange`, not on every reactive read. **Why**: the CSS layer reflows immediately
via `--font-scale`, so users see text grow live. Recomputing per-column widths on every slider step would thrash pretext
rebuilds. Coalescing to the same 1 s + idle window the font-metrics IPC uses keeps the UI smooth during drag and snaps
to correct widths once the user releases. `FullList` tracks the settle event via a local `scaleSettleTick` `$state` it
bumps from the subscription, then reads inside the column-width `$effect`. `BriefList`'s Brief-column widths come from
the backend `get_brief_column_text_widths` IPC, which uses the live font ID. The same `onDebouncedScaleChange` callback
triggers a refetch.

## Gotchas

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

**Gotcha**: `DATE_PARTS_GAP` (4px) in `measure-column-widths.ts` mirrors the `margin-left: var(--spacing-xs)` on
`.date-right` in `FullList.svelte`. **Why**: The measurer adds it to the total date column width when any visible row
splits via `|`. If you change either value, change both: split-date columns will be one or two pixels off from what the
renderer actually draws otherwise.

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
