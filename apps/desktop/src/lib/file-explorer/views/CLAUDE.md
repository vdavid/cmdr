# File explorer views

This directory contains the virtual-scrolling file list components and utilities for rendering 100k+ file directories
without DOM performance issues.

## Architecture

### Components

- **BriefList.svelte** – Horizontal columns, fixed-width items, horizontal scrolling
- **FullList.svelte** – Vertical rows, full metadata display, vertical scrolling
- **virtual-scroll.ts** – Pure math functions for calculating visible windows
- **file-list-utils.ts** – Shared helpers: entry caching, icon prefetching, sync status
- **brief-list-utils.ts** / **full-list-utils.ts** – Mode-specific rendering logic
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

## Gotchas

**Gotcha**: `$state()` cannot live in `.ts` files **Why**: `virtual-scroll.ts` is pure functions. Reactive state must be
in `.svelte` or `.svelte.ts`. Math functions return plain objects consumed by `$derived` in components.

**Gotcha**: File watcher diffs shift indices while scrolled **Why**: If 20 files added before cursor, visible range
shifts by 20. Must recalculate virtual window when `totalCount` changes.

**Gotcha**: When `hasParent = true`, UI indices are 1-based **Why**: Index 0 is ".." parent entry (not in backend
cache). Real files start at index 1. Adjust: `cache_index = ui_index - 1`.

**Gotcha**: Scroll position must use `transform`, not absolute positioning **Why**: Absolute positioning causes full
layout recalc. `transform` uses GPU compositor for 60fps.

**Gotcha**: Cache re-fetch during scroll uses range expansion **Why**: If visible range is [100, 150] but cached is [0,
200], don't re-fetch. If scrolled to [250, 300], expand fetch to [0, 550] to include buffer. `shouldResetCache()`
handles this.
