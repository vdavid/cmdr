import { describe, it, expect } from 'vitest'
import {
    generateDeleteTitle,
    abbreviatePath,
    countSymlinks,
    getSymlinkNotice,
    MAX_VISIBLE_ITEMS,
    type DeleteSourceItem,
} from './delete-dialog-utils'

function makeItem(overrides: Partial<DeleteSourceItem> = {}): DeleteSourceItem {
    return { name: 'file.txt', size: 1024, isDirectory: false, isSymlink: false, ...overrides }
}

describe('generateDeleteTitle', () => {
    describe('cursor item (isFromCursor = true)', () => {
        it('returns file title for a single file', () => {
            const items = [makeItem()]
            expect(generateDeleteTitle(items, true)).toBe('Delete 1 file under cursor')
        })

        it('returns folder title for a single folder', () => {
            const items = [makeItem({ name: 'components', isDirectory: true })]
            expect(generateDeleteTitle(items, true)).toBe('Delete 1 folder under cursor')
        })
    })

    describe('selected items (isFromCursor = false)', () => {
        it('returns singular file correctly', () => {
            const items = [makeItem()]
            expect(generateDeleteTitle(items, false)).toBe('Delete 1 selected file')
        })

        it('returns plural files correctly', () => {
            const items = [makeItem({ name: 'a.txt' }), makeItem({ name: 'b.txt' })]
            expect(generateDeleteTitle(items, false)).toBe('Delete 2 selected files')
        })

        it('returns singular folder correctly', () => {
            const items = [makeItem({ name: 'src', isDirectory: true })]
            expect(generateDeleteTitle(items, false)).toBe('Delete 1 folder')
        })

        it('returns plural folders correctly', () => {
            const items = [makeItem({ name: 'src', isDirectory: true }), makeItem({ name: 'dist', isDirectory: true })]
            expect(generateDeleteTitle(items, false)).toBe('Delete 2 folders')
        })

        it('combines files and folders', () => {
            const items = [
                makeItem({ name: 'a.txt' }),
                makeItem({ name: 'b.txt' }),
                makeItem({ name: 'c.txt' }),
                makeItem({ name: 'src', isDirectory: true }),
            ]
            expect(generateDeleteTitle(items, false)).toBe('Delete 3 selected files and 1 folder')
        })

        it('returns "Delete" for empty items', () => {
            expect(generateDeleteTitle([], false)).toBe('Delete')
        })
    })
})

describe('abbreviatePath', () => {
    it('abbreviates /Users/xxx paths to ~', () => {
        expect(abbreviatePath('/Users/john/Documents')).toBe('~/Documents')
    })

    it('handles /Users/xxx root', () => {
        expect(abbreviatePath('/Users/john')).toBe('~')
    })

    it('passes through non-user paths', () => {
        expect(abbreviatePath('/Volumes/NAS/projects')).toBe('/Volumes/NAS/projects')
    })

    it('passes through root path', () => {
        expect(abbreviatePath('/')).toBe('/')
    })
})

describe('countSymlinks', () => {
    it('returns 0 for no symlinks', () => {
        expect(countSymlinks([makeItem(), makeItem({ name: 'dir', isDirectory: true })])).toBe(0)
    })

    it('counts symlinks correctly', () => {
        expect(
            countSymlinks([makeItem({ isSymlink: true }), makeItem(), makeItem({ name: 'link', isSymlink: true })]),
        ).toBe(2)
    })
})

describe('getSymlinkNotice', () => {
    it('returns null when no symlinks', () => {
        expect(getSymlinkNotice([makeItem()])).toBeNull()
    })

    it('returns singular notice for single symlink only item', () => {
        const notice = getSymlinkNotice([makeItem({ isSymlink: true })])
        expect(notice).toBe('This item is a symlink. Only the link will be deleted, not its target.')
    })

    it('returns plural notice for multiple symlinks', () => {
        const notice = getSymlinkNotice([
            makeItem({ isSymlink: true }),
            makeItem(),
            makeItem({ name: 'other', isSymlink: true }),
        ])
        expect(notice).toContain('2 symlinks')
        expect(notice).toContain('links themselves')
    })

    it('returns plural notice for one symlink among many items', () => {
        const notice = getSymlinkNotice([makeItem({ isSymlink: true }), makeItem()])
        expect(notice).toContain('1 symlink')
        expect(notice).toContain('links themselves')
    })
})

describe('MAX_VISIBLE_ITEMS', () => {
    it('is 10', () => {
        expect(MAX_VISIBLE_ITEMS).toBe(10)
    })
})
