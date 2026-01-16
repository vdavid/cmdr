/**
 * Utility functions for BriefList component.
 * Extracted for testability.
 */

/** Calculates new index for arrow key navigation in Brief mode */
export function handleArrowKeyNavigation(
    key: string,
    cursorIndex: number,
    totalCount: number,
    itemsPerColumn: number,
): number | undefined {
    if (key === 'ArrowUp') {
        return Math.max(0, cursorIndex - 1)
    }
    if (key === 'ArrowDown') {
        return Math.min(totalCount - 1, cursorIndex + 1)
    }
    if (key === 'ArrowLeft') {
        const newIndex = cursorIndex - itemsPerColumn
        return newIndex >= 0 ? newIndex : 0
    }
    if (key === 'ArrowRight') {
        const newIndex = cursorIndex + itemsPerColumn
        return newIndex < totalCount ? newIndex : totalCount - 1
    }
    return undefined
}

/** Calculates layout parameters for Brief mode */
export function calculateBriefLayout(
    containerHeight: number,
    containerWidth: number,
    totalCount: number,
    backendMaxWidth: number | undefined,
    rowHeight: number = 20,
    minColumnWidth: number = 100,
    columnPadding: number = 42, // icon + gaps + padding + buffer
): {
    itemsPerColumn: number
    columnWidth: number
    totalColumns: number
} {
    const itemsPerColumn = Math.max(1, Math.floor(containerHeight / rowHeight))

    // Calculate column width
    const estimatedWidth = Math.min(200, Math.max(minColumnWidth, containerWidth / 3))
    const calculatedWidth = (backendMaxWidth ?? estimatedWidth) + columnPadding

    // Cap to container width
    const columnWidth = containerWidth > 0 ? Math.min(calculatedWidth, containerWidth) : calculatedWidth

    // Total columns needed
    const totalColumns = Math.ceil(totalCount / itemsPerColumn)

    return { itemsPerColumn, columnWidth, totalColumns }
}

/** Calculates which column an index is in */
export function getColumnForIndex(index: number, itemsPerColumn: number): number {
    return Math.floor(index / itemsPerColumn)
}

/** Calculates the item range for a given column range */
export function getItemRangeForColumns(
    startColumn: number,
    endColumn: number,
    itemsPerColumn: number,
    totalCount: number,
): { startItem: number; endItem: number } {
    const startItem = startColumn * itemsPerColumn
    const endItem = Math.min(endColumn * itemsPerColumn, totalCount)
    return { startItem, endItem }
}

/** Double-click detection helper */
export function isDoubleClick(
    lastClickTime: number,
    lastClickIndex: number,
    currentIndex: number,
    currentTime: number,
    doubleClickMs: number = 300,
): boolean {
    return lastClickIndex === currentIndex && currentTime - lastClickTime < doubleClickMs
}
