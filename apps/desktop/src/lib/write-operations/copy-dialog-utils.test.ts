/**
 * Tests for copy dialog utility functions
 */
import { describe, it, expect } from 'vitest'
import { generateTitle, getFolderName } from './copy-dialog-utils'

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
