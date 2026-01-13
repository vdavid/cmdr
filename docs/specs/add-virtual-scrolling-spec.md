Virtual scrolling implementation

## Overview

Implement virtual scrolling for the file list to handle 100k+ files without DOM performance issues. Currently, all files
are rendered as DOM elements which becomes slow with large directories.

## Current architecture

### Files to modify

- src/lib/file-explorer/FileList.svelte - Main target, needs virtual scrolling
- [src/lib/file-explorer/FilePane.svelte](../../src/lib/file-explorer/FilePane.svelte) - May need updates for scroll
  position management
- [src/lib/file-explorer/apply-diff.ts](../../src/lib/file-explorer/apply-diff.ts) - Already handles cursor
  preservation, likely no changes needed

### Current FileList.svelte structure

```svelte
<ul class="file-list">
    {#each files as file, index (file.path)}
        <li class="file-entry">...</li>
    {/each}
</ul>
```

**Problem:** Renders ALL files in DOM. With 100k files = 100k DOM elements = slow.

**Goal:** Only render ~50 visible items + buffer, recycle DOM nodes as user scrolls.

### Current data flow

```
FilePane.svelte
├── allFilesRaw: FileEntry[]  (plain JS array, NOT reactive)
├── filesVersion: number      (incremented to trigger re-renders)
├── selectedIndex: number     (cursor position)
└── FileList.svelte
    ├── files: FileEntry[]    (filtered view, prop)
    ├── selectedIndex: number (prop)
    └── scrollToIndex(index)  (exported method for keyboard nav)
```

## Interaction with Phase 3.5 (file watching)

### Key concern: Diffs during partial render

When file watching emits a diff (add/remove/modify), [applyDiff()](../../src/lib/file-explorer/apply-diff.ts) in
[apply-diff.ts](../../src/lib/file-explorer/apply-diff.ts) modifies `allFilesRaw` and returns the new cursor index. The
virtual scroller must handle:

1. **Added files** - May be inserted anywhere in the list (sorted insertion)
2. **Removed files** - May be in visible area, before visible area, or after
3. **Modified files** - Same position, just data change
4. **Cursor preservation** - Already handled by [applyDiff()](../../src/lib/file-explorer/apply-diff.ts) which finds
   selected file by path

### Race condition: Diff arrives during scroll

If user is scrolling and a diff arrives:

- `allFilesRaw` length changes
- Virtual scroll calculations (startIndex, endIndex) may become stale
- Must recalculate visible window

**Recommendation:** After [applyDiff()](../../src/lib/file-explorer/apply-diff.ts), bump `filesVersion` (already done)
which should trigger recalculation.

### Edge case: Diff during chunked loading

Files load in chunks (5000 at a time). If a diff arrives while loading:

1. Diff applies to current `allFilesRaw` (partial list)
2. Next chunk arrives and is appended
3. The file from the diff may already exist in the next chunk (duplicate!)

**Current safeguard:** Diffs use path matching, so duplicates would be skipped. But this needs testing.

## Technical approach

### Option A: Fixed row height (recommended)

- Assume each file entry is exactly 24px tall (current CSS: `padding: var(--spacing-xxs) var(--spacing-sm)` ≈ 24px)
- Calculate: `visibleCount = Math.ceil(containerHeight / ROW_HEIGHT)`
- Render: `startIndex` to `startIndex + visibleCount + buffer`
- Use CSS transforms or absolute positioning for visible items

```svelte
<script>
    const ROW_HEIGHT = 24
    let containerHeight = $state(0)
    let scrollTop = $state(0)

    const startIndex = $derived(Math.floor(scrollTop / ROW_HEIGHT))
    const visibleCount = $derived(Math.ceil(containerHeight / ROW_HEIGHT) + 20) // buffer
    const endIndex = $derived(Math.min(startIndex + visibleCount, files.length))
    const visibleFiles = $derived(files.slice(startIndex, endIndex))
    const totalHeight = $derived(files.length * ROW_HEIGHT)
</script>

<div class="scroll-container" bind:clientHeight={containerHeight} onscroll={handleScroll}>
    <div class="spacer" style="height: {totalHeight}px">
        <div class="visible-window" style="transform: translateY({startIndex * ROW_HEIGHT}px)">
            {#each visibleFiles as file, i (file.path)}
                <div class="file-entry">...</div>
            {/each}
        </div>
    </div>
</div>
```

