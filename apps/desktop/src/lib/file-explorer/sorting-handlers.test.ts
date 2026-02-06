import { describe, it, expect } from 'vitest'
import { getNewSortOrder } from './sorting-handlers'

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
