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
