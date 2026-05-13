// Shared virtual scrolling calculations for file lists
// Used by both BriefList (horizontal) and FullList (vertical)

export interface VirtualScrollConfig {
  /** Scroll direction */
  direction: 'vertical' | 'horizontal'
  /** Size of each item (row height for vertical, column width for horizontal) */
  itemSize: number
  /** Number of buffer items above/below (or left/right) viewport */
  bufferSize: number
  /** Container size in pixels (height for vertical, width for horizontal) */
  containerSize: number
  /** Current scroll offset (scrollTop for vertical, scrollLeft for horizontal) */
  scrollOffset: number
  /** Total number of items */
  totalItems: number
}

export interface VirtualWindow {
  /** First visible item index (with buffer) */
  startIndex: number
  /** Last visible item index (exclusive, with buffer) */
  endIndex: number
  /** Number of visible items (including buffer) */
  visibleCount: number
  /** Total size of the full list (height or width) */
  totalSize: number
  /** Offset for the visible window (translateY or translateX) */
  offset: number
}

/**
 * Calculates the virtual window for rendering.
 * Returns the range of items to render and positioning info.
 */
export function calculateVirtualWindow(config: VirtualScrollConfig): VirtualWindow {
  const { itemSize, bufferSize, containerSize, scrollOffset, totalItems } = config

  // Calculate the first visible item (before buffer)
  const firstVisibleIndex = Math.floor(scrollOffset / itemSize)

  // Apply buffer before
  const startIndex = Math.max(0, firstVisibleIndex - bufferSize)

  // Calculate how many items fit in the container
  const itemsInView = Math.ceil(containerSize / itemSize)

  // Total visible count including buffer on both sides
  const visibleCount = itemsInView + bufferSize * 2

  // End index (clamped to total items)
  const endIndex = Math.min(startIndex + visibleCount, totalItems)

  // Total scrollable size
  const totalSize = totalItems * itemSize

  // Offset to position the visible window
  const offset = startIndex * itemSize

  return {
    startIndex,
    endIndex,
    visibleCount: endIndex - startIndex,
    totalSize,
    offset,
  }
}

/**
 * Calculates the scroll position needed to bring an item into view.
 * Returns undefined if the item is already visible.
 */
export function getScrollToPosition(
  index: number,
  itemSize: number,
  scrollOffset: number,
  containerSize: number,
): number | undefined {
  const itemTop = index * itemSize
  const itemBottom = itemTop + itemSize
  const viewportBottom = scrollOffset + containerSize

  if (itemTop < scrollOffset) {
    // Item is above viewport - scroll up
    return itemTop
  }

  if (itemBottom > viewportBottom) {
    // Item is below viewport - scroll down to show it
    return itemBottom - containerSize
  }

  // Item is already visible
  return undefined
}

/**
 * Variable-size variant of `calculateVirtualWindow`.
 *
 * Where the uniform version derives positions from `index * itemSize`, this
 * variant takes a `prefixSums` array of length `totalItems + 1` where
 * `prefixSums[i]` is the cumulative size of items `[0..i)`. That makes
 * `prefixSums[0] = 0` and `prefixSums[totalItems]` the total content size.
 *
 * Used by BriefList's shrink-wrapped columns where each column has its own
 * measured width. FullList still uses the uniform `calculateVirtualWindow`.
 */
export function calculateVirtualWindowVariable(
  prefixSums: number[],
  bufferSize: number,
  containerSize: number,
  scrollOffset: number,
  totalItems: number,
): VirtualWindow {
  if (prefixSums.length !== totalItems + 1) {
    throw new Error(
      `calculateVirtualWindowVariable: prefixSums.length (${String(prefixSums.length)}) must equal totalItems + 1 (${String(totalItems + 1)})`,
    )
  }

  if (totalItems === 0) {
    return {
      startIndex: 0,
      endIndex: 0,
      visibleCount: 0,
      totalSize: 0,
      offset: 0,
    }
  }

  const totalSize = prefixSums[totalItems]

  // Binary search for the largest i in [0, totalItems) such that prefixSums[i] <= scrollOffset.
  // That's the first column whose left edge is at or before the viewport's left edge — i.e. the
  // first visible column (it may extend right into the viewport even if it starts before it).
  let lo = 0
  let hi = totalItems
  while (lo < hi) {
    const mid = (lo + hi + 1) >>> 1
    if (prefixSums[mid] <= scrollOffset) {
      lo = mid
    } else {
      hi = mid - 1
    }
  }
  const firstVisibleIndex = lo

  // Walk forward from firstVisibleIndex to find the smallest j >= firstVisibleIndex such that
  // prefixSums[j] >= scrollOffset + containerSize. That's the exclusive end of the visible range.
  // For typical layouts (a few visible columns), linear walk is faster than binary search.
  const viewportEnd = scrollOffset + containerSize
  let lastVisibleEnd = firstVisibleIndex
  while (lastVisibleEnd < totalItems && prefixSums[lastVisibleEnd] < viewportEnd) {
    lastVisibleEnd++
  }

  // Apply buffer on both sides — clamp independently so the right-edge buffer expansion is not
  // limited by the (possibly clamped) left-edge buffer. This is the off-by-buffer bug guard:
  // computing end as `startIndex + visibleCount + 2 * bufferSize` would underestimate the right
  // edge whenever `firstVisibleIndex - bufferSize` clamps to 0, because the "lost" left buffer
  // would shrink the right buffer too.
  const startIndex = Math.max(0, firstVisibleIndex - bufferSize)
  const endIndex = Math.min(totalItems, lastVisibleEnd + bufferSize)

  return {
    startIndex,
    endIndex,
    visibleCount: endIndex - startIndex,
    totalSize,
    offset: prefixSums[startIndex],
  }
}

/**
 * Variable-size variant of `getScrollToPosition`.
 *
 * Returns the scroll offset that brings item `index` into view, or `undefined` if it's already
 * fully visible. Uses `prefixSums[index]` for the left edge and `prefixSums[index + 1]` for the
 * right edge.
 */
export function getScrollToPositionVariable(
  prefixSums: number[],
  index: number,
  scrollOffset: number,
  containerSize: number,
): number | undefined {
  if (index < 0 || index >= prefixSums.length - 1) {
    throw new Error(
      `getScrollToPositionVariable: index ${String(index)} out of range [0, ${String(prefixSums.length - 1)})`,
    )
  }

  const itemLeft = prefixSums[index]
  const itemRight = prefixSums[index + 1]
  const viewportRight = scrollOffset + containerSize

  if (itemLeft < scrollOffset) {
    // Item is off-left - scroll so its left edge aligns with the viewport's left edge.
    return itemLeft
  }

  if (itemRight > viewportRight) {
    // Item is off-right - scroll so its right edge aligns with the viewport's right edge.
    return itemRight - containerSize
  }

  // Item is fully visible.
  return undefined
}