### Option B: Use a virtualization library

Libraries like `svelte-virtual-list` or `svelte-tiny-virtual-list` exist but may have issues with:

- Svelte 5 compatibility
- Custom item rendering
- Dynamic content updates from diffs

**Recommendation:** Implement Option A (fixed height) - it's simpler, more controllable, and sufficient for file lists.

## Required changes

### FileList.svelte

1. **Add container with fixed height and overflow**
2. **Track scroll position and container height**
3. **Calculate visible window (startIndex, endIndex)**
4. **Render only visible items with correct offset**
5. **Update `scrollToIndex()` to scroll by setting `scrollTop`, not `scrollIntoView`**

### FilePane.svelte

1. **May need to pass container height or let FileList handle it**
2. **Ensure `filesVersion` bump triggers virtual list recalculation**

### scrollToIndex() implementation

Current:

```typescript
export function scrollToIndex(index: number) {
    const items = listElement.querySelectorAll('.file-entry')
    const item = items[index]
    item?.scrollIntoView({ block: 'nearest' })
}
```

With virtual scrolling:

```typescript
export function scrollToIndex(index: number) {
    const targetScrollTop = index * ROW_HEIGHT
    const containerBottom = scrollTop + containerHeight

    if (targetScrollTop < scrollTop) {
        // Item above viewport - scroll up
        scrollContainer.scrollTop = targetScrollTop
    } else if (targetScrollTop + ROW_HEIGHT > containerBottom) {
        // Item below viewport - scroll down
        scrollContainer.scrollTop = targetScrollTop - containerHeight + ROW_HEIGHT
    }
    // else: item already visible, do nothing
}
```

## Testing considerations

### Unit tests

- Virtual window calculation with different list sizes
- `scrollToIndex` behavior (above viewport, below viewport, already visible)
- Interaction with [applyDiff](../../src/lib/file-explorer/apply-diff.ts) - cursor should stay visible after diff

### Manual tests

1. **Large directory (100k files):**
    - Scroll performance should be smooth
    - Keyboard navigation should work
    - Cursor should stay visible when navigating

2. **File watching interaction:**
    - Add file at top of list while scrolled to bottom - list should update, cursor stay
    - Delete visible file - adjacent file should become selected
    - Bulk changes (simulate git pull) - cursor should stay on same file or reset

3. **Edge cases:**
    - Scroll to bottom, then delete last file
    - Navigate to parent (..) while virtual scroll is mid-list
    - Resize window while scrolled

## Dependencies

- No external dependencies needed
- Use native scroll APIs
- Use Svelte 5 reactivity (`$derived`, `$state`)

## Performance targets

- Render time for 100k files: < 16ms (60fps)
- Scroll jank: none (use `will-change: transform` if needed)
- Memory: ~50 DOM nodes regardless of list size

## Files to reference

- src/lib/file-explorer/FileList.svelte - Current implementation
- [src/lib/file-explorer/FilePane.svelte](../../src/lib/file-explorer/FilePane.svelte) - Parent component
- [src/lib/file-explorer/apply-diff.ts](../../src/lib/file-explorer/apply-diff.ts) - Cursor preservation logic
- [src/lib/file-explorer/types.ts](../../src/lib/file-explorer/types.ts) - FileEntry type

## Commands

```bash
# Run checks
./scripts/check.sh

# Run just frontend tests
pnpm vitest run

# Dev server
pnpm tauri dev
```

## Success criteria

1. All existing tests pass
2. Directory with 100k files scrolls smoothly (60fps)
3. Keyboard navigation (arrow keys, Enter) works correctly
4. File watching diffs apply correctly while scrolled
5. Cursor stays visible or moves appropriately after diffs
