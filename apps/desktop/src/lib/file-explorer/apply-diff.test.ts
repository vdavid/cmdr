import { describe, it, expect } from 'vitest'
import { applyDiff } from './apply-diff'
import { createFileEntry } from './test-helpers'
import type { FileEntry, DiffChange } from './types'

// Helper to create a file entry quickly
function file(name: string, isDirectory = false): FileEntry {
    return createFileEntry({
        name,
        path: `/test/${name}`,
        isDirectory,
    })
}

// Helper to create ".." parent entry
function parentEntry(): FileEntry {
    return createFileEntry({
        name: '..',
        path: '/parent',
        isDirectory: true,
    })
}

describe('applyDiff', () => {
    describe('basic operations', () => {
        it('adds a file at the correct sorted position', () => {
            const files = [file('a.txt'), file('c.txt')]
            const changes: DiffChange[] = [{ type: 'add', entry: file('b.txt') }]

            const newIndex = applyDiff(files, 0, changes)

            expect(files.map((f) => f.name)).toEqual(['a.txt', 'b.txt', 'c.txt'])
            expect(newIndex).toBe(0) // Cursor stayed on a.txt
        })

        it('removes a file', () => {
            const files = [file('a.txt'), file('b.txt'), file('c.txt')]
            const changes: DiffChange[] = [{ type: 'remove', entry: file('b.txt') }]

            const newIndex = applyDiff(files, 0, changes)

            expect(files.map((f) => f.name)).toEqual(['a.txt', 'c.txt'])
            expect(newIndex).toBe(0)
        })

        it('modifies a file', () => {
            const original = file('a.txt')
            original.size = 100
            const files = [original]
            const modified = file('a.txt')
            modified.size = 200
            const changes: DiffChange[] = [{ type: 'modify', entry: modified }]

            const newIndex = applyDiff(files, 0, changes)

            expect(files[0].size).toBe(200)
            expect(newIndex).toBe(0)
        })
    })

    describe('cursor preservation - single file operations', () => {
        it('keeps cursor on same file when file added before cursor', () => {
            const files = [file('c.txt'), file('d.txt')]
            const changes: DiffChange[] = [{ type: 'add', entry: file('b.txt') }]

            const newIndex = applyDiff(files, 1, changes) // cursor on d.txt

            expect(files.map((f) => f.name)).toEqual(['b.txt', 'c.txt', 'd.txt'])
            expect(newIndex).toBe(2) // d.txt is now at index 2
        })

        it('keeps cursor on same file when file added after cursor', () => {
            const files = [file('a.txt'), file('b.txt')]
            const changes: DiffChange[] = [{ type: 'add', entry: file('c.txt') }]

            const newIndex = applyDiff(files, 0, changes) // cursor on a.txt

            expect(files.map((f) => f.name)).toEqual(['a.txt', 'b.txt', 'c.txt'])
            expect(newIndex).toBe(0) // a.txt still at index 0
        })

        it('keeps cursor on same file when file removed before cursor', () => {
            const files = [file('a.txt'), file('b.txt'), file('c.txt')]
            const changes: DiffChange[] = [{ type: 'remove', entry: file('a.txt') }]

            const newIndex = applyDiff(files, 2, changes) // cursor on c.txt

            expect(files.map((f) => f.name)).toEqual(['b.txt', 'c.txt'])
            expect(newIndex).toBe(1) // c.txt is now at index 1
        })

        it('keeps cursor on same file when file removed after cursor', () => {
            const files = [file('a.txt'), file('b.txt'), file('c.txt')]
            const changes: DiffChange[] = [{ type: 'remove', entry: file('c.txt') }]

            const newIndex = applyDiff(files, 0, changes) // cursor on a.txt

            expect(files.map((f) => f.name)).toEqual(['a.txt', 'b.txt'])
            expect(newIndex).toBe(0) // a.txt still at index 0
        })

        it('resets cursor to 0 when selected file is removed', () => {
            const files = [file('a.txt'), file('b.txt'), file('c.txt')]
            const changes: DiffChange[] = [{ type: 'remove', entry: file('b.txt') }]

            const newIndex = applyDiff(files, 1, changes) // cursor on b.txt

            expect(files.map((f) => f.name)).toEqual(['a.txt', 'c.txt'])
            expect(newIndex).toBe(0) // b.txt deleted, reset to 0
        })
    })

    describe('cursor preservation - multi-file diffs', () => {
        it('keeps cursor on same file after multiple adds and removes', () => {
            const files = [file('b.txt'), file('d.txt'), file('f.txt')]
            // Cursor on d.txt (index 1)
            const changes: DiffChange[] = [
                { type: 'add', entry: file('a.txt') }, // before d.txt
                { type: 'add', entry: file('c.txt') }, // before d.txt
                { type: 'add', entry: file('e.txt') }, // after d.txt
                { type: 'remove', entry: file('b.txt') }, // before d.txt
            ]

            const newIndex = applyDiff(files, 1, changes) // cursor on d.txt

            expect(files.map((f) => f.name)).toEqual(['a.txt', 'c.txt', 'd.txt', 'e.txt', 'f.txt'])
            expect(newIndex).toBe(2) // d.txt is now at index 2
        })

        it('handles large batch of changes (simulating git pull)', () => {
            const files = [file('keep1.txt'), file('keep2.txt'), file('remove1.txt'), file('remove2.txt')]
            const changes: DiffChange[] = [
                { type: 'remove', entry: file('remove1.txt') },
                { type: 'remove', entry: file('remove2.txt') },
                { type: 'add', entry: file('add1.txt') },
                { type: 'add', entry: file('add2.txt') },
                { type: 'add', entry: file('add3.txt') },
            ]

            const newIndex = applyDiff(files, 1, changes) // cursor on keep2.txt

            expect(files.map((f) => f.name)).toEqual(['add1.txt', 'add2.txt', 'add3.txt', 'keep1.txt', 'keep2.txt'])
            expect(newIndex).toBe(4) // keep2.txt is now at index 4
        })

        it('resets cursor to 0 when selected file is one of many removed', () => {
            const files = [file('a.txt'), file('b.txt'), file('c.txt')]
            const changes: DiffChange[] = [
                { type: 'add', entry: file('d.txt') },
                { type: 'remove', entry: file('b.txt') }, // this is the selected one
                { type: 'remove', entry: file('c.txt') },
            ]

            const newIndex = applyDiff(files, 1, changes) // cursor on b.txt

            expect(files.map((f) => f.name)).toEqual(['a.txt', 'd.txt'])
            expect(newIndex).toBe(0) // b.txt deleted
        })
    })

    describe('sort order', () => {
        it('maintains directories before files', () => {
            const files = [file('aDir', true), file('zDir', true), file('aFile.txt')]
            const changes: DiffChange[] = [
                { type: 'add', entry: file('mDir', true) },
                { type: 'add', entry: file('mFile.txt') },
            ]

            applyDiff(files, 0, changes)

            expect(files.map((f) => f.name)).toEqual(['aDir', 'mDir', 'zDir', 'aFile.txt', 'mFile.txt'])
        })

        it('keeps ".." at the top always', () => {
            const files = [parentEntry(), file('a.txt')]
            const changes: DiffChange[] = [{ type: 'add', entry: file('0-first.txt') }]

            applyDiff(files, 0, changes)

            expect(files.map((f) => f.name)).toEqual(['..', '0-first.txt', 'a.txt'])
        })
    })

    describe('edge cases', () => {
        it('handles empty file list with add', () => {
            const files: FileEntry[] = []
            const changes: DiffChange[] = [{ type: 'add', entry: file('first.txt') }]

            const newIndex = applyDiff(files, 0, changes)

            expect(files.map((f) => f.name)).toEqual(['first.txt'])
            expect(newIndex).toBe(0)
        })

        it('handles removing last file leaving empty list', () => {
            const files = [file('only.txt')]
            const changes: DiffChange[] = [{ type: 'remove', entry: file('only.txt') }]

            const newIndex = applyDiff(files, 0, changes)

            expect(files).toEqual([])
            expect(newIndex).toBe(0) // Safe fallback
        })

        it('handles removing non-existent file (no-op)', () => {
            const files = [file('a.txt'), file('b.txt')]
            const changes: DiffChange[] = [{ type: 'remove', entry: file('nonexistent.txt') }]

            const newIndex = applyDiff(files, 1, changes)

            expect(files.map((f) => f.name)).toEqual(['a.txt', 'b.txt'])
            expect(newIndex).toBe(1)
        })

        it('handles modifying non-existent file (no-op)', () => {
            const files = [file('a.txt')]
            const modified = file('nonexistent.txt')
            modified.size = 999
            const changes: DiffChange[] = [{ type: 'modify', entry: modified }]

            const newIndex = applyDiff(files, 0, changes)

            expect(files.length).toBe(1)
            expect(files[0].name).toBe('a.txt')
            expect(newIndex).toBe(0)
        })

        it('handles selectedIndex beyond array bounds', () => {
            const files = [file('a.txt')]
            const changes: DiffChange[] = [{ type: 'add', entry: file('b.txt') }]

            // Invalid index should return 0 since selectedPath is undefined
            const newIndex = applyDiff(files, 999, changes)

            expect(newIndex).toBe(0)
        })
    })

    describe('hidden files interaction', () => {
        // Note: Hidden file filtering happens AFTER applyDiff in the component
        // This test verifies the raw list is correct before filtering
        it('adds hidden files in correct alphabetical position', () => {
            const files = [file('a.txt'), file('b.txt')]
            const changes: DiffChange[] = [{ type: 'add', entry: file('.hidden') }]

            applyDiff(files, 0, changes)

            // Hidden files sort alphabetically with others (filtering is separate)
            expect(files.map((f) => f.name)).toEqual(['.hidden', 'a.txt', 'b.txt'])
        })
    })
})
