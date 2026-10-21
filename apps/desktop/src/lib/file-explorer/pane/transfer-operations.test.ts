import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import {
  getDestinationVolumeInfo,
  buildTransferPropsFromSelection,
  buildTransferPropsFromCursor,
  buildTransferPropsFromDroppedPaths,
  buildTransferPropsFromSnapshot,
  getCommonParentPath,
  type TransferContext,
} from './transfer-operations'
import type { VolumeInfo } from '../types'
import type { FilePaneAPI } from './types'
import { ListingLookupFailure } from '#lib/tauri-commands/listing-gone.js'

vi.mock('#lib/tauri-commands/index.js', async () => {
  const actual = await vi.importActual<typeof import('#lib/tauri-commands/listing-gone.js')>(
    '#lib/tauri-commands/listing-gone.js',
  )
  return {
    getSelectionSnapshot: vi.fn(),
    asListingLookupError: actual.asListingLookupError,
  }
})
vi.mock('./operation-start-gate', () => ({ announceRowsStillSettling: vi.fn() }))

const { getSelectionSnapshot } = await import('#lib/tauri-commands/index.js')
const { announceRowsStillSettling } = await import('./operation-start-gate')

describe('getDestinationVolumeInfo', () => {
  const volumes: VolumeInfo[] = [
    {
      id: 'vol-1',
      name: 'Main Drive',
      path: '/mnt/main',
      category: 'main_volume',
      isEjectable: false,
      mountIsReadOnly: false,
    },
    {
      id: 'vol-2',
      name: 'Backup',
      path: '/mnt/backup',
      category: 'attached_volume',
      isEjectable: true,
      mountIsReadOnly: true,
    },
    {
      id: 'mtp-device-1:65537',
      name: 'Phone Storage',
      path: 'mtp://device-1/65537',
      category: 'mobile_device',
      isEjectable: true,
      mountIsReadOnly: false,
    },
    {
      id: 'mtp-device-2:65537',
      name: 'Read-only Device',
      path: 'mtp://device-2/65537',
      category: 'mobile_device',
      isEjectable: true,
      mountIsReadOnly: true,
    },
  ]

  it('returns info for regular volume', () => {
    expect(getDestinationVolumeInfo('vol-1', volumes)).toEqual({
      name: 'Main Drive',
      mountIsReadOnly: false,
    })
  })

  it('returns info for read-only regular volume', () => {
    expect(getDestinationVolumeInfo('vol-2', volumes)).toEqual({ name: 'Backup', mountIsReadOnly: true })
  })

  it('returns info for MTP volume', () => {
    expect(getDestinationVolumeInfo('mtp-device-1:65537', volumes)).toEqual({
      name: 'Phone Storage',
      mountIsReadOnly: false,
    })
  })

  it('returns info for read-only MTP volume', () => {
    expect(getDestinationVolumeInfo('mtp-device-2:65537', volumes)).toEqual({
      name: 'Read-only Device',
      mountIsReadOnly: true,
    })
  })

  it('returns undefined when not found', () => {
    expect(getDestinationVolumeInfo('nonexistent', volumes)).toBeUndefined()
  })
})

