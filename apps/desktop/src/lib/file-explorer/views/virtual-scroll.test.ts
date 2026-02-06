/**
 * Tests for virtual-scroll.ts
 */
import { describe, it, expect } from 'vitest'
import { calculateVirtualWindow, getScrollToPosition, type VirtualScrollConfig } from './virtual-scroll'

describe('calculateVirtualWindow', () => {
    const baseConfig: VirtualScrollConfig = {
        direction: 'vertical',
        itemSize: 20,
        bufferSize: 5,
        containerSize: 400, // 20 items visible
        scrollOffset: 0,
        totalItems: 1000,
    }

    describe('basic calculations', () => {
        it('calculates correct start index at scroll position 0', () => {
            const result = calculateVirtualWindow(baseConfig)
            expect(result.startIndex).toBe(0)
        })

        it('calculates correct end index for viewport', () => {
            const result = calculateVirtualWindow(baseConfig)
            // 400px / 20px = 20 items visible + 5 buffer on each side = 30
            // But starts at 0 so no buffer before
            // End should be 0 + 20 (visible) + 5 (buffer) = 25
            expect(result.endIndex).toBeLessThanOrEqual(30)
            expect(result.endIndex).toBeGreaterThan(20)
        })

        it('returns correct visibleCount', () => {
            const result = calculateVirtualWindow(baseConfig)
            expect(result.visibleCount).toBe(result.endIndex - result.startIndex)
        })

        it('calculates correct totalSize', () => {
            const result = calculateVirtualWindow(baseConfig)
            expect(result.totalSize).toBe(1000 * 20)
        })

        it('calculates correct offset at scroll position 0', () => {
            const result = calculateVirtualWindow(baseConfig)
            expect(result.offset).toBe(0)
        })
    })

    describe('scrolled positions', () => {
        it('calculates correct range when scrolled partway', () => {
            const config = { ...baseConfig, scrollOffset: 200 } // Scrolled 10 items
            const result = calculateVirtualWindow(config)

            // First visible index = 200/20 = 10
            // startIndex = 10 - 5 = 5
            expect(result.startIndex).toBe(5)
        })

        it('applies buffer correctly when scrolled', () => {
            const config = { ...baseConfig, scrollOffset: 500 } // Scrolled 25 items
            const result = calculateVirtualWindow(config)

            // First visible index = 500/20 = 25
            // startIndex = 25 - 5 = 20
            expect(result.startIndex).toBe(20)
            // endIndex = 20 + 20 (visible) + 10 (buffer both sides) = 50 or clamped
            expect(result.endIndex).toBeGreaterThan(40)
        })

        it('calculates correct offset when scrolled', () => {
            const config = { ...baseConfig, scrollOffset: 500 }
            const result = calculateVirtualWindow(config)

            // offset = startIndex * itemSize
            expect(result.offset).toBe(result.startIndex * 20)
        })
    })

    describe('edge cases', () => {
        it('handles empty list', () => {
            const config = { ...baseConfig, totalItems: 0 }
            const result = calculateVirtualWindow(config)

            expect(result.startIndex).toBe(0)
            expect(result.endIndex).toBe(0)
            expect(result.visibleCount).toBe(0)
            expect(result.totalSize).toBe(0)
        })

        it('handles list smaller than viewport', () => {
            const config = { ...baseConfig, totalItems: 5 } // Only 5 items
            const result = calculateVirtualWindow(config)

            expect(result.startIndex).toBe(0)
            expect(result.endIndex).toBe(5)
            expect(result.visibleCount).toBe(5)
        })

        it('clamps startIndex to 0 with large buffer', () => {
            const config = { ...baseConfig, bufferSize: 20, scrollOffset: 100 }
            const result = calculateVirtualWindow(config)

            // First visible = 100/20 = 5
            // With buffer of 20, would be 5 - 20 = -15, clamped to 0
            expect(result.startIndex).toBe(0)
        })

        it('clamps endIndex to totalItems', () => {
            const config = { ...baseConfig, scrollOffset: 19500, totalItems: 1000 }
            const result = calculateVirtualWindow(config)

            expect(result.endIndex).toBe(1000)
        })

        it('handles scrollOffset near end of list', () => {
            const config = { ...baseConfig, scrollOffset: 19800, totalItems: 1000 }
            const result = calculateVirtualWindow(config)

            // First visible = 19800/20 = 990
            // startIndex = 990 - 5 = 985
            expect(result.startIndex).toBe(985)
            expect(result.endIndex).toBe(1000)
        })

        it('handles fractional scroll positions', () => {
            const config = { ...baseConfig, scrollOffset: 155 } // Not aligned to item size
            const result = calculateVirtualWindow(config)

            // First visible = floor(155/20) = 7
            // startIndex = 7 - 5 = 2
            expect(result.startIndex).toBe(2)
        })

        it('handles very large item size', () => {
            const config = { ...baseConfig, itemSize: 500, containerSize: 400 }
            const result = calculateVirtualWindow(config)

            // itemsInView = ceil(400/500) = 1
            // visibleCount = 1 + 5*2 = 11
            expect(result.visibleCount).toBeLessThanOrEqual(11)
        })

        it('handles buffer size of 0', () => {
            const config = { ...baseConfig, bufferSize: 0 }
            const result = calculateVirtualWindow(config)

            // No buffer means exactly the visible items
            expect(result.startIndex).toBe(0)
            // itemsInView = ceil(400/20) = 20
            expect(result.endIndex).toBe(20)
        })
    })

    describe('horizontal scrolling', () => {
        it('works the same for horizontal direction', () => {
            const config: VirtualScrollConfig = {
                direction: 'horizontal',
                itemSize: 150, // Column width
                bufferSize: 2,
                containerSize: 600, // 4 columns visible
                scrollOffset: 300, // 2 columns scrolled
                totalItems: 100,
            }
            const result = calculateVirtualWindow(config)

            // First visible = floor(300/150) = 2
            // startIndex = 2 - 2 = 0
            expect(result.startIndex).toBe(0)

            // itemsInView = ceil(600/150) = 4
            // visibleCount = 4 + 2*2 = 8
            // endIndex = 0 + 8 = 8
            expect(result.endIndex).toBe(8)
        })
    })
})

