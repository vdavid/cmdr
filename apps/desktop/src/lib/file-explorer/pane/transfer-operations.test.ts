import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
    getDestinationVolumeInfo,
    getSelectedFilePaths,
    buildTransferPropsFromSelection,
    buildTransferPropsFromDroppedPaths,
    getCommonParentPath,
    type TransferContext,
} from './transfer-operations'
import type { FileEntry, VolumeInfo } from '../types'

vi.mock('$lib/tauri-commands', () => ({
    getFileAt: vi.fn(),
    getListingStats: vi.fn(),
}))

const { getFileAt, getListingStats } = await import('$lib/tauri-commands')

const mockFileEntry = (
    overrides: Partial<FileEntry> & { name: string; path: string; isDirectory: boolean },
): FileEntry =>
    ({
        isSymlink: false,
        permissions: 0o755,
        owner: 'user',
        group: 'staff',
        iconId: '',
        extendedMetadataLoaded: false,
        ...overrides,
    }) as FileEntry

describe('getDestinationVolumeInfo', () => {
    const volumes: VolumeInfo[] = [
        {
            id: 'vol-1',
            name: 'Main Drive',
            path: '/mnt/main',
            category: 'main_volume',
            isEjectable: false,
            isReadOnly: false,
        },
        {
            id: 'vol-2',
            name: 'Backup',
            path: '/mnt/backup',
            category: 'attached_volume',
            isEjectable: true,
            isReadOnly: true,
        },
    ]

    const mtpVolumes = [
        { id: 'mtp-device-1', deviceId: 'device-1', name: 'Phone Storage', isReadOnly: false },
        { id: 'mtp-device-2', deviceId: 'device-2', name: 'Read-only Device', isReadOnly: true },
    ]

    it('returns info for regular volume', () => {
        expect(getDestinationVolumeInfo('vol-1', volumes, mtpVolumes)).toEqual({
            name: 'Main Drive',
            isReadOnly: false,
        })
    })

    it('returns info for read-only regular volume', () => {
        expect(getDestinationVolumeInfo('vol-2', volumes, mtpVolumes)).toEqual({ name: 'Backup', isReadOnly: true })
    })

    it('returns info for MTP volume', () => {
        expect(getDestinationVolumeInfo('mtp-device-1', volumes, mtpVolumes)).toEqual({
            name: 'Phone Storage',
            isReadOnly: false,
        })
    })

    it('returns info for read-only MTP volume', () => {
        expect(getDestinationVolumeInfo('mtp-device-2', volumes, mtpVolumes)).toEqual({
            name: 'Read-only Device',
            isReadOnly: true,
        })
    })

    it('returns undefined when not found', () => {
        expect(getDestinationVolumeInfo('nonexistent', volumes, mtpVolumes)).toBeUndefined()
    })
})

describe('getSelectedFilePaths', () => {
    beforeEach(() => vi.clearAllMocks())

    it('returns paths for valid files', async () => {
        vi.mocked(getFileAt).mockResolvedValueOnce(
            mockFileEntry({ name: 'file1.txt', path: '/p/file1.txt', isDirectory: false }),
        )
        vi.mocked(getFileAt).mockResolvedValueOnce(
            mockFileEntry({ name: 'file2.txt', path: '/p/file2.txt', isDirectory: false }),
        )

        expect(await getSelectedFilePaths('listing-1', [0, 1], false)).toEqual(['/p/file1.txt', '/p/file2.txt'])
    })

    it('skips ".." entries', async () => {
        vi.mocked(getFileAt).mockResolvedValueOnce(mockFileEntry({ name: '..', path: '/parent', isDirectory: true }))
        vi.mocked(getFileAt).mockResolvedValueOnce(
            mockFileEntry({ name: 'file.txt', path: '/p/file.txt', isDirectory: false }),
        )

        expect(await getSelectedFilePaths('listing-1', [0, 1], false)).toEqual(['/p/file.txt'])
    })
})

