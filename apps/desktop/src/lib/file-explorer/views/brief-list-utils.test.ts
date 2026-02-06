/**
 * Tests for brief-list-utils.ts
 */
import { describe, it, expect } from 'vitest'
import {
    handleArrowKeyNavigation,
    calculateBriefLayout,
    getColumnForIndex,
    getItemRangeForColumns,
    isDoubleClick,
} from './brief-list-utils'

describe('handleArrowKeyNavigation', () => {
    const totalCount = 100
    const itemsPerColumn = 20

    describe('ArrowUp', () => {
        it('moves up one item', () => {
            expect(handleArrowKeyNavigation('ArrowUp', 5, totalCount, itemsPerColumn)).toBe(4)
        })

        it('clamps at 0', () => {
            expect(handleArrowKeyNavigation('ArrowUp', 0, totalCount, itemsPerColumn)).toBe(0)
        })

        it('moves from 1 to 0', () => {
            expect(handleArrowKeyNavigation('ArrowUp', 1, totalCount, itemsPerColumn)).toBe(0)
        })
    })

    describe('ArrowDown', () => {
        it('moves down one item', () => {
            expect(handleArrowKeyNavigation('ArrowDown', 5, totalCount, itemsPerColumn)).toBe(6)
        })

        it('clamps at totalCount - 1', () => {
            expect(handleArrowKeyNavigation('ArrowDown', 99, totalCount, itemsPerColumn)).toBe(99)
        })

        it('moves to last item', () => {
            expect(handleArrowKeyNavigation('ArrowDown', 98, totalCount, itemsPerColumn)).toBe(99)
        })
    })

    describe('ArrowLeft', () => {
        it('moves left by one column', () => {
            expect(handleArrowKeyNavigation('ArrowLeft', 25, totalCount, itemsPerColumn)).toBe(5)
        })

        it('clamps at 0 when would go negative', () => {
            expect(handleArrowKeyNavigation('ArrowLeft', 5, totalCount, itemsPerColumn)).toBe(0)
        })

        it('returns 0 from first column', () => {
            expect(handleArrowKeyNavigation('ArrowLeft', 10, totalCount, itemsPerColumn)).toBe(0)
        })

        it('moves from exact column boundary', () => {
            expect(handleArrowKeyNavigation('ArrowLeft', 40, totalCount, itemsPerColumn)).toBe(20)
        })
    })

    describe('ArrowRight', () => {
        it('moves right by one column', () => {
            expect(handleArrowKeyNavigation('ArrowRight', 5, totalCount, itemsPerColumn)).toBe(25)
        })

        it('clamps at totalCount - 1 when would exceed', () => {
            expect(handleArrowKeyNavigation('ArrowRight', 85, totalCount, itemsPerColumn)).toBe(99)
        })

        it('clamps when in last column', () => {
            expect(handleArrowKeyNavigation('ArrowRight', 95, totalCount, itemsPerColumn)).toBe(99)
        })

        it('moves from exact column boundary', () => {
            expect(handleArrowKeyNavigation('ArrowRight', 20, totalCount, itemsPerColumn)).toBe(40)
        })
    })

    describe('unhandled keys', () => {
        it('returns undefined for Enter', () => {
            expect(handleArrowKeyNavigation('Enter', 5, totalCount, itemsPerColumn)).toBeUndefined()
        })

        it('returns undefined for letter keys', () => {
            expect(handleArrowKeyNavigation('a', 5, totalCount, itemsPerColumn)).toBeUndefined()
        })

        it('returns undefined for Home', () => {
            // Home is handled by keyboard-shortcuts.ts, not arrow navigation
            expect(handleArrowKeyNavigation('Home', 5, totalCount, itemsPerColumn)).toBeUndefined()
        })
    })

    describe('edge cases', () => {
        it('handles single item list', () => {
            expect(handleArrowKeyNavigation('ArrowUp', 0, 1, 10)).toBe(0)
            expect(handleArrowKeyNavigation('ArrowDown', 0, 1, 10)).toBe(0)
            expect(handleArrowKeyNavigation('ArrowLeft', 0, 1, 10)).toBe(0)
            expect(handleArrowKeyNavigation('ArrowRight', 0, 1, 10)).toBe(0)
        })

        it('handles itemsPerColumn = 1', () => {
            // With 1 item per column, Left/Right act like Up/Down
            expect(handleArrowKeyNavigation('ArrowLeft', 5, 10, 1)).toBe(4)
            expect(handleArrowKeyNavigation('ArrowRight', 5, 10, 1)).toBe(6)
        })

        it('handles large itemsPerColumn', () => {
            // All items in one column
            expect(handleArrowKeyNavigation('ArrowLeft', 50, 100, 100)).toBe(0)
            expect(handleArrowKeyNavigation('ArrowRight', 50, 100, 100)).toBe(99)
        })
    })
})

