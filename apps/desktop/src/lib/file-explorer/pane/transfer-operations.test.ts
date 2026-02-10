import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
    getDestinationVolumeInfo,
    getSelectedFilePaths,
    buildTransferPropsFromSelection,
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
