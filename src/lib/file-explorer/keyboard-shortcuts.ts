/**
 * Keyboard shortcut handling for file lists
 *
 * Provides centralized logic for handling keyboard navigation shortcuts
 * across different view modes (Brief and Full).
 */

export interface NavigationResult {
    /** The new index to select */
    newIndex: number
    /** Whether the event was handled */
    handled: boolean
}

export interface NavigationContext {
    currentIndex: number
    totalCount: number
    /** For Brief mode: items per column */
    itemsPerColumn?: number
    /** For Brief mode: number of visible columns (for PageUp/PageDown) */
    visibleColumns?: number
    /** For Full mode: number of visible items (for PageUp/PageDown) */
    visibleItems?: number
}

/** Helper: Handle Page Up key in Brief mode (horizontal navigation) */
function handleBriefPageUp(
    currentIndex: number,
    totalCount: number,
    itemsPerColumn: number,
    visibleColumns: number,
): number {
    const columnsToMove = Math.max(1, visibleColumns - 1)
    const currentColumn = Math.floor(currentIndex / itemsPerColumn)
    const targetColumn = currentColumn - columnsToMove

    // If we'd go to or past the leftmost column, jump to first item
    if (targetColumn <= 0) {
        return 0
    }

    // Otherwise, go to the bottommost item in the target column
    const targetColumnStart = targetColumn * itemsPerColumn
    return Math.min(totalCount - 1, targetColumnStart + itemsPerColumn - 1)
}

/** Helper: Handle Page Down key in Brief mode (horizontal navigation) */
function handleBriefPageDown(
    currentIndex: number,
    totalCount: number,
    itemsPerColumn: number,
    visibleColumns: number,
): number {
    const columnsToMove = Math.max(1, visibleColumns - 1)
    const currentColumn = Math.floor(currentIndex / itemsPerColumn)
    const totalColumns = Math.ceil(totalCount / itemsPerColumn)
    const targetColumn = currentColumn + columnsToMove

    // If we'd go to or past the rightmost column, jump to last item
    if (targetColumn >= totalColumns - 1) {
        return totalCount - 1
    }

    // Otherwise, go to the bottommost item in the target column
    const targetColumnStart = targetColumn * itemsPerColumn
    return Math.min(totalCount - 1, targetColumnStart + itemsPerColumn - 1)
}

/** Helper: Handle Page Up/Down in Full mode (vertical navigation) */
function handleFullPageNavigation(
    currentIndex: number,
    totalCount: number,
    visibleItems: number | undefined,
    isPageDown: boolean,
): number {
    const pageSize = visibleItems ? Math.max(1, visibleItems - 1) : 20
    if (isPageDown) {
        return Math.min(totalCount - 1, currentIndex + pageSize)
    }
    return Math.max(0, currentIndex - pageSize)
}

/**
 * Checks if the event is a Home shortcut (Option+Up or Fn+Left/Home).
 */
function isHomeShortcut(event: KeyboardEvent): boolean {
    return (event.altKey && event.key === 'ArrowUp') || (event.key === 'Home' && !event.metaKey)
}

/**
 * Checks if the event is an End shortcut (Option+Down or Fn+Right/End).
 */
function isEndShortcut(event: KeyboardEvent): boolean {
    return (event.altKey && event.key === 'ArrowDown') || (event.key === 'End' && !event.metaKey)
}

/**
 * Handles keyboard navigation shortcuts for file lists.
 * Returns the new index and whether the event was handled.
 */
export function handleNavigationShortcut(event: KeyboardEvent, context: NavigationContext): NavigationResult | null {
    const { currentIndex, totalCount, itemsPerColumn, visibleColumns, visibleItems } = context

    // Home shortcut (Option+Up or Fn+Left)
    if (isHomeShortcut(event)) {
        return { newIndex: 0, handled: true }
    }

    // End shortcut (Option+Down or Fn+Right)
    if (isEndShortcut(event)) {
        return { newIndex: Math.max(0, totalCount - 1), handled: true }
    }

    const isBriefMode = visibleColumns !== undefined && itemsPerColumn !== undefined

    // Page Up
    if (event.key === 'PageUp') {
        const newIndex = isBriefMode
            ? handleBriefPageUp(currentIndex, totalCount, itemsPerColumn, visibleColumns)
            : handleFullPageNavigation(currentIndex, totalCount, visibleItems, false)
        return { newIndex, handled: true }
    }

    // Page Down
    if (event.key === 'PageDown') {
        const newIndex = isBriefMode
            ? handleBriefPageDown(currentIndex, totalCount, itemsPerColumn, visibleColumns)
            : handleFullPageNavigation(currentIndex, totalCount, visibleItems, true)
        return { newIndex, handled: true }
    }

    // Not a handled shortcut
    return null
}
