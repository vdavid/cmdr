import { describe, it, expect } from 'vitest'
import { findMatches, nextMatchIndex, prevMatchIndex } from './viewer-search'

describe('findMatches', () => {
    it('returns empty array for empty query', () => {
        expect(findMatches(['hello world'], '')).toEqual([])
    })

    it('finds a single match on a single line', () => {
        const matches = findMatches(['hello world'], 'world')
        expect(matches).toEqual([{ line: 0, start: 6, length: 5 }])
    })

    it('finds multiple matches on the same line', () => {
        const matches = findMatches(['foo bar foo baz foo'], 'foo')
        expect(matches).toHaveLength(3)
        expect(matches[0]).toEqual({ line: 0, start: 0, length: 3 })
        expect(matches[1]).toEqual({ line: 0, start: 8, length: 3 })
        expect(matches[2]).toEqual({ line: 0, start: 16, length: 3 })
    })

    it('finds matches across multiple lines', () => {
        const matches = findMatches(['first line', 'second test', 'third line'], 'line')
        expect(matches).toHaveLength(2)
        expect(matches[0]).toEqual({ line: 0, start: 6, length: 4 })
        expect(matches[1]).toEqual({ line: 2, start: 6, length: 4 })
    })

    it('is case-insensitive', () => {
        const matches = findMatches(['Hello WORLD'], 'hello')
        expect(matches).toHaveLength(1)
        expect(matches[0]).toEqual({ line: 0, start: 0, length: 5 })
    })

    it('returns empty for no matches', () => {
        expect(findMatches(['hello world'], 'xyz')).toEqual([])
    })

    it('handles empty lines', () => {
        const matches = findMatches(['', 'hello', ''], 'hello')
        expect(matches).toEqual([{ line: 1, start: 0, length: 5 }])
    })

    it('handles overlapping potential matches (non-overlapping search)', () => {
        // "aaa" in "aaaa" should find at 0 and 1 (overlapping start positions)
        const matches = findMatches(['aaaa'], 'aaa')
        expect(matches).toHaveLength(2)
        expect(matches[0]).toEqual({ line: 0, start: 0, length: 3 })
        expect(matches[1]).toEqual({ line: 0, start: 1, length: 3 })
    })

    it('handles special regex characters in query', () => {
        const matches = findMatches(['price is $10.00'], '$10')
        expect(matches).toEqual([{ line: 0, start: 9, length: 3 }])
    })
})

describe('nextMatchIndex', () => {
    it('returns -1 for zero total', () => {
        expect(nextMatchIndex(0, 0)).toBe(-1)
    })

    it('advances to the next index', () => {
        expect(nextMatchIndex(0, 5)).toBe(1)
        expect(nextMatchIndex(3, 5)).toBe(4)
    })

    it('wraps around at the end', () => {
        expect(nextMatchIndex(4, 5)).toBe(0)
    })
})

describe('prevMatchIndex', () => {
    it('returns -1 for zero total', () => {
        expect(prevMatchIndex(0, 0)).toBe(-1)
    })

    it('moves to the previous index', () => {
        expect(prevMatchIndex(3, 5)).toBe(2)
        expect(prevMatchIndex(1, 5)).toBe(0)
    })

    it('wraps around at the beginning', () => {
        expect(prevMatchIndex(0, 5)).toBe(4)
    })
})