describe('getScrollToPosition', () => {
    const itemSize = 20
    const containerSize = 400

    describe('item is visible', () => {
        it('returns undefined when item is in middle of viewport', () => {
            // Scrolled to 200 (items 10-30 visible)
            const result = getScrollToPosition(15, itemSize, 200, containerSize)
            expect(result).toBeUndefined()
        })

        it('returns undefined when item is at top of viewport', () => {
            // Scrolled to 200, item 10 is at top
            const result = getScrollToPosition(10, itemSize, 200, containerSize)
            expect(result).toBeUndefined()
        })

        it('returns undefined when item is at bottom of viewport', () => {
            // Scrolled to 200, item 29 has bottom at 600 which equals viewport bottom
            const result = getScrollToPosition(29, itemSize, 200, containerSize)
            expect(result).toBeUndefined()
        })
    })

    describe('item is above viewport', () => {
        it('returns scroll position to show item at top', () => {
            // Scrolled to 400 (items 20-40 visible), item 10 is above
            const result = getScrollToPosition(10, itemSize, 400, containerSize)
            expect(result).toBe(200) // Item 10 starts at 200
        })

        it('returns 0 for first item when scrolled', () => {
            const result = getScrollToPosition(0, itemSize, 400, containerSize)
            expect(result).toBe(0)
        })
    })

    describe('item is below viewport', () => {
        it('returns scroll position to show item at bottom', () => {
            // Scrolled to 0 (items 0-20 visible), item 30 is below
            const result = getScrollToPosition(30, itemSize, 0, containerSize)
            // Item 30 bottom is at 620, need to scroll so viewport bottom = 620
            // scrollOffset = 620 - 400 = 220
            expect(result).toBe(220)
        })

        it('scrolls to show last item at bottom', () => {
            // Last item (index 99) when scrolled to 0
            const result = getScrollToPosition(99, itemSize, 0, containerSize)
            // Item 99 bottom is at 2000, scroll so viewport bottom = 2000
            // scrollOffset = 2000 - 400 = 1600
            expect(result).toBe(1600)
        })
    })

    describe('edge cases', () => {
        it('handles item size of 1', () => {
            const result = getScrollToPosition(500, 1, 0, 100)
            // Item 500 bottom is 501, need scroll = 501 - 100 = 401
            expect(result).toBe(401)
        })

        it('handles large item that fills viewport', () => {
            const largeItemSize = 400 // Same as container
            const result = getScrollToPosition(5, largeItemSize, 0, containerSize)
            // Item 5 is at 2000-2400
            // Bottom = 2400, viewport at 400, need scroll = 2400 - 400 = 2000
            expect(result).toBe(2000)
        })

        it('handles container larger than total content', () => {
            // Small list with large container
            const result = getScrollToPosition(5, 20, 0, 1000)
            // Item 5 bottom is 120, viewport bottom is 1000
            // Item is visible
            expect(result).toBeUndefined()
        })

        it('handles scroll position exactly at item boundary', () => {
            // Scroll offset exactly at item 10 start
            const result = getScrollToPosition(10, itemSize, 200, containerSize)
            expect(result).toBeUndefined()
        })

        it('returns scroll position when item is exactly one pixel above', () => {
            // Item 9 ends at 200, viewport starts at 201
            const result = getScrollToPosition(9, itemSize, 201, containerSize)
            // Item 9 starts at 180
            expect(result).toBe(180)
        })

        it('returns scroll position when item is exactly one pixel below', () => {
            // Item 30 starts at 600, viewport ends at 599
            const result = getScrollToPosition(30, itemSize, 199, containerSize)
            // Item 30 bottom is 620, scroll = 620 - 400 = 220
            expect(result).toBe(220)
        })
    })
})
