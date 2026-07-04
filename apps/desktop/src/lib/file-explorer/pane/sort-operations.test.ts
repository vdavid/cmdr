import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FilePaneAPI } from './types'
import type { SortColumn, SortOrder } from '../types'

const { resortListingSpy, getDirectorySortModeSpy } = vi.hoisted(() => ({
  resortListingSpy: vi.fn<() => Promise<{ newCursorIndex: number | null; newSelectedIndices: number[] | null }>>(),
  getDirectorySortModeSpy: vi.fn<() => string>(),
}))

vi.mock('$lib/tauri-commands', () => ({ resortListing: resortListingSpy }))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({ getDirectorySortMode: getDirectorySortModeSpy }))

import { createSortOperations, type SortOperationsDeps } from './sort-operations'

/** Minimal FilePaneAPI stub carrying only the methods the sort path touches. */
function makePaneRef(overrides: Record<string, unknown> = {}) {
  return {
    cancelRename: vi.fn(),
    clearJumpState: vi.fn(),
    getListingId: vi.fn(() => 'listing-1'),
    getFilenameUnderCursor: vi.fn(() => 'cursor.txt'),
    getSelectedIndices: vi.fn(() => []),
    isAllSelected: vi.fn(() => false),
    hasParentEntry: vi.fn(() => false),
    setCursorIndex: vi.fn(),
    setSelectedIndices: vi.fn(),
    refreshView: vi.fn(),
    ...overrides,
  }
}

function makeDeps(
  paneRef: ReturnType<typeof makePaneRef>,
  sort: { sortBy: SortColumn; sortOrder: SortOrder },
): {
  deps: SortOperationsDeps
  setPaneSort: ReturnType<typeof vi.fn>
} {
  const setPaneSort = vi.fn()
  const deps: SortOperationsDeps = {
    getPaneRef: () => paneRef as unknown as FilePaneAPI,
    getPaneSort: () => sort,
    setPaneSort,
    getShowHiddenFiles: () => false,
    getFocusedPane: () => 'left',
  }
  return { deps, setPaneSort }
}

describe('createSortOperations', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resortListingSpy.mockResolvedValue({ newCursorIndex: null, newSelectedIndices: null })
    getDirectorySortModeSpy.mockReturnValue('foldersFirst')
  })

  it('handleSortChange on a NEW column applies that column default order', async () => {
    const paneRef = makePaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    await ops.handleSortChange('left', 'size')

    // `size` default order is descending (defaultSortOrders); a new column ignores current order.
    expect(setPaneSort).toHaveBeenCalledWith('left', 'size', 'descending')
    expect(resortListingSpy).toHaveBeenCalledWith(
      'listing-1',
      'size',
      'descending',
      'cursor.txt',
      false,
      [],
      false,
      'foldersFirst',
    )
  })

  it('handleSortChange on the SAME column toggles order', async () => {
    const paneRef = makePaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    await ops.handleSortChange('left', 'name')

    expect(setPaneSort).toHaveBeenCalledWith('left', 'name', 'descending')
  })

  it('handleSortChange cancels rename and clears type-to-jump before re-sorting', async () => {
    const paneRef = makePaneRef()
    const { deps } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    await ops.handleSortChange('left', 'name')

    expect(paneRef.cancelRename).toHaveBeenCalled()
    expect(paneRef.clearJumpState).toHaveBeenCalled()
  })

  it('handleSortChange with no listing id is a no-op (no re-sort)', async () => {
    const paneRef = makePaneRef({ getListingId: vi.fn(() => '') })
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    await ops.handleSortChange('left', 'size')

    expect(resortListingSpy).not.toHaveBeenCalled()
    expect(setPaneSort).not.toHaveBeenCalled()
  })

  it('setSortOrder toggle flips ascending to descending', async () => {
    const paneRef = makePaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    ops.setSortOrder('toggle')
    await Promise.resolve()

    expect(setPaneSort).toHaveBeenCalledWith('left', 'name', 'descending')
  })

  it('setSortOrder is a no-op when the requested order already matches', async () => {
    const paneRef = makePaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    ops.setSortOrder('asc')
    await Promise.resolve()

    expect(setPaneSort).not.toHaveBeenCalled()
    expect(resortListingSpy).not.toHaveBeenCalled()
  })

  it('setSort applies the column and order atomically', async () => {
    const paneRef = makePaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    await ops.setSort('modified', 'desc', 'right')

    expect(setPaneSort).toHaveBeenCalledWith('right', 'modified', 'descending')
    expect(resortListingSpy).toHaveBeenCalledWith(
      'listing-1',
      'modified',
      'descending',
      'cursor.txt',
      false,
      [],
      false,
      'foldersFirst',
    )
  })

  it('setSortColumn defaults to the focused pane', async () => {
    const paneRef = makePaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    ops.setSortColumn('size')
    await Promise.resolve()

    expect(setPaneSort).toHaveBeenCalledWith('left', 'size', 'descending')
  })
})