describe('buildTransferPropsFromSelection / buildTransferPropsFromCursor', () => {
  const context: TransferContext = {
    showHiddenFiles: false,
    sourcePath: '/source',
    destPath: '/dest',
    sourceVolumeId: 'vol-src',
    destVolumeId: 'vol-dest',
  }
  /** A pane at `sequence`, its selection (or cursor) on the given rows, with `..` on row 0. */
  const paneOf = (state: { selected?: number[]; cursor?: number; sequence?: number }) =>
    ({
      getListingId: () => 'listing-1',
      getSelectedIndices: () => state.selected ?? [],
      getCursorIndex: () => state.cursor ?? 0,
      hasParentEntry: () => true,
      getLastSequence: () => state.sequence ?? 0,
    }) as unknown as FilePaneAPI

  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(getSelectionSnapshot).mockReset()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('reads the selection as backend rows under the revision the pane applied', async () => {
    vi.mocked(getSelectionSnapshot).mockResolvedValueOnce({
      paths: ['/source/file1.txt', '/source/folder'],
      fileCount: 1,
      folderCount: 1,
    })

    const result = await buildTransferPropsFromSelection(
      'copy',
      paneOf({ selected: [1, 2], sequence: 7 }),
      true,
      context,
    )

    // `..` is pane row 0, so pane rows 1 and 2 are backend rows 0 and 1.
    expect(getSelectionSnapshot).toHaveBeenCalledWith('listing-1', false, [0, 1], 7)
    expect(result).toEqual({
      operationType: 'copy',
      sourcePaths: ['/source/file1.txt', '/source/folder'],
      destinationPath: '/dest',
      direction: 'right',
      currentVolumeId: 'vol-dest',
      fileCount: 1,
      folderCount: 1,
      sourceFolderPath: '/source',
      sourceVolumeId: 'vol-src',
      destVolumeId: 'vol-dest',
    })
  })

  it('reads the cursor row as a backend row under the revision the pane applied', async () => {
    vi.mocked(getSelectionSnapshot).mockResolvedValueOnce({ paths: ['/source/dir'], fileCount: 0, folderCount: 1 })

    const result = await buildTransferPropsFromCursor('move', paneOf({ cursor: 3, sequence: 5 }), true, context)

    expect(getSelectionSnapshot).toHaveBeenCalledWith('listing-1', false, [2], 5)
    expect(result).toMatchObject({ operationType: 'move', sourcePaths: ['/source/dir'], folderCount: 1 })
  })

  it('offers nothing with the cursor on the `..` row', async () => {
    expect(await buildTransferPropsFromCursor('copy', paneOf({ cursor: 0 }), true, context)).toBeNull()
    expect(getSelectionSnapshot).not.toHaveBeenCalled()
  })

  it.each([true, false])(
    'waits for the pane to apply the diff, then reads the rows it picked (selection: %s)',
    async (onSelection) => {
      const moved = await import('./moved-listing.test-fixtures')
      const pane = moved.createMovedPane({ onSelection })
      vi.mocked(getSelectionSnapshot).mockImplementation(moved.catchingUpAfterRefusal(pane, moved.selectionSnapshot))
      const build = onSelection ? buildTransferPropsFromSelection : buildTransferPropsFromCursor

      const result = await build('move', pane, true, context)

      expect(result?.sourcePaths).toEqual([moved.SELECTED_PATH])
      expect(announceRowsStillSettling).not.toHaveBeenCalled()
    },
  )

  it('refuses once the rows never settle, failing an MCP round-trip', async () => {
    vi.useFakeTimers()
    const moved = await import('./moved-listing.test-fixtures')
    vi.mocked(getSelectionSnapshot).mockImplementation(moved.selectionSnapshot)
    const pane = moved.createMovedPane({ onSelection: true }) as unknown as FilePaneAPI

    const built = buildTransferPropsFromSelection('move', pane, true, { ...context, mcpRequestId: 'req-7' })
    await vi.advanceTimersByTimeAsync(3_500)

    expect(await built).toBeNull()
    expect(announceRowsStillSettling).toHaveBeenCalledWith('req-7')
  })

  it('refuses a selection that vanished while it waited, never falling through to the cursor row', async () => {
    const state = { selected: [2], sequence: 0 }
    vi.mocked(getSelectionSnapshot).mockImplementation(() => {
      state.selected = []
      return Promise.reject(new ListingLookupFailure({ type: 'changed', listingId: 'listing-1' }))
    })

    expect(await buildTransferPropsFromSelection('copy', paneOf(state), true, context)).toBeNull()
    expect(getSelectionSnapshot).toHaveBeenCalledTimes(1)
    expect(announceRowsStillSettling).toHaveBeenCalledTimes(1)
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
  it('returns correct props for a single dropped file (carries the resolved source volume)', () => {
    const result = buildTransferPropsFromDroppedPaths(
      'copy',
      ['/Users/alice/file.txt'],
      '/dest',
      'right',
      'vol-dest',
      'vol-src',
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
      sourceVolumeId: 'vol-src',
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
      'vol-1',
    )

    expect(result.fileCount).toBe(3)
    expect(result.sourceFolderPath).toBe('/Users/alice')
    expect(result.direction).toBe('left')
  })

  it('keeps source and dest volume ids independent so a cross-volume drop stats the right volume', () => {
    // Dropping an MTP source onto a local dest: the source volume id must be the
    // resolved MTP volume, NOT the dest volume. The hardcoded
    // `sourceVolumeId = destVolumeId` placeholder made the scan preview stat
    // MTP-shaped paths as local and report 0 bytes / 0 files.
    const result = buildTransferPropsFromDroppedPaths(
      'move',
      ['mtp://dev/65538/DCIM/IMG.JPG'],
      '/Users/alice/dest',
      'right',
      'root',
      'mtp-dev:65538',
    )

    expect(result.sourceVolumeId).toBe('mtp-dev:65538')
    expect(result.destVolumeId).toBe('root')
    expect(result.operationType).toBe('move')
  })

  it('splits files and folders when all kind flags are known (3 folders dropped)', () => {
    const result = buildTransferPropsFromDroppedPaths(
      'copy',
      ['/Users/alice/one', '/Users/alice/two', '/Users/alice/three'],
      '/dest',
      'right',
      'vol-dest',
      'vol-src',
      [true, true, true],
    )

    expect(result.fileCount).toBe(0)
    expect(result.folderCount).toBe(3)
  })

  it('splits a mixed drop (1 file + 2 folders) when all flags are known', () => {
    const result = buildTransferPropsFromDroppedPaths(
      'copy',
      ['/Users/alice/a.txt', '/Users/alice/dir1', '/Users/alice/dir2'],
      '/dest',
      'right',
      'vol-dest',
      'vol-src',
      [false, true, true],
    )

    expect(result.fileCount).toBe(1)
    expect(result.folderCount).toBe(2)
  })

  it('falls back to the approximate shape when ANY flag is unknown', () => {
    // Honest beats half-right: one null kind ⇒ the whole batch uses today's
    // approximate shape (all-files, zero folders) rather than a partial split.
    const result = buildTransferPropsFromDroppedPaths(
      'copy',
      ['/Users/alice/a.txt', '/Users/alice/dir1', '/Users/alice/mystery'],
      '/dest',
      'right',
      'vol-dest',
      'vol-src',
      [false, true, null],
    )

    expect(result.fileCount).toBe(3)
    expect(result.folderCount).toBe(0)
  })

  it('falls back to the approximate shape when the flags length disagrees', () => {
    const result = buildTransferPropsFromDroppedPaths(
      'copy',
      ['/Users/alice/a.txt', '/Users/alice/dir1'],
      '/dest',
      'right',
      'vol-dest',
      'vol-src',
      [true],
    )

    expect(result.fileCount).toBe(2)
    expect(result.folderCount).toBe(0)
  })
})

describe('buildTransferPropsFromSnapshot (source-side ops)', () => {
  it('returns null when no source paths are supplied', () => {
    expect(buildTransferPropsFromSnapshot('copy', [], [], true, '/dest', 'vol-dest', 'root')).toBeNull()
  })

  it('returns null when paths and flags lengths disagree', () => {
    // Defensive: would otherwise misreport file/folder counts.
    expect(
      buildTransferPropsFromSnapshot('copy', ['/a/x', '/a/y'], [false], true, '/dest', 'vol-dest', 'root'),
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
      'vol-src',
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
    // The caller places the rows against the live volume list and passes the
    // answer in; this builder never assumes one. `file-operation-commands`
    // resolves it through `snapshot-source-volume.ts`.
    expect(props.sourceVolumeId).toBe('vol-src')
  })

  it('sets direction based on which pane is the source (isLeft=true → direction "right")', () => {
    const left = buildTransferPropsFromSnapshot(
      'move',
      ['/a/x'],
      [false],
      true, // source is the left pane → files travel right
      '/dest',
      'vol-dest',
      'root',
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
      'root',
    )
    if (!right) throw new Error('expected non-null right')
    expect(right.direction).toBe('left')
  })

  it('passes the operation type through unchanged', () => {
    const props = buildTransferPropsFromSnapshot('move', ['/a/x'], [false], true, '/dest', 'vol-dest', 'root')
    if (!props) throw new Error('expected non-null props')
    expect(props.operationType).toBe('move')
  })
})
