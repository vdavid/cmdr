/**
 * Tests for transfer dialog utility functions
 */
import { describe, it, expect } from 'vitest'
import { generateTitle, getFolderName, toBackendIndices, toBackendCursorIndex } from './transfer-dialog-utils'

describe('generateTitle', () => {
    describe('copy operation', () => {
        it('returns "Copy" for zero files and folders', () => {
            expect(generateTitle('copy', 0, 0)).toBe('Copy')
        })

        it('returns singular file correctly', () => {
            expect(generateTitle('copy', 1, 0)).toBe('Copy 1 file')
        })

        it('returns plural files correctly', () => {
            expect(generateTitle('copy', 2, 0)).toBe('Copy 2 files')
            expect(generateTitle('copy', 10, 0)).toBe('Copy 10 files')
            expect(generateTitle('copy', 100, 0)).toBe('Copy 100 files')
        })

        it('returns singular folder correctly', () => {
            expect(generateTitle('copy', 0, 1)).toBe('Copy 1 folder')
        })

        it('returns plural folders correctly', () => {
            expect(generateTitle('copy', 0, 2)).toBe('Copy 2 folders')
            expect(generateTitle('copy', 0, 10)).toBe('Copy 10 folders')
        })

        it('combines files and folders with "and"', () => {
            expect(generateTitle('copy', 1, 1)).toBe('Copy 1 file and 1 folder')
            expect(generateTitle('copy', 2, 3)).toBe('Copy 2 files and 3 folders')
            expect(generateTitle('copy', 5, 1)).toBe('Copy 5 files and 1 folder')
            expect(generateTitle('copy', 1, 5)).toBe('Copy 1 file and 5 folders')
        })

        it('handles large numbers', () => {
            expect(generateTitle('copy', 1000, 500)).toBe('Copy 1000 files and 500 folders')
        })
    })

    describe('move operation', () => {
        it('returns "Move" for zero files and folders', () => {
            expect(generateTitle('move', 0, 0)).toBe('Move')
        })

        it('returns singular file correctly', () => {
            expect(generateTitle('move', 1, 0)).toBe('Move 1 file')
        })

        it('returns plural files correctly', () => {
            expect(generateTitle('move', 2, 0)).toBe('Move 2 files')
        })

        it('returns singular folder correctly', () => {
            expect(generateTitle('move', 0, 1)).toBe('Move 1 folder')
        })

        it('combines files and folders with "and"', () => {
            expect(generateTitle('move', 2, 3)).toBe('Move 2 files and 3 folders')
        })
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
            expect(toBackendIndices([1, 2, 3], true)).toEqual([0, 1, 2])
        })

        it('filters out index 0 (the ".." entry)', () => {
            expect(toBackendIndices([0, 1, 2], true)).toEqual([0, 1])
        })

        it('handles the last file correctly (the original bug)', () => {
            expect(toBackendIndices([5], true)).toEqual([4])
        })

        it('handles selecting all files (excluding "..")', () => {
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
