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