describe('calculateBriefLayout', () => {
    it('calculates itemsPerColumn based on height and row height', () => {
        const result = calculateBriefLayout(400, 600, 100, undefined)
        expect(result.itemsPerColumn).toBe(20) // 400 / 20 = 20
    })

    it('ensures itemsPerColumn is at least 1', () => {
        const result = calculateBriefLayout(10, 600, 100, undefined)
        expect(result.itemsPerColumn).toBe(1) // floor(10/20) = 0, clamped to 1
    })

    it('uses backend width when provided', () => {
        const result = calculateBriefLayout(400, 600, 100, 150)
        // 150 + 42 (padding) = 192
        expect(result.columnWidth).toBe(192)
    })

    it('estimates width when backend width not provided', () => {
        const result = calculateBriefLayout(400, 600, 100, undefined)
        // Estimated: min(200, max(100, 600/3)) = min(200, 200) = 200
        // + padding 42 = 242
        expect(result.columnWidth).toBe(242)
    })

    it('caps column width to container width', () => {
        const result = calculateBriefLayout(400, 150, 100, 200)
        // 200 + 42 = 242, but capped to 150
        expect(result.columnWidth).toBe(150)
    })

    it('calculates total columns', () => {
        const result = calculateBriefLayout(400, 600, 100, undefined)
        // 100 items / 20 per column = 5 columns
        expect(result.totalColumns).toBe(5)
    })

    it('rounds up total columns', () => {
        const result = calculateBriefLayout(400, 600, 105, undefined)
        // 105 items / 20 per column = 5.25, rounded up = 6
        expect(result.totalColumns).toBe(6)
    })

    it('handles custom row height', () => {
        const result = calculateBriefLayout(400, 600, 100, undefined, 40)
        expect(result.itemsPerColumn).toBe(10) // 400 / 40 = 10
    })

    it('handles custom min column width', () => {
        const result = calculateBriefLayout(400, 150, 100, undefined, 20, 200)
        // Estimated: min(200, max(200, 50)) = 200, + 42 = 242, capped to 150
        expect(result.columnWidth).toBe(150)
    })

    it('handles zero container width gracefully', () => {
        const result = calculateBriefLayout(400, 0, 100, 150)
        // When containerWidth is 0, column width is not capped
        expect(result.columnWidth).toBe(192) // 150 + 42
    })
})

describe('getColumnForIndex', () => {
    it('returns 0 for indices in first column', () => {
        expect(getColumnForIndex(0, 20)).toBe(0)
        expect(getColumnForIndex(19, 20)).toBe(0)
    })

    it('returns 1 for indices in second column', () => {
        expect(getColumnForIndex(20, 20)).toBe(1)
        expect(getColumnForIndex(39, 20)).toBe(1)
    })

    it('handles itemsPerColumn = 1', () => {
        expect(getColumnForIndex(5, 1)).toBe(5)
    })

    it('handles large itemsPerColumn', () => {
        expect(getColumnForIndex(50, 100)).toBe(0)
    })
})

describe('getItemRangeForColumns', () => {
    it('returns correct range for single column', () => {
        const result = getItemRangeForColumns(0, 1, 20, 100)
        expect(result).toEqual({ startItem: 0, endItem: 20 })
    })

    it('returns correct range for multiple columns', () => {
        const result = getItemRangeForColumns(1, 3, 20, 100)
        expect(result).toEqual({ startItem: 20, endItem: 60 })
    })

    it('clamps endItem to totalCount', () => {
        const result = getItemRangeForColumns(4, 6, 20, 100)
        // Would be 80-120, but clamped to 100
        expect(result).toEqual({ startItem: 80, endItem: 100 })
    })

    it('handles last partial column', () => {
        const result = getItemRangeForColumns(4, 5, 20, 95)
        // Column 4 starts at 80, column 5 would end at 100, but total is 95
        expect(result).toEqual({ startItem: 80, endItem: 95 })
    })
})

describe('isDoubleClick', () => {
    it('returns true for same index within time limit', () => {
        expect(isDoubleClick(1000, 5, 5, 1200, 300)).toBe(true)
    })

    it('returns false for different index', () => {
        expect(isDoubleClick(1000, 5, 6, 1200, 300)).toBe(false)
    })

    it('returns false when time exceeds limit', () => {
        expect(isDoubleClick(1000, 5, 5, 1400, 300)).toBe(false)
    })

    it('returns false when time exactly at limit', () => {
        expect(isDoubleClick(1000, 5, 5, 1300, 300)).toBe(false)
    })

    it('returns true for very fast double click', () => {
        expect(isDoubleClick(1000, 5, 5, 1050, 300)).toBe(true)
    })

    it('uses default timeout of 300ms', () => {
        expect(isDoubleClick(1000, 5, 5, 1299)).toBe(true)
        expect(isDoubleClick(1000, 5, 5, 1300)).toBe(false)
    })

    it('handles custom timeout', () => {
        expect(isDoubleClick(1000, 5, 5, 1499, 500)).toBe(true)
        expect(isDoubleClick(1000, 5, 5, 1500, 500)).toBe(false)
    })
})
