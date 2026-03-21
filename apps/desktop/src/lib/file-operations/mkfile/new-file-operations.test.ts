import { describe, it, expect } from 'vitest'
import { getInitialFileName } from './new-file-operations'

function makeMockGetFileAt(entry: { name: string; isDirectory: boolean } | null) {
    // eslint-disable-next-line @typescript-eslint/require-await
    return async () => entry
}

function makeMockPaneRef(cursorIndex: number, hasParent: boolean) {
    return {
        getCursorIndex: () => cursorIndex,
        hasParentEntry: () => hasParent,
    } as never
}

describe('getInitialFileName', () => {
    it('returns full filename with extension for files', async () => {
        const paneRef = makeMockPaneRef(1, true)
        const result = await getInitialFileName(
            paneRef,
            'listing-1',
            false,
            makeMockGetFileAt({ name: 'notes.txt', isDirectory: false }),
        )
        expect(result).toBe('notes.txt')
    })

    it('returns empty string for directories', async () => {
        const paneRef = makeMockPaneRef(1, true)
        const result = await getInitialFileName(
            paneRef,
            'listing-1',
            false,
            makeMockGetFileAt({ name: 'my-folder', isDirectory: true }),
        )
        expect(result).toBe('')
    })

    it('returns empty string for ".." entry (backendIndex < 0)', async () => {
        const paneRef = makeMockPaneRef(0, true)
        const result = await getInitialFileName(
            paneRef,
            'listing-1',
            false,
            makeMockGetFileAt({ name: 'anything', isDirectory: false }),
        )
        expect(result).toBe('')
    })

    it('returns empty string when paneRef is undefined', async () => {
        const result = await getInitialFileName(
            undefined,
            'listing-1',
            false,
            makeMockGetFileAt({ name: 'test.ts', isDirectory: false }),
        )
        expect(result).toBe('')
    })

    it('returns full name for dotfiles', async () => {
        const paneRef = makeMockPaneRef(1, true)
        const result = await getInitialFileName(
            paneRef,
            'listing-1',
            false,
            makeMockGetFileAt({ name: '.gitignore', isDirectory: false }),
        )
        expect(result).toBe('.gitignore')
    })

    it('handles files without extension', async () => {
        const paneRef = makeMockPaneRef(0, false)
        const result = await getInitialFileName(
            paneRef,
            'listing-1',
            false,
            makeMockGetFileAt({ name: 'Makefile', isDirectory: false }),
        )
        expect(result).toBe('Makefile')
    })
})
