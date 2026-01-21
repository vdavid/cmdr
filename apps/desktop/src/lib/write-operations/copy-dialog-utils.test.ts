/**
 * Tests for copy dialog utility functions
 */
import { describe, it, expect } from 'vitest'
import { generateTitle, getFolderName, toBackendIndices, toBackendCursorIndex } from './copy-dialog-utils'

describe('generateTitle', () => {
    it('returns "Copy" for zero files and folders', () => {
        expect(generateTitle(0, 0)).toBe('Copy')
    })

    it('returns singular file correctly', () => {
        expect(generateTitle(1, 0)).toBe('Copy 1 file')
    })

    it('returns plural files correctly', () => {
        expect(generateTitle(2, 0)).toBe('Copy 2 files')
        expect(generateTitle(10, 0)).toBe('Copy 10 files')
        expect(generateTitle(100, 0)).toBe('Copy 100 files')
    })

    it('returns singular folder correctly', () => {
        expect(generateTitle(0, 1)).toBe('Copy 1 folder')
    })

    it('returns plural folders correctly', () => {
        expect(generateTitle(0, 2)).toBe('Copy 2 folders')
        expect(generateTitle(0, 10)).toBe('Copy 10 folders')
    })

    it('combines files and folders with "and"', () => {
        expect(generateTitle(1, 1)).toBe('Copy 1 file and 1 folder')
        expect(generateTitle(2, 3)).toBe('Copy 2 files and 3 folders')
        expect(generateTitle(5, 1)).toBe('Copy 5 files and 1 folder')
        expect(generateTitle(1, 5)).toBe('Copy 1 file and 5 folders')
    })

    it('handles large numbers', () => {
        expect(generateTitle(1000, 500)).toBe('Copy 1000 files and 500 folders')
    })
})

describe('getFolderName', () => {
    it('returns "/" for root path', () => {
        expect(getFolderName('/')).toBe('/')
    })

    it('extracts folder name from simple path', () => {
        expect(getFolderName('/Users')).toBe('Users')
        expect(getFolderName('/Users/Documents')).toBe('Documents')
    })

    it('handles paths with trailing slash', () => {
        expect(getFolderName('/Users/')).toBe('Users')
        expect(getFolderName('/Users/Documents/')).toBe('Documents')
    })

    it('handles nested paths', () => {
        expect(getFolderName('/Users/john/Documents/Projects')).toBe('Projects')
        expect(getFolderName('/Volumes/External/Backup')).toBe('Backup')
    })

    it('handles single component paths', () => {
        expect(getFolderName('/home')).toBe('home')
    })

    it('handles home directory tilde expansion result', () => {
        expect(getFolderName('/Users/veszelovszki')).toBe('veszelovszki')
    })
})

describe('toBackendIndices', () => {
    describe('with hasParent=true (directory has ".." entry at index 0)', () => {
        it('adjusts indices by -1', () => {
            // Frontend [1,2,3] → backend [0,1,2]
            expect(toBackendIndices([1, 2, 3], true)).toEqual([0, 1, 2])
        })

        it('filters out index 0 (the ".." entry)', () => {
            // Frontend [0,1,2] → backend [0,1] (index 0 becomes -1 and is filtered)
            expect(toBackendIndices([0, 1, 2], true)).toEqual([0, 1])
        })

        it('handles the last file correctly (the original bug)', () => {
            // If frontend has 6 items (index 0-5) and backend has 5 (index 0-4),
            // frontend index 5 should become backend index 4
            expect(toBackendIndices([5], true)).toEqual([4])
        })

        it('handles selecting all files (excluding "..")', () => {
            // Frontend [1,2,3,4,5] → backend [0,1,2,3,4]
            expect(toBackendIndices([1, 2, 3, 4, 5], true)).toEqual([0, 1, 2, 3, 4])
        })

        it('handles empty selection', () => {
            expect(toBackendIndices([], true)).toEqual([])
        })

        it('handles selection with only ".." entry', () => {
            expect(toBackendIndices([0], true)).toEqual([])
        })
    })

    describe('with hasParent=false (at root, no ".." entry)', () => {
        it('passes through indices unchanged', () => {
            expect(toBackendIndices([0, 1, 2], false)).toEqual([0, 1, 2])
        })

        it('handles the last file correctly', () => {
            expect(toBackendIndices([4], false)).toEqual([4])
        })

        it('handles empty selection', () => {
            expect(toBackendIndices([], false)).toEqual([])
        })
    })
})

describe('toBackendCursorIndex', () => {
    describe('with hasParent=true', () => {
        it('adjusts cursor index by -1', () => {
            expect(toBackendCursorIndex(5, true)).toBe(4)
            expect(toBackendCursorIndex(1, true)).toBe(0)
        })

        it('returns null for cursor on ".." entry (index 0)', () => {
            expect(toBackendCursorIndex(0, true)).toBeNull()
        })

        it('handles the last file correctly (the original bug)', () => {
            // If frontend cursor is at index 5 with hasParent=true,
            // backend index should be 4 (not 5 which would be out of bounds)
            expect(toBackendCursorIndex(5, true)).toBe(4)
        })
    })

    describe('with hasParent=false', () => {
        it('passes through cursor index unchanged', () => {
            expect(toBackendCursorIndex(0, false)).toBe(0)
            expect(toBackendCursorIndex(4, false)).toBe(4)
        })
    })

    describe('edge cases', () => {
        it('returns null for negative index', () => {
            expect(toBackendCursorIndex(-1, false)).toBeNull()
            expect(toBackendCursorIndex(-1, true)).toBeNull()
        })
    })
})
