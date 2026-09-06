/**
 * Tests for `pane-mcp-sync.svelte.ts`, the factory that mirrors a pane's state
 * into the MCP `PaneState` store. They pin:
 * - the visible range is fetched in ONE backend call, whatever its size,
 * - the parent `..` row is prepended when it's in view,
 * - a listing that has shrunk under the frontend's cached count yields a short
 *   list rather than a throw,
 * - a pane whose kind doesn't mirror to MCP asks the backend nothing.
 *
 * The call COUNT is the point of the first one. Fetching the range a row at a
 * time was one IPC round trip per row, each landing on a backend accessor that
 * walked the whole listing, and at the bottom of a big directory that stopped
 * the app answering IPC at all
 * (`docs/notes/listing-row-fetch-quadratic-2026-08-22.md`).
 */
import { beforeEach, describe, expect, it, vi, type Mock } from 'vitest'

const { getFileAt, getFileRange, updateLeftPaneState, updateRightPaneState } = vi.hoisted<{
  getFileAt: Mock
  getFileRange: Mock
  updateLeftPaneState: Mock
  updateRightPaneState: Mock
}>(() => ({
  getFileAt: vi.fn(),
  getFileRange: vi.fn(),
  updateLeftPaneState: vi.fn().mockResolvedValue(undefined),
  updateRightPaneState: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('$lib/tauri-commands', () => ({
  getFileAt,
  getFileRange,
  updateLeftPaneState,
  updateRightPaneState,
}))

import type { CanonicalPath } from '$lib/path/canonical'
import type { SearchResultEntry } from '$lib/ipc/bindings'
import { createPaneMcpSync, type PaneMcpSyncDeps } from './pane-mcp-sync.svelte'

const TOTAL_COUNT = 74_144

function entryAt(index: number) {
  return {
    name: `file-${String(index)}.bin`,
    path: `/big/file-${String(index)}.bin`,
    isDirectory: false,
    size: index,
    modifiedAt: null,
    tags: [],
  }
}

/** A pane parked at the bottom of a big local directory, the wedge's shape. */
function deps(overrides: Partial<PaneMcpSyncDeps> = {}): PaneMcpSyncDeps {
  return {
    paneId: 'left',
    getSyncsToMcp: () => true,
    getListingId: () => 'listing-1',
    getTotalCount: () => TOTAL_COUNT,
    getRowCount: () => TOTAL_COUNT,
    getSnapshotEntries: () => null,
    getHasParent: () => false,
    getVisibleRangeStart: () => TOTAL_COUNT - 100,
    getVisibleRangeEnd: () => TOTAL_COUNT,
    getCanonicalPath: () => '/big' as CanonicalPath,
    getIncludeHidden: () => true,
    getCurrentPath: () => '/big',
    getVolumeId: () => 'root',
    getVolumeName: () => 'Macintosh HD',
    getCursorIndex: () => TOTAL_COUNT - 1,
    getViewMode: () => 'full',
    getSelectedIndices: () => [],
    getSortBy: () => 'name',
    getSortOrder: () => 'ascending',
    getShowHiddenFiles: () => true,
    getTypeToJump: () => ({ buffer: '', indicatorVisible: false, indicatorStale: false }),
    getLastJumpMatchedName: () => null,
    ...overrides,
  }
}

describe('buildMcpFileList', () => {
  beforeEach(() => {
    getFileAt.mockReset()
    getFileRange.mockReset()
    getFileRange.mockImplementation((_id: string, start: number, count: number) =>
      Promise.resolve(Array.from({ length: count }, (_unused, i) => entryAt(start + i))),
    )
  })

  it('fetches the whole visible range in one call', async () => {
    const sync = createPaneMcpSync(deps())

    const files = await sync.buildMcpFileList()

    expect(files).toHaveLength(100)
    expect(files[0]?.name).toBe(`file-${String(TOTAL_COUNT - 100)}.bin`)
    expect(getFileRange).toHaveBeenCalledTimes(1)
    expect(getFileRange).toHaveBeenCalledWith('listing-1', TOTAL_COUNT - 100, 100, true)
    expect(getFileAt).not.toHaveBeenCalled()
  })

  it('prepends the parent row when the top of the listing is in view', async () => {
    const sync = createPaneMcpSync(
      deps({
        getHasParent: () => true,
        getVisibleRangeStart: () => 0,
        getVisibleRangeEnd: () => 10,
      }),
    )

    const files = await sync.buildMcpFileList()

    expect(files[0]?.name).toBe('..')
    expect(files).toHaveLength(10)
    // Nine backend rows, because the parent occupies the first of the ten slots.
    expect(getFileRange).toHaveBeenCalledWith('listing-1', 0, 9, true)
  })

  it('stops at the last row the backend actually has', async () => {
    // The listing shrank while a `directory-diff` was in flight, so the range
    // comes back short of what the cached `totalCount` promised.
    getFileRange.mockResolvedValueOnce([entryAt(0), entryAt(1)])
    const sync = createPaneMcpSync(deps({ getVisibleRangeStart: () => 0, getVisibleRangeEnd: () => 50 }))

    const files = await sync.buildMcpFileList()

    expect(files).toHaveLength(2)
  })

  it('asks the backend nothing for a pane that does not mirror to MCP', async () => {
    const sync = createPaneMcpSync(deps({ getSyncsToMcp: () => false }))

    expect(await sync.buildMcpFileList()).toEqual([])
    expect(getFileRange).not.toHaveBeenCalled()
  })
})

/**
 * A search-results snapshot pane. It has no backend listing to fetch rows from, so
 * for a long time it pushed nothing at all and the MCP store kept describing the
 * directory the pane came FROM. An MCP delete then reasoned on that stale state and
 * refused ("the cursor is on the parent entry") over a pane holding a real cursor row.
 */
describe('a search-results snapshot pane', () => {
  function result(name: string): SearchResultEntry {
    return {
      name,
      path: `/Users/test/${name}`,
      parentPath: '/Users/test',
      isDirectory: false,
      size: 12,
      modifiedAt: 1_700_000_000,
      iconId: 'ext:txt',
    }
  }

  const ROWS = [result('a.txt'), result('b.txt'), result('c.txt')]

  function snapshotDeps(overrides: Partial<PaneMcpSyncDeps> = {}): PaneMcpSyncDeps {
    return deps({
      getListingId: () => '',
      getTotalCount: () => 0,
      getRowCount: () => ROWS.length,
      getSnapshotEntries: () => ROWS,
      getHasParent: () => false,
      getCurrentPath: () => 'search-results://snap-1',
      getVolumeId: () => 'search-results',
      getVisibleRangeStart: () => 0,
      getVisibleRangeEnd: () => 100,
      getCursorIndex: () => 1,
      getSelectedIndices: () => [],
      ...overrides,
    })
  }

  beforeEach(() => {
    getFileRange.mockReset()
    updateLeftPaneState.mockClear()
  })

  it('mirrors its rows without asking the backend for a listing', async () => {
    const sync = createPaneMcpSync(snapshotDeps())

    const files = await sync.buildMcpFileList()

    expect(files.map((f) => f.name)).toEqual(['a.txt', 'b.txt', 'c.txt'])
    // The Name column shows the friendly full path, but MCP reports the basename
    // and carries the absolute path separately, the way every other pane does.
    expect(files[0]?.path).toBe('/Users/test/a.txt')
    expect(getFileRange).not.toHaveBeenCalled()
  })

  it('pushes the snapshot path, its row count, and its parentless shape', async () => {
    const sync = createPaneMcpSync(snapshotDeps())

    await sync.syncPaneStateToMcp()

    expect(updateLeftPaneState).toHaveBeenCalledTimes(1)
    const state = updateLeftPaneState.mock.calls[0][0] as {
      path: string
      volumeId: string
      totalFiles: number
      hasParentRow: boolean
      cursorIndex: number
    }
    expect(state.path).toBe('search-results://snap-1')
    expect(state.volumeId).toBe('search-results')
    expect(state.totalFiles).toBe(3)
    // No `..` row, so the backend gate can't read "one counted row" as "empty folder".
    expect(state.hasParentRow).toBe(false)
    expect(state.cursorIndex).toBe(1)
  })

  it('carries the live selection, which is what an MCP delete acts on', async () => {
    const sync = createPaneMcpSync(snapshotDeps({ getSelectedIndices: () => [0, 2] }))

    await sync.syncPaneStateToMcp()

    const state = updateLeftPaneState.mock.calls[0][0] as { selectedIndices: number[] }
    expect(state.selectedIndices).toEqual([0, 2])
  })

  it('follows the visible range rather than serializing the whole result set', async () => {
    const sync = createPaneMcpSync(snapshotDeps({ getVisibleRangeStart: () => 1, getVisibleRangeEnd: () => 3 }))

    expect((await sync.buildMcpFileList()).map((f) => f.name)).toEqual(['b.txt', 'c.txt'])
  })

  it('pushes an empty file list for an empty snapshot, so the gate can say so', async () => {
    const sync = createPaneMcpSync(snapshotDeps({ getSnapshotEntries: () => [], getRowCount: () => 0 }))

    await sync.syncPaneStateToMcp()

    const state = updateLeftPaneState.mock.calls[0][0] as { files: unknown[]; totalFiles: number }
    expect(state.files).toEqual([])
    expect(state.totalFiles).toBe(0)
  })
})

/**
 * A normal pane's `totalFiles` still counts its `..` row, and `hasParentRow` says so.
 * The two travel together: the backend's empty-pane gate subtracts one from the count
 * only where the flag is set.
 */
describe('the parent row a normal pane counts', () => {
  it('reports its `..` row in both the count and the flag', async () => {
    getFileRange.mockResolvedValue([])
    updateLeftPaneState.mockClear()
    const sync = createPaneMcpSync(
      deps({
        getHasParent: () => true,
        getTotalCount: () => 4,
        getRowCount: () => 5,
        getVisibleRangeStart: () => 0,
        getVisibleRangeEnd: () => 5,
      }),
    )

    await sync.syncPaneStateToMcp()

    const state = updateLeftPaneState.mock.calls[0][0] as { totalFiles: number; hasParentRow: boolean }
    expect(state.totalFiles).toBe(5)
    expect(state.hasParentRow).toBe(true)
  })
})
