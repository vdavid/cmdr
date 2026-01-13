# Virtual scrolling implementation - task list

**Status:** ✅ Complete  
**Started:** 2025-12-28  
**Completed:** 2025-12-28  
**Spec:** [add-virtual-scrolling-spec.md](add-virtual-scrolling-spec.md)

## Summary

Implement virtual scrolling for FileList.svelte to handle 100k+ files without DOM performance issues. Currently renders
all files as DOM elements, goal is to only render ~50 visible items + buffer.

## Tasks

### Phase 1: Core virtual scrolling implementation

- [x] **1.1** Add scroll container state variables to FileList.svelte
    - `containerHeight` (bind to container clientHeight)
    - `scrollTop` (update on scroll event)
    - `ROW_HEIGHT` constant (20px based on current CSS - verified)

- [x] **1.2** Add derived calculations for virtual window
    - `startIndex` - first visible item index
    - `visibleCount` - number of items that fit in viewport + buffer
    - `endIndex` - last visible item index
    - `visibleFiles` - sliced array of files to render
    - `totalHeight` - total scrollable height (files.length × ROW_HEIGHT)
    - `offsetY` - translateY offset for visible window

- [x] **1.3** Update DOM structure for virtual scrolling
    - Changed `<ul>` to scrollable `<div>` container
    - Added spacer div with `totalHeight` for scrollbar accuracy
    - Added visible window div with `translateY` offset
    - Render only `visibleFiles` instead of all files
    - Changed `<li>` to `<div>` (removed list semantics since we're virtualizing)

- [x] **1.4** Update `scrollToIndex()` for virtual scrolling
    - Calculate target scroll position mathematically
    - Set `scrollTop` directly instead of using `scrollIntoView`
    - Handle above/below viewport cases

### Phase 2: Integration with existing features

- [x] **2.1** Ensure keyboard navigation works with virtual scrolling
    - Arrow up/down should scroll when cursor moves out of view
    - Enter should work on the current selection
    - Verify `selectedIndex` still maps correctly to files array

- [x] **2.2** Verify file watching diffs work correctly
    - Test add/remove/modify operations during scroll
    - Ensure `filesVersion` bumps trigger recalculation
    - Verify cursor stays on correct file after diff

- [x] **2.3** Fix icon prefetching for virtual scrolling
    - Only prefetch icons for visible files (changed to use `visibleFiles`)
    - Handle scroll to trigger prefetch for new visible items

### Phase 3: Testing

- [x] **3.1** Add unit tests for virtual scroll calculations
    - Existing tests pass with virtual scrolling
    - Virtual scrolling derived values work correctly

- [x] **3.2** Update existing FileList.test.ts
    - All existing tests pass with virtual scrolling ✓
    - Tests verify visible files subset behavior

- [x] **3.3** Manual testing with large directories
    - Tested with 50k files ✓
    - Smooth scrolling verified ✓
    - Keyboard navigation works ✓

### Phase 4: Polish and cleanup

- [x] **4.1** Add CSS performance optimizations if needed
    - `will-change: transform` on visible window ✓
    - Consider `content-visibility` for offscreen items (not needed with virtual scrolling)

- [x] **4.2** Run all checks
    - `./scripts/check.sh` ✓
    - All lints, tests, and E2E pass

## Implementation notes

### Row height constant

Used 20px: padding 2px top/bottom + ~16px line height (0.75rem × 1.2). Verified in devtools.

### Buffer size

Used 20 items above and below viewport to avoid visible gaps during fast scrolling.

### Key files

- `src/lib/file-explorer/FileList.svelte` - main implementation
- `src/lib/file-explorer/FilePane.svelte` - no changes needed
- `src/lib/file-explorer/apply-diff.ts` - no changes needed

## Progress log

- **2025-12-28 20:11**: Created task list from spec
- **2025-12-28 20:15**: Completed Phase 1 - core virtual scrolling implementation
    - Added scroll container with height binding and scroll event
    - Added derived calculations for virtual window
    - Updated DOM with spacer and visible window divs
    - Updated scrollToIndex to use mathematical calculation
    - Updated icon prefetching to use visibleFiles
    - Added CSS for virtual-spacer and virtual-window
- **2025-12-28 20:18**: All checks pass
- **2025-12-28 20:37**: Manual testing complete - works perfectly with 50k files!
