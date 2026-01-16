/**
 * Tests for keyboard-shortcuts.ts
 */
import { describe, it, expect } from 'vitest'
import { handleNavigationShortcut, type NavigationContext } from './keyboard-shortcuts'

/** Helper to create a mock keyboard event */
function createKeyboardEvent(key: string, options: { altKey?: boolean; metaKey?: boolean } = {}): KeyboardEvent {
    return {
        key,
        altKey: options.altKey ?? false,
        metaKey: options.metaKey ?? false,
        ctrlKey: false,
        shiftKey: false,
    } as KeyboardEvent
}

describe('handleNavigationShortcut', () => {
    describe('Home shortcut', () => {
        const context: NavigationContext = {
            currentIndex: 50,
            totalCount: 100,
        }

        it('handles Option+ArrowUp as Home', () => {
            const event = createKeyboardEvent('ArrowUp', { altKey: true })
            const result = handleNavigationShortcut(event, context)
            expect(result).toEqual({ newIndex: 0, handled: true })
        })

        it('handles Home key', () => {
            const event = createKeyboardEvent('Home')
            const result = handleNavigationShortcut(event, context)
            expect(result).toEqual({ newIndex: 0, handled: true })
        })

        it('does not handle Home with metaKey', () => {
            const event = createKeyboardEvent('Home', { metaKey: true })
            const result = handleNavigationShortcut(event, context)
            expect(result).toBeNull()
        })
    })

    describe('End shortcut', () => {
        const context: NavigationContext = {
            currentIndex: 50,
            totalCount: 100,
        }

        it('handles Option+ArrowDown as End', () => {
            const event = createKeyboardEvent('ArrowDown', { altKey: true })
            const result = handleNavigationShortcut(event, context)
            expect(result).toEqual({ newIndex: 99, handled: true })
        })

        it('handles End key', () => {
            const event = createKeyboardEvent('End')
            const result = handleNavigationShortcut(event, context)
            expect(result).toEqual({ newIndex: 99, handled: true })
        })

        it('does not handle End with metaKey', () => {
            const event = createKeyboardEvent('End', { metaKey: true })
            const result = handleNavigationShortcut(event, context)
            expect(result).toBeNull()
        })

        it('handles End with empty list', () => {
            const emptyContext: NavigationContext = {
                currentIndex: 0,
                totalCount: 0,
            }
            const event = createKeyboardEvent('End')
            const result = handleNavigationShortcut(event, emptyContext)
            expect(result).toEqual({ newIndex: 0, handled: true })
        })
    })

    describe('PageUp in Full mode', () => {
        it('moves up by visible items', () => {
            const context: NavigationContext = {
                currentIndex: 50,
                totalCount: 100,
                visibleItems: 20,
            }
            const event = createKeyboardEvent('PageUp')
            const result = handleNavigationShortcut(event, context)
            // pageSize = max(1, 20 - 1) = 19
            expect(result).toEqual({ newIndex: 31, handled: true })
        })

        it('uses default page size when visibleItems not provided', () => {
            const context: NavigationContext = {
                currentIndex: 50,
                totalCount: 100,
            }
            const event = createKeyboardEvent('PageUp')
            const result = handleNavigationShortcut(event, context)
            // pageSize defaults to 20
            expect(result).toEqual({ newIndex: 30, handled: true })
        })

        it('clamps to 0', () => {
            const context: NavigationContext = {
                currentIndex: 5,
                totalCount: 100,
                visibleItems: 20,
            }
            const event = createKeyboardEvent('PageUp')
            const result = handleNavigationShortcut(event, context)
            expect(result).toEqual({ newIndex: 0, handled: true })
        })
    })

    describe('PageDown in Full mode', () => {
        it('moves down by visible items', () => {
            const context: NavigationContext = {
                currentIndex: 50,
                totalCount: 100,
                visibleItems: 20,
            }
            const event = createKeyboardEvent('PageDown')
            const result = handleNavigationShortcut(event, context)
            // pageSize = max(1, 20 - 1) = 19
            expect(result).toEqual({ newIndex: 69, handled: true })
        })

        it('uses default page size when visibleItems not provided', () => {
            const context: NavigationContext = {
                currentIndex: 50,
                totalCount: 100,
            }
            const event = createKeyboardEvent('PageDown')
            const result = handleNavigationShortcut(event, context)
            // pageSize defaults to 20
            expect(result).toEqual({ newIndex: 70, handled: true })
        })

        it('clamps to totalCount - 1', () => {
            const context: NavigationContext = {
                currentIndex: 95,
                totalCount: 100,
                visibleItems: 20,
            }
            const event = createKeyboardEvent('PageDown')
            const result = handleNavigationShortcut(event, context)
            expect(result).toEqual({ newIndex: 99, handled: true })
        })
    })

    describe('PageUp in Brief mode', () => {
        // Brief mode: items arranged in columns
        // itemsPerColumn = 10, visibleColumns = 3
        // Layout: Col0[0-9], Col1[10-19], Col2[20-29], Col3[30-39], etc.

        it('moves left by visible columns minus 1', () => {
            const context: NavigationContext = {
                currentIndex: 35, // Column 3, row 5
                totalCount: 50,
                itemsPerColumn: 10,
                visibleColumns: 3,
            }
            const event = createKeyboardEvent('PageUp')
            const result = handleNavigationShortcut(event, context)
            // columnsToMove = max(1, 3 - 1) = 2
            // currentColumn = 3, targetColumn = 1
            // targetColumnStart = 10, bottommost = min(49, 10 + 10 - 1) = 19
            expect(result).toEqual({ newIndex: 19, handled: true })
        })

        it('jumps to first item when near start', () => {
            const context: NavigationContext = {
                currentIndex: 5, // Column 0, row 5
                totalCount: 50,
                itemsPerColumn: 10,
                visibleColumns: 3,
            }
            const event = createKeyboardEvent('PageUp')
            const result = handleNavigationShortcut(event, context)
            // targetColumn would be -2, which is <= 0, so jump to 0
            expect(result).toEqual({ newIndex: 0, handled: true })
        })

        it('jumps to first item from column 1', () => {
            const context: NavigationContext = {
                currentIndex: 15, // Column 1, row 5
                totalCount: 50,
                itemsPerColumn: 10,
                visibleColumns: 3,
            }
            const event = createKeyboardEvent('PageUp')
            const result = handleNavigationShortcut(event, context)
            // targetColumn = 1 - 2 = -1, which is <= 0
            expect(result).toEqual({ newIndex: 0, handled: true })
        })
    })

    describe('PageDown in Brief mode', () => {
        it('moves right by visible columns minus 1', () => {
            const context: NavigationContext = {
                currentIndex: 15, // Column 1, row 5
                totalCount: 50,
                itemsPerColumn: 10,
                visibleColumns: 3,
            }
            const event = createKeyboardEvent('PageDown')
            const result = handleNavigationShortcut(event, context)
            // columnsToMove = max(1, 3 - 1) = 2
            // currentColumn = 1, targetColumn = 3
            // totalColumns = ceil(50/10) = 5, so 3 < 4 (last column index)
            // targetColumnStart = 30, bottommost = min(49, 30 + 10 - 1) = 39
            expect(result).toEqual({ newIndex: 39, handled: true })
        })

        it('jumps to last item when near end', () => {
            const context: NavigationContext = {
                currentIndex: 35, // Column 3, row 5
                totalCount: 50,
                itemsPerColumn: 10,
                visibleColumns: 3,
            }
            const event = createKeyboardEvent('PageDown')
            const result = handleNavigationShortcut(event, context)
            // totalColumns = 5, targetColumn = 3 + 2 = 5 >= 4 (last column index)
            expect(result).toEqual({ newIndex: 49, handled: true })
        })

        it('handles partial last column', () => {
            const context: NavigationContext = {
                currentIndex: 25, // Column 2, row 5
                totalCount: 45, // Last column only has 5 items
                itemsPerColumn: 10,
                visibleColumns: 3,
            }
            const event = createKeyboardEvent('PageDown')
            const result = handleNavigationShortcut(event, context)
            // totalColumns = ceil(45/10) = 5, targetColumn = 2 + 2 = 4 (last column)
            // 4 >= 4, so jump to last item
            expect(result).toEqual({ newIndex: 44, handled: true })
        })
    })

    describe('unhandled keys', () => {
        const context: NavigationContext = {
            currentIndex: 50,
            totalCount: 100,
        }

        it('returns null for ArrowUp without modifiers', () => {
            const event = createKeyboardEvent('ArrowUp')
            const result = handleNavigationShortcut(event, context)
            expect(result).toBeNull()
        })

        it('returns null for ArrowDown without modifiers', () => {
            const event = createKeyboardEvent('ArrowDown')
            const result = handleNavigationShortcut(event, context)
            expect(result).toBeNull()
        })

        it('returns null for Enter', () => {
            const event = createKeyboardEvent('Enter')
            const result = handleNavigationShortcut(event, context)
            expect(result).toBeNull()
        })

        it('returns null for letter keys', () => {
            const event = createKeyboardEvent('a')
            const result = handleNavigationShortcut(event, context)
            expect(result).toBeNull()
        })
    })
})
