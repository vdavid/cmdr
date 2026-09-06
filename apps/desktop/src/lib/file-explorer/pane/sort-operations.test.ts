import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FilePaneAPI } from './types'
import type { SortColumn, SortOrder } from '../types'

const { resortListingSpy, getDirectorySortModeSpy, sortSnapshotSpy, getSnapshotSpy } = vi.hoisted(() => ({
  resortListingSpy: vi.fn<() => Promise<{ newCursorIndex: number | null; newSelectedIndices: number[] | null }>>(),
  getDirectorySortModeSpy: vi.fn<() => string>(),
  sortSnapshotSpy: vi.fn<() => Promise<void>>(),
  getSnapshotSpy: vi.fn<() => { sort: { column: SortColumn; order: SortOrder } | null } | undefined>(),
}))

vi.mock('$lib/tauri-commands', () => ({ resortListing: resortListingSpy }))
vi.mock('$lib/search/snapshot-sort.svelte', async () => {
  // The tri-state cycle itself is pinned in `snapshot-sort.svelte.test.ts`; here
  // we only assert the ROUTING, so the real cycle is kept and the mutator spied.
  const real = await vi.importActual<typeof import('$lib/search/snapshot-sort.svelte')>(
    '$lib/search/snapshot-sort.svelte',
  )
  return { nextSnapshotSort: real.nextSnapshotSort, sortSnapshot: sortSnapshotSpy }
})
vi.mock('$lib/search/snapshot-store.svelte', () => ({
  getSnapshot: getSnapshotSpy,
  snapshotIdFromPanePath: (path: string) =>
    path.startsWith('search-results://') ? path.slice('search-results://'.length) : null,
}))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
  getDirectorySortMode: getDirectorySortModeSpy,
}))

import { createSortOperations, type SortOperationsDeps } from './sort-operations'

/** Minimal FilePaneAPI stub carrying only the methods the sort path touches. */
function makePaneRef(overrides: Record<string, unknown> = {}) {
  return {
    cancelRename: vi.fn(),
    clearJumpState: vi.fn(),
    getListingId: vi.fn(() => 'listing-1'),
    // An ordinary folder pane, so the snapshot branch doesn't take.
    getCurrentPath: vi.fn(() => '/Users/test/dir'),
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
    sortSnapshotSpy.mockResolvedValue(undefined)
    getSnapshotSpy.mockReturnValue(undefined)
  })

  // ── A search-results pane sorts its SNAPSHOT, never its tab ──────────────
  //
  // The keyboard sort commands (`sort.byName` and friends) reach the focused pane
  // through here, so they have to run the same tri-state cycle a header click
  // does. A snapshot pane has no listing id, so without this branch every one of
  // them was silently inert.

  /** A pane ref that reports itself as a snapshot pane on `sr-7`. */
  function makeSnapshotPaneRef() {
    return makePaneRef({
      getListingId: vi.fn(() => ''),
      getCurrentPath: vi.fn(() => 'search-results://sr-7'),
    })
  }

  it('routes a sort command on a snapshot pane to the snapshot, not the listing', async () => {
    getSnapshotSpy.mockReturnValue({ sort: null })
    const paneRef = makeSnapshotPaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })

    await createSortOperations(deps).handleSortChange('left', 'size')

    expect(sortSnapshotSpy).toHaveBeenCalledWith('sr-7', { column: 'size', order: 'descending' })
    expect(resortListingSpy).not.toHaveBeenCalled()
    // ❗ The tab's persisted sort belongs to the folder the user came from.
    // Writing it here would re-sort that folder on the way back.
    expect(setPaneSort).not.toHaveBeenCalled()
  })

  it('cycles a snapshot pane back to the ranked order on the third command', async () => {
    getSnapshotSpy.mockReturnValue({ sort: { column: 'size', order: 'ascending' } })
    const paneRef = makeSnapshotPaneRef()
    const { deps } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })

    await createSortOperations(deps).handleSortChange('left', 'size')

    expect(sortSnapshotSpy).toHaveBeenCalledWith('sr-7', null)
  })

  it('sets a snapshot pane order atomically for the MCP sort, with no ranked state to land in', async () => {
    getSnapshotSpy.mockReturnValue({ sort: null })
    const paneRef = makeSnapshotPaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })

    await createSortOperations(deps).setSort('modified', 'asc', 'left')

    expect(sortSnapshotSpy).toHaveBeenCalledWith('sr-7', { column: 'modified', order: 'ascending' })
    expect(setPaneSort).not.toHaveBeenCalled()
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
