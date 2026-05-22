import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  getDestinationVolumeInfo,
  getSelectedFilePaths,
  buildTransferPropsFromSelection,
  buildTransferPropsFromDroppedPaths,
  buildTransferPropsFromSnapshot,
  getCommonParentPath,
  type TransferContext,
} from './transfer-operations'
import type { VolumeInfo } from '../types'

vi.mock('$lib/tauri-commands', () => ({
  getListingStats: vi.fn(),
  getPathsAtIndices: vi.fn(),
}))

const { getListingStats, getPathsAtIndices } = await import('$lib/tauri-commands')

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
    {
      id: 'mtp-device-1:65537',
      name: 'Phone Storage',
      path: 'mtp://device-1/65537',
      category: 'mobile_device',
      isEjectable: true,
      isReadOnly: false,
    },
    {
      id: 'mtp-device-2:65537',
      name: 'Read-only Device',
      path: 'mtp://device-2/65537',
      category: 'mobile_device',
      isEjectable: true,
      isReadOnly: true,
    },
  ]

  it('returns info for regular volume', () => {
    expect(getDestinationVolumeInfo('vol-1', volumes)).toEqual({
      name: 'Main Drive',
      isReadOnly: false,
    })
  })

  it('returns info for read-only regular volume', () => {
    expect(getDestinationVolumeInfo('vol-2', volumes)).toEqual({ name: 'Backup', isReadOnly: true })
  })

  it('returns info for MTP volume', () => {
    expect(getDestinationVolumeInfo('mtp-device-1:65537', volumes)).toEqual({
      name: 'Phone Storage',
      isReadOnly: false,
    })
  })

  it('returns info for read-only MTP volume', () => {
    expect(getDestinationVolumeInfo('mtp-device-2:65537', volumes)).toEqual({
      name: 'Read-only Device',
      isReadOnly: true,
    })
  })

  it('returns undefined when not found', () => {
    expect(getDestinationVolumeInfo('nonexistent', volumes)).toBeUndefined()
  })
})

describe('getSelectedFilePaths', () => {
  beforeEach(() => vi.clearAllMocks())

  it('returns paths for valid files', async () => {
    vi.mocked(getPathsAtIndices).mockResolvedValueOnce(['/p/file1.txt', '/p/file2.txt'])

    expect(await getSelectedFilePaths('listing-1', [0, 1], false, false)).toEqual(['/p/file1.txt', '/p/file2.txt'])
    expect(getPathsAtIndices).toHaveBeenCalledWith('listing-1', [0, 1], false, false)
  })

  it('passes hasParent to backend for ".." filtering', async () => {
    vi.mocked(getPathsAtIndices).mockResolvedValueOnce(['/p/file.txt'])

    expect(await getSelectedFilePaths('listing-1', [0, 1], false, true)).toEqual(['/p/file.txt'])
    expect(getPathsAtIndices).toHaveBeenCalledWith('listing-1', [0, 1], false, true)
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
      totalSize: 1000,
      totalPhysicalSize: 1024,
      selectedFiles: 2,
      selectedDirs: 1,
      selectedSize: null,
      selectedPhysicalSize: null,
    })
    vi.mocked(getPathsAtIndices).mockResolvedValueOnce(['/source/file1.txt', '/source/folder'])

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
      totalSize: 500,
      totalPhysicalSize: 512,
      selectedFiles: 1,
      selectedDirs: 0,
      selectedSize: null,
      selectedPhysicalSize: null,
    })
    vi.mocked(getPathsAtIndices).mockResolvedValueOnce(['/source/file.txt'])

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

describe('buildTransferPropsFromSnapshot (M8d source-side ops)', () => {
  it('returns null when no source paths are supplied', () => {
    expect(buildTransferPropsFromSnapshot('copy', [], [], true, '/dest', 'vol-dest', 'name', 'ascending')).toBeNull()
  })

  it('returns null when paths and flags lengths disagree', () => {
    // Defensive: would otherwise misreport file/folder counts.
    expect(
      buildTransferPropsFromSnapshot('copy', ['/a/x', '/a/y'], [false], true, '/dest', 'vol-dest', 'name', 'ascending'),
    ).toBeNull()
  })

  it('counts files and folders separately and derives the common parent', () => {
    const props = buildTransferPropsFromSnapshot(
      'copy',
      ['/Users/a/photos/img1.jpg', '/Users/a/photos/img2.jpg', '/Users/a/photos/subdir'],
      [false, false, true],
      true,
      '/Users/a/desktop',
      'vol-dest',
      'name',
      'ascending',
    )

    if (!props) throw new Error('expected non-null props')
    expect(props.fileCount).toBe(2)
    expect(props.folderCount).toBe(1)
    expect(props.sourceFolderPath).toBe('/Users/a/photos')
    expect(props.sourcePaths).toEqual([
      '/Users/a/photos/img1.jpg',
      '/Users/a/photos/img2.jpg',
      '/Users/a/photos/subdir',
    ])
    expect(props.destinationPath).toBe('/Users/a/desktop')
    expect(props.destVolumeId).toBe('vol-dest')
    // Source is always 'root' for snapshot panes (entries live on the local FS).
    expect(props.sourceVolumeId).toBe('root')
  })

  it('sets direction based on which pane is the source (isLeft=true → direction "right")', () => {
    const left = buildTransferPropsFromSnapshot(
      'move',
      ['/a/x'],
      [false],
      true, // source is the left pane → files travel right
      '/dest',
      'vol-dest',
      'name',
      'ascending',
    )
    if (!left) throw new Error('expected non-null left')
    expect(left.direction).toBe('right')

    const right = buildTransferPropsFromSnapshot(
      'move',
      ['/a/x'],
      [false],
      false, // source is the right pane → files travel left
      '/dest',
      'vol-dest',
      'name',
      'ascending',
    )
    if (!right) throw new Error('expected non-null right')
    expect(right.direction).toBe('left')
  })

  it('passes the operation type through unchanged', () => {
    const props = buildTransferPropsFromSnapshot(
      'move',
      ['/a/x'],
      [false],
      true,
      '/dest',
      'vol-dest',
      'name',
      'ascending',
    )
    if (!props) throw new Error('expected non-null props')
    expect(props.operationType).toBe('move')
  })
})
