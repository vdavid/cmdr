/**
 * Tests for file-list-utils.ts
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
    getSyncIconPath,
    createParentEntry,
    getEntryAt,
    getFallbackEmoji,
    calculateFetchRange,
    isRangeCached,
    shouldResetCache,
    PREFETCH_BUFFER,
    fetchVisibleRange,
} from './file-list-utils'
import type { FileEntry } from './types'

// Mock dependencies
vi.mock('$lib/tauri-commands', () => ({
    getFileRange: vi.fn(),
}))
vi.mock('$lib/icon-cache', () => ({
    prefetchIcons: vi.fn(),
}))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
    getUseAppIconsForDocuments: vi.fn().mockReturnValue(true),
}))

import { getFileRange } from '$lib/tauri-commands'

describe('getSyncIconPath', () => {
    it('returns undefined for undefined status', () => {
        expect(getSyncIconPath(undefined)).toBeUndefined()
    })

    it('returns correct icon for synced status', () => {
        expect(getSyncIconPath('synced')).toBe('/icons/sync-synced.svg')
    })

    it('returns correct icon for online_only status', () => {
        expect(getSyncIconPath('online_only')).toBe('/icons/sync-online-only.svg')
    })

    it('returns correct icon for uploading status', () => {
        expect(getSyncIconPath('uploading')).toBe('/icons/sync-uploading.svg')
    })

    it('returns correct icon for downloading status', () => {
        expect(getSyncIconPath('downloading')).toBe('/icons/sync-downloading.svg')
    })

    it('returns undefined for unknown status', () => {
        expect(getSyncIconPath('unknown')).toBeUndefined()
    })
})

describe('createParentEntry', () => {
    it('creates parent entry with correct name', () => {
        const entry = createParentEntry('/home/user')
        expect(entry.name).toBe('..')
    })

    it('creates parent entry with correct path', () => {
        const entry = createParentEntry('/home/user')
        expect(entry.path).toBe('/home/user')
    })

    it('creates parent entry as directory', () => {
        const entry = createParentEntry('/home/user')
        expect(entry.isDirectory).toBe(true)
    })

    it('creates parent entry with correct icon', () => {
        const entry = createParentEntry('/home/user')
        expect(entry.iconId).toBe('dir')
    })

    it('creates parent entry with extendedMetadataLoaded', () => {
        const entry = createParentEntry('/home/user')
        expect(entry.extendedMetadataLoaded).toBe(true)
    })
})

describe('getEntryAt', () => {
    const mockEntries: FileEntry[] = [
        {
            name: 'file1.txt',
            path: '/dir/file1.txt',
            isDirectory: false,
            isSymlink: false,
            permissions: 0o644,
            owner: 'user',
            group: 'group',
            iconId: 'txt',
            extendedMetadataLoaded: true,
        },
        {
            name: 'file2.txt',
            path: '/dir/file2.txt',
            isDirectory: false,
            isSymlink: false,
            permissions: 0o644,
            owner: 'user',
            group: 'group',
            iconId: 'txt',
            extendedMetadataLoaded: true,
        },
    ]

    it('returns parent entry at index 0 when hasParent is true', () => {
        const entry = getEntryAt(0, true, '/parent', mockEntries, { start: 0, end: 2 })
        expect(entry?.name).toBe('..')
        expect(entry?.path).toBe('/parent')
    })

    it('returns first cached entry at index 0 when hasParent is false', () => {
        const entry = getEntryAt(0, false, '/parent', mockEntries, { start: 0, end: 2 })
        expect(entry?.name).toBe('file1.txt')
    })

    it('returns cached entry at index 1 when hasParent is true', () => {
        const entry = getEntryAt(1, true, '/parent', mockEntries, { start: 0, end: 2 })
        expect(entry?.name).toBe('file1.txt')
    })

    it('returns undefined for index outside cached range', () => {
        const entry = getEntryAt(5, false, '/parent', mockEntries, { start: 0, end: 2 })
        expect(entry).toBeUndefined()
    })

    it('returns undefined for negative index', () => {
        const entry = getEntryAt(-1, false, '/parent', mockEntries, { start: 0, end: 2 })
        expect(entry).toBeUndefined()
    })

    it('handles cached range that does not start at 0', () => {
        const entry = getEntryAt(10, false, '/parent', mockEntries, { start: 10, end: 12 })
        expect(entry?.name).toBe('file1.txt')
    })
})

describe('getFallbackEmoji', () => {
    it('returns link emoji for symlinks', () => {
        const file: FileEntry = {
            name: 'link',
            path: '/link',
            isDirectory: false,
            isSymlink: true,
            permissions: 0o777,
            owner: 'user',
            group: 'group',
            iconId: 'link',
            extendedMetadataLoaded: true,
        }
        expect(getFallbackEmoji(file)).toBe('🔗')
    })

    it('returns folder emoji for directories', () => {
        const file: FileEntry = {
            name: 'dir',
            path: '/dir',
            isDirectory: true,
            isSymlink: false,
            permissions: 0o755,
            owner: 'user',
            group: 'group',
            iconId: 'dir',
            extendedMetadataLoaded: true,
        }
        expect(getFallbackEmoji(file)).toBe('📁')
    })

    it('returns document emoji for regular files', () => {
        const file: FileEntry = {
            name: 'file.txt',
            path: '/file.txt',
            isDirectory: false,
            isSymlink: false,
            permissions: 0o644,
            owner: 'user',
            group: 'group',
            iconId: 'txt',
            extendedMetadataLoaded: true,
        }
        expect(getFallbackEmoji(file)).toBe('📄')
    })

    it('prioritizes symlink over directory', () => {
        const file: FileEntry = {
            name: 'link-to-dir',
            path: '/link-to-dir',
            isDirectory: true,
            isSymlink: true,
            permissions: 0o777,
            owner: 'user',
            group: 'group',
            iconId: 'link',
            extendedMetadataLoaded: true,
        }
        expect(getFallbackEmoji(file)).toBe('🔗')
    })
})

describe('calculateFetchRange', () => {
    it('calculates range without parent entry', () => {
        const result = calculateFetchRange({
            startItem: 150,
            endItem: 160,
            hasParent: false,
            totalCount: 500,
        })
        // PREFETCH_BUFFER is 200, so buffer is 100 on each side
        expect(result.fetchStart).toBe(150 - PREFETCH_BUFFER / 2) // 50
        expect(result.fetchEnd).toBe(160 + PREFETCH_BUFFER / 2) // 260
    })

    it('calculates range with parent entry', () => {
        const result = calculateFetchRange({
            startItem: 150,
            endItem: 160,
            hasParent: true,
            totalCount: 500,
        })
        // With parent, indices are shifted down by 1
        expect(result.fetchStart).toBe(149 - PREFETCH_BUFFER / 2) // 49
        expect(result.fetchEnd).toBe(159 + PREFETCH_BUFFER / 2) // 259
    })

    it('clamps fetchStart to 0', () => {
        const result = calculateFetchRange({
            startItem: 5,
            endItem: 10,
            hasParent: false,
            totalCount: 100,
        })
        expect(result.fetchStart).toBe(0)
    })

    it('clamps fetchEnd to totalCount', () => {
        const result = calculateFetchRange({
            startItem: 90,
            endItem: 100,
            hasParent: false,
            totalCount: 100,
        })
        expect(result.fetchEnd).toBe(100)
    })

    it('handles hasParent with startItem 0', () => {
        const result = calculateFetchRange({
            startItem: 0,
            endItem: 10,
            hasParent: true,
            totalCount: 100,
        })
        expect(result.fetchStart).toBe(0)
    })
})

describe('isRangeCached', () => {
    it('returns true when range is fully cached', () => {
        expect(isRangeCached(10, 20, { start: 0, end: 50 })).toBe(true)
    })

    it('returns true when range exactly matches cache', () => {
        expect(isRangeCached(0, 50, { start: 0, end: 50 })).toBe(true)
    })

    it('returns false when fetchStart is before cache', () => {
        expect(isRangeCached(0, 20, { start: 10, end: 50 })).toBe(false)
    })

    it('returns false when fetchEnd is after cache', () => {
        expect(isRangeCached(10, 60, { start: 0, end: 50 })).toBe(false)
    })

    it('returns false when range is completely outside cache', () => {
        expect(isRangeCached(60, 80, { start: 0, end: 50 })).toBe(false)
    })
})

describe('shouldResetCache', () => {
    const base = {
        listingId: 'listing-1',
        includeHidden: false,
        totalCount: 100,
        cacheGeneration: 1,
    }

    it('returns false when all properties match', () => {
        expect(shouldResetCache(base, base)).toBe(false)
    })

    it('returns true when listingId changes', () => {
        expect(shouldResetCache({ ...base, listingId: 'listing-2' }, base)).toBe(true)
    })

    it('returns true when includeHidden changes', () => {
        expect(shouldResetCache({ ...base, includeHidden: true }, base)).toBe(true)
    })

    it('returns true when totalCount changes', () => {
        expect(shouldResetCache({ ...base, totalCount: 200 }, base)).toBe(true)
    })

    it('returns true when cacheGeneration changes', () => {
        expect(shouldResetCache({ ...base, cacheGeneration: 2 }, base)).toBe(true)
    })
})

describe('fetchVisibleRange', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    const mockEntries: FileEntry[] = [
        {
            name: 'file1.txt',
            path: '/dir/file1.txt',
            isDirectory: false,
            isSymlink: false,
            permissions: 0o644,
            owner: 'user',
            group: 'group',
            iconId: 'txt',
            extendedMetadataLoaded: true,
        },
    ]

    it('returns null when range is already cached', async () => {
        const result = await fetchVisibleRange({
            listingId: 'listing-1',
            startItem: 10,
            endItem: 20,
            hasParent: false,
            totalCount: 100,
            includeHidden: false,
            cachedRange: { start: 0, end: 200 },
        })
        expect(result).toBeNull()
        expect(getFileRange).not.toHaveBeenCalled()
    })

    it('fetches entries when range is not cached', async () => {
        vi.mocked(getFileRange).mockResolvedValue(mockEntries)

        const result = await fetchVisibleRange({
            listingId: 'listing-1',
            startItem: 10,
            endItem: 20,
            hasParent: false,
            totalCount: 100,
            includeHidden: false,
            cachedRange: { start: 0, end: 5 },
        })

        expect(result).not.toBeNull()
        expect(result?.entries).toEqual(mockEntries)
        expect(getFileRange).toHaveBeenCalled()
    })

    it('calls onSyncStatusRequest when provided', async () => {
        vi.mocked(getFileRange).mockResolvedValue(mockEntries)
        const onSyncStatusRequest = vi.fn()

        await fetchVisibleRange({
            listingId: 'listing-1',
            startItem: 10,
            endItem: 20,
            hasParent: false,
            totalCount: 100,
            includeHidden: false,
            cachedRange: { start: 0, end: 5 },
            onSyncStatusRequest,
        })

        expect(onSyncStatusRequest).toHaveBeenCalledWith(['/dir/file1.txt'])
    })
})
