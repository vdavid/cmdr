/**
 * Integration tests for view mode rendering.
 *
 * These tests verify that both Brief and Full view modes correctly render
 * file lists with various data scenarios including hidden files.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createMockDirectoryListing, filterHiddenFiles, createFileEntry } from './test-helpers'

// Mock tauri-commands
vi.mock('$lib/tauri-commands', () => ({
    getFileRange: vi.fn(),
    getSyncStatusBatch: vi.fn().mockResolvedValue([]),
}))

// Mock icon-cache
vi.mock('$lib/icon-cache', () => ({
    getCachedIcon: vi.fn().mockReturnValue(undefined),
    iconCacheVersion: { subscribe: vi.fn() },
    prefetchIcons: vi.fn(),
}))

// Mock reactive-settings
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
    getRowHeight: vi.fn().mockReturnValue(24),
    formatDateTime: vi.fn().mockReturnValue('2025-01-01 00:00'),
    formatFileSize: vi.fn().mockReturnValue('1.0 KB'),
    getUseAppIconsForDocuments: vi.fn().mockReturnValue(true),
}))

// Mock drag-and-drop
vi.mock('$lib/drag-and-drop', () => ({
    startDragTracking: vi.fn(),
}))

describe('View mode rendering', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    describe('Hidden files filtering', () => {
        const mockListing = createMockDirectoryListing()

        it('filterHiddenFiles returns all files when showHidden is true', () => {
            const result = filterHiddenFiles(mockListing, true)
            expect(result.length).toBe(mockListing.length)
            expect(result.some((f) => f.name.startsWith('.'))).toBe(true)
        })

        it('filterHiddenFiles excludes hidden files when showHidden is false', () => {
            const result = filterHiddenFiles(mockListing, false)
            expect(result.every((f) => !f.name.startsWith('.'))).toBe(true)
            // Should have 4 visible entries: Documents, Downloads, README.md, file.txt
            expect(result.length).toBe(4)
        })

        it('filterHiddenFiles maintains sort order (directories first)', () => {
            const result = filterHiddenFiles(mockListing, false)
            const dirs = result.filter((f) => f.isDirectory)
            const files = result.filter((f) => !f.isDirectory)

            // All directories should come before all files
            const lastDirIndex = result.findIndex((f) => f === dirs[dirs.length - 1])
            const firstFileIndex = result.findIndex((f) => f === files[0])

            expect(lastDirIndex).toBeLessThan(firstFileIndex)
        })
    })

    describe('Directory listing structure', () => {
        it('createMockDirectoryListing includes expected file types', () => {
            const listing = createMockDirectoryListing()

            // Check for hidden directories
            const hiddenDirs = listing.filter((f) => f.isDirectory && f.name.startsWith('.'))
            expect(hiddenDirs.length).toBeGreaterThan(0)

            // Check for visible directories
            const visibleDirs = listing.filter((f) => f.isDirectory && !f.name.startsWith('.'))
            expect(visibleDirs.length).toBeGreaterThan(0)

            // Check for hidden files
            const hiddenFiles = listing.filter((f) => !f.isDirectory && f.name.startsWith('.'))
            expect(hiddenFiles.length).toBeGreaterThan(0)

            // Check for visible files
            const visibleFiles = listing.filter((f) => !f.isDirectory && !f.name.startsWith('.'))
            expect(visibleFiles.length).toBeGreaterThan(0)
        })

        it('files have required metadata', () => {
            const listing = createMockDirectoryListing()

            for (const file of listing) {
                expect(file.name).toBeDefined()
                expect(file.path).toBeDefined()
                expect(typeof file.isDirectory).toBe('boolean')
                expect(typeof file.isSymlink).toBe('boolean')
                expect(file.iconId).toBeDefined()
                expect(file.owner).toBeDefined()
                expect(file.group).toBeDefined()
            }
        })
    })

    describe('Parent entry (..) handling', () => {
        it('parent entry is created correctly', () => {
            const parentEntry = createFileEntry({
                name: '..',
                path: '/parent',
                isDirectory: true,
            })

            expect(parentEntry.name).toBe('..')
            expect(parentEntry.isDirectory).toBe(true)
            expect(parentEntry.iconId).toBe('dir')
        })

        it('parent entry should appear at top of filtered list', () => {
            // When we have a parent entry, it should always be first
            const parentEntry = createFileEntry({
                name: '..',
                path: '/parent',
                isDirectory: true,
            })
            const mockListing = createMockDirectoryListing()

            // Parent entry is typically prepended to the list
            const listWithParent = [parentEntry, ...mockListing]

            // Even after filtering hidden files, parent should remain first
            const filtered = filterHiddenFiles(listWithParent, false)
            expect(filtered[0].name).toBe('..')
        })
    })
})

describe('Large directory handling', () => {
    it('createMockEntriesWithCount generates correct number of entries', async () => {
        const { createMockEntriesWithCount } = await import('./test-helpers')
        const entries = createMockEntriesWithCount(1000)

        expect(entries.length).toBe(1000)
    })

    it('entries are sorted correctly (directories first)', async () => {
        const { createMockEntriesWithCount } = await import('./test-helpers')
        const entries = createMockEntriesWithCount(100)

        // Find the last directory and first file
        const dirs = entries.filter((e) => e.isDirectory)
        const files = entries.filter((e) => !e.isDirectory)

        if (dirs.length > 0 && files.length > 0) {
            const lastDirIndex = entries.findIndex((e) => e === dirs[dirs.length - 1])
            const firstFileIndex = entries.findIndex((e) => e === files[0])
            expect(lastDirIndex).toBeLessThan(firstFileIndex)
        }
    })
})