describe('buildTransferPropsFromSelection', () => {
    const context: TransferContext = {
        showHiddenFiles: false,
        sourcePath: '/source',
        destPath: '/dest',
        sourceVolumeId: 'vol-src',
        destVolumeId: 'vol-dest',
        sortColumn: 'name',
        sortOrder: 'ascending',
    }

    beforeEach(() => vi.clearAllMocks())

    it('returns null for empty indices', async () => {
        expect(await buildTransferPropsFromSelection('copy', 'listing-1', [], false, true, context)).toBeNull()
    })

    it('returns correct props for copy selection', async () => {
        vi.mocked(getListingStats).mockResolvedValueOnce({
            totalFiles: 2,
            totalDirs: 1,
            totalFileSize: 1000,
            selectedFiles: 2,
            selectedDirs: 1,
        })
        vi.mocked(getFileAt).mockResolvedValueOnce(
            mockFileEntry({ name: 'file1.txt', path: '/source/file1.txt', isDirectory: false }),
        )
        vi.mocked(getFileAt).mockResolvedValueOnce(
            mockFileEntry({ name: 'folder', path: '/source/folder', isDirectory: true }),
        )

        const result = await buildTransferPropsFromSelection('copy', 'listing-1', [0, 1], false, true, context)
        expect(result).toEqual({
            operationType: 'copy',
            sourcePaths: ['/source/file1.txt', '/source/folder'],
            destinationPath: '/dest',
            direction: 'right',
            currentVolumeId: 'vol-dest',
            fileCount: 2,
            folderCount: 1,
            sourceFolderPath: '/source',
            sortColumn: 'name',
            sortOrder: 'ascending',
            sourceVolumeId: 'vol-src',
            destVolumeId: 'vol-dest',
        })
    })

    it('returns correct props for move selection', async () => {
        vi.mocked(getListingStats).mockResolvedValueOnce({
            totalFiles: 1,
            totalDirs: 0,
            totalFileSize: 500,
            selectedFiles: 1,
            selectedDirs: 0,
        })
        vi.mocked(getFileAt).mockResolvedValueOnce(
            mockFileEntry({ name: 'file.txt', path: '/source/file.txt', isDirectory: false }),
        )

        const result = await buildTransferPropsFromSelection('move', 'listing-1', [0], false, true, context)
        expect(result?.operationType).toBe('move')
    })
})

describe('getCommonParentPath', () => {
    it('returns / for empty paths', () => {
        expect(getCommonParentPath([])).toBe('/')
    })

    it('returns parent of single path', () => {
        expect(getCommonParentPath(['/Users/alice/file.txt'])).toBe('/Users/alice')
    })

    it('returns / for single root-level path', () => {
        expect(getCommonParentPath(['/file.txt'])).toBe('/')
    })

    it('returns common parent for sibling files', () => {
        expect(getCommonParentPath(['/Users/alice/a.txt', '/Users/alice/b.txt'])).toBe('/Users/alice')
    })

    it('returns common parent for paths in different subdirectories', () => {
        expect(getCommonParentPath(['/Users/alice/docs/a.txt', '/Users/alice/photos/b.jpg'])).toBe('/Users/alice')
    })

    it('returns / when only root is common', () => {
        expect(getCommonParentPath(['/foo/a.txt', '/bar/b.txt'])).toBe('/')
    })

    it('handles deeply nested common path', () => {
        expect(getCommonParentPath(['/a/b/c/d/file1.txt', '/a/b/c/d/file2.txt', '/a/b/c/d/file3.txt'])).toBe('/a/b/c/d')
    })
})

describe('buildTransferPropsFromDroppedPaths', () => {
    it('returns correct props for a single dropped file', () => {
        const result = buildTransferPropsFromDroppedPaths(
            'copy',
            ['/Users/alice/file.txt'],
            '/dest',
            'right',
            'vol-dest',
            'name',
            'ascending',
        )

        expect(result).toEqual({
            operationType: 'copy',
            sourcePaths: ['/Users/alice/file.txt'],
            destinationPath: '/dest',
            direction: 'right',
            currentVolumeId: 'vol-dest',
            fileCount: 1,
            folderCount: 0,
            sourceFolderPath: '/Users/alice',
            sortColumn: 'name',
            sortOrder: 'ascending',
            sourceVolumeId: 'vol-dest',
            destVolumeId: 'vol-dest',
        })
    })

    it('returns correct props for multiple dropped files', () => {
        const result = buildTransferPropsFromDroppedPaths(
            'copy',
            ['/Users/alice/a.txt', '/Users/alice/b.txt', '/Users/alice/c.txt'],
            '/dest/folder',
            'left',
            'vol-1',
            'size',
            'descending',
        )

        expect(result.fileCount).toBe(3)
        expect(result.sourceFolderPath).toBe('/Users/alice')
        expect(result.direction).toBe('left')
        expect(result.sortColumn).toBe('size')
        expect(result.sortOrder).toBe('descending')
    })

    it('uses destVolumeId as sourceVolumeId fallback', () => {
        const result = buildTransferPropsFromDroppedPaths(
            'move',
            ['/file.txt'],
            '/dest',
            'right',
            'vol-dest',
            'name',
            'ascending',
        )

        expect(result.sourceVolumeId).toBe('vol-dest')
        expect(result.destVolumeId).toBe('vol-dest')
        expect(result.operationType).toBe('move')
    })
})
