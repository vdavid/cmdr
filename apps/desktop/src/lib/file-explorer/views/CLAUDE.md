# File explorer views

This directory contains the virtual-scrolling file list components and utilities for rendering 100k+ file directories
without DOM performance issues.

## Architecture

### Components

- **BriefList.svelte** – Horizontal columns, fixed-width items, horizontal scrolling
- **FullList.svelte** – Vertical rows, full metadata display, vertical scrolling
- **virtual-scroll.ts** – Pure math functions for calculating visible windows
- **file-list-utils.ts** – Shared helpers: entry caching, icon prefetching, sync status
- **brief-list-utils.ts** / **full-list-utils.ts** – Mode-specific rendering logic. `full-list-utils.ts` includes
  dual-size display helpers: `getDisplaySize()` (picks logical/physical/smart), `hasSizeMismatch()`,
  `buildFileSizeTooltip()`, `buildDirSizeTooltip()`, `buildSelectionSizeTooltip()`
- **measure-column-widths.ts** – `computeFullListColumnWidths()`: pixel-accurate widths for the Ext / Size / Modified
  columns based on the currently loaded entries. Uses `@chenglou/pretext` for canvas-based measurement (no DOM reflow).
  FullList transitions `grid-template-columns` over 300ms so widths refine smoothly as more entries stream in.
- **FullList.svelte** – Reads `listing.sizeDisplay` (via `getSizeDisplayMode()`) and `listing.sizeMismatchWarning` (via
  `getSizeMismatchWarning()`) settings. Uses UnoCSS/Lucide `i-lucide:circle-alert` for size mismatch warnings and
  `i-lucide:hourglass` for stale index indicators
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

**Decision**: Shrink-wrap Ext / Size / Modified columns from the rows **currently on screen**, not the prefetch buffer
or the full directory **Why**: The name column should keep every spare pixel, so columns track live content. Pretext's
canvas measurement is fast enough to recompute on every scroll row-crossing and window resize. The 300ms
`grid-template-columns ease` transition (on both `.header-row` and `.file-entry`) smooths the resulting width changes.
Dir switches snap instead of animating (see Gotcha below). The `..` row's (often huge) recursive size only contributes
when that row is actually on screen — otherwise the size column would stay oversized after scrolling past it.
`SelectionInfo` keeps using `measureDateColumnWidth` (worst-case sampling) because it renders a single-entry snapshot
with no "visible set" to measure from.

## Gotchas

**Gotcha**: `$state()` cannot live in `.ts` files **Why**: `virtual-scroll.ts` is pure functions. Reactive state must be
in `.svelte` or `.svelte.ts`. Math functions return plain objects consumed by `$derived` in components.

**Gotcha**: File watcher diffs shift indices while scrolled **Why**: If 20 files added before cursor, visible range
shifts by 20. Must recalculate virtual window when `totalCount` changes.

**Gotcha**: When `hasParent = true`, UI indices are 1-based **Why**: Index 0 is ".." parent entry (not in backend
cache). Real files start at index 1. Adjust: `cache_index = ui_index - 1`.

**Gotcha**: The ".." row shows the CURRENT folder's recursive size, not the parent folder's **Why**: The `..` row's size
column is otherwise wasted space. Showing the total for the folder the user is browsing (sum of everything visible plus
unloaded entries) answers "how much is in here?" — more useful than "how big is the place I'd go if I pressed
Backspace." Implementation: `createParentEntry(parentPath, stats?)` in `file-list-utils.ts` takes optional stats;
`BriefList`/`FullList` fetch them via `getDirStatsBatch([currentPath])` on dir change and via
`updateIndexSizesInPlace(cachedEntries, currentPath)` on index refresh (single batch IPC call).

**Gotcha**: Scroll position must use `transform`, not absolute positioning **Why**: Absolute positioning causes full
layout recalc. `transform` uses GPU compositor for 60fps.

**Gotcha**: Cache re-fetch during scroll uses range expansion **Why**: If visible range is [100, 150] but cached is [0,
200], don't re-fetch. If scrolled to [250, 300], expand fetch to [0, 550] to include buffer. `shouldResetCache()`
handles this.

**Gotcha**: `HEADER_CHROME_ACTIVE/INACTIVE` in `measure-column-widths.ts` are tied to `SortableHeader`'s padding + flex
gap + caret glyph (`--spacing-xs` = 4px × 3 + 8px caret = 20px active, 4px × 2 = 8px inactive) **Why**: If you change
those CSS values or the caret size/markup, update the two constants or column widths drift. The values aren't derived
from the live DOM because pretext measurement runs without a reference element — everything is computed from the
pre-known chrome formula.

**Gotcha**: FullList's `grid-template-columns` transition would "slide" the header on dir switches, because the header
lives outside the virtual scroll and persists across navs **Why**: When `shouldResetCache` fires, a `skipTransition`
flag is set and cleared after two `requestAnimationFrame` ticks (one to paint with `transition: none`, one more before
re-enabling). Widths also don't update while `cachedEntries` is empty AND `parentDirStats` is null, so the brief
post-nav gap doesn't collapse them to header-only floors. Combined, nav = snap; within-dir scroll/resize/stream-in =
animated.
