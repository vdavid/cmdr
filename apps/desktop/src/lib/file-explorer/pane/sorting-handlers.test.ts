import { describe, it, expect } from 'vitest'
import { getNewSortOrder, toBackendIndices, toFrontendIndices } from './sorting-handlers'

describe('getNewSortOrder', () => {
    describe('clicking the same column', () => {
        it('toggles ascending to descending', () => {
            expect(getNewSortOrder('name', 'name', 'ascending')).toBe('descending')
            expect(getNewSortOrder('extension', 'extension', 'ascending')).toBe('descending')
            expect(getNewSortOrder('size', 'size', 'ascending')).toBe('descending')
            expect(getNewSortOrder('modified', 'modified', 'ascending')).toBe('descending')
            expect(getNewSortOrder('created', 'created', 'ascending')).toBe('descending')
        })

        it('toggles descending to ascending', () => {
            expect(getNewSortOrder('name', 'name', 'descending')).toBe('ascending')
            expect(getNewSortOrder('extension', 'extension', 'descending')).toBe('ascending')
            expect(getNewSortOrder('size', 'size', 'descending')).toBe('ascending')
            expect(getNewSortOrder('modified', 'modified', 'descending')).toBe('ascending')
            expect(getNewSortOrder('created', 'created', 'descending')).toBe('ascending')
        })
    })

    describe('clicking a different column', () => {
        it('uses ascending for name column', () => {
            expect(getNewSortOrder('name', 'size', 'descending')).toBe('ascending')
            expect(getNewSortOrder('name', 'modified', 'descending')).toBe('ascending')
        })

        it('uses ascending for extension column', () => {
            expect(getNewSortOrder('extension', 'name', 'ascending')).toBe('ascending')
            expect(getNewSortOrder('extension', 'size', 'descending')).toBe('ascending')
        })

        it('uses descending for size column', () => {
            expect(getNewSortOrder('size', 'name', 'ascending')).toBe('descending')
            expect(getNewSortOrder('size', 'extension', 'ascending')).toBe('descending')
        })

        it('uses descending for modified column', () => {
            expect(getNewSortOrder('modified', 'name', 'ascending')).toBe('descending')
            expect(getNewSortOrder('modified', 'size', 'descending')).toBe('descending')
        })

        it('uses descending for created column', () => {
            expect(getNewSortOrder('created', 'name', 'ascending')).toBe('descending')
            expect(getNewSortOrder('created', 'modified', 'descending')).toBe('descending')
        })

        it('ignores current order when switching columns', () => {
            expect(getNewSortOrder('name', 'size', 'ascending')).toBe('ascending')
            expect(getNewSortOrder('name', 'size', 'descending')).toBe('ascending')
            expect(getNewSortOrder('size', 'name', 'ascending')).toBe('descending')
            expect(getNewSortOrder('size', 'name', 'descending')).toBe('descending')
        })
    })
})

describe('toBackendIndices', () => {
    it('returns indices unchanged when no parent', () => {
        expect(toBackendIndices([0, 1, 2], false)).toEqual([0, 1, 2])
    })

    it('shifts indices down by 1 when hasParent', () => {
        expect(toBackendIndices([1, 2, 3], true)).toEqual([0, 1, 2])
    })

    it('filters out index 0 when hasParent (the ".." entry)', () => {
        expect(toBackendIndices([0, 1, 2], true)).toEqual([0, 1])
    })

    it('handles empty array', () => {
        expect(toBackendIndices([], true)).toEqual([])
        expect(toBackendIndices([], false)).toEqual([])
    })

    it('filters index 0 and shifts remaining when hasParent', () => {
        expect(toBackendIndices([0, 3, 5], true)).toEqual([2, 4])
    })
})

describe('toFrontendIndices', () => {
    it('returns indices unchanged when no parent', () => {
        expect(toFrontendIndices([0, 1, 2], false)).toEqual([0, 1, 2])
    })

    it('shifts indices up by 1 when hasParent', () => {
        expect(toFrontendIndices([0, 1, 2], true)).toEqual([1, 2, 3])
    })

    it('handles empty array', () => {
        expect(toFrontendIndices([], true)).toEqual([])
        expect(toFrontendIndices([], false)).toEqual([])
    })

    it('handles single element', () => {
        expect(toFrontendIndices([5], true)).toEqual([6])
        expect(toFrontendIndices([5], false)).toEqual([5])
    })
})
