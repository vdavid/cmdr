import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FilePaneAPI } from './types'
import type { ResortResult, SortColumn, SortOrder } from '../types'
import type { AppliedSnapshotSort } from '#lib/search/snapshot-store.svelte.js'
import { createPaneRowState } from './pane-row-state'

const {
  resortListingSpy,
  getDirectorySortModeSpy,
  sortSnapshotSpy,
  getSnapshotSpy,
  commitSizeSortLimitSpy,
  addToastForPaneSpy,
  showDateSortMenuSpy,
} = vi.hoisted(() => ({
  showDateSortMenuSpy: vi.fn<(...args: unknown[]) => Promise<void>>(),
  commitSizeSortLimitSpy: vi.fn<(listingId: string) => number | null>(),
  addToastForPaneSpy: vi.fn(),
  resortListingSpy: vi.fn<(...args: unknown[]) => Promise<ResortResult>>(),
  getDirectorySortModeSpy: vi.fn<() => string>(),
  sortSnapshotSpy: vi.fn<() => Promise<AppliedSnapshotSort | null>>(),
  getSnapshotSpy: vi.fn<() => { sort: { column: SortColumn; order: SortOrder } | null } | undefined>(),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  resortListing: resortListingSpy,
  showDateSortMenu: showDateSortMenuSpy,
}))
vi.mock('../git/commit-count-fill', () => ({ commitSizeSortLimit: commitSizeSortLimitSpy }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToastForPane: addToastForPaneSpy }))
vi.mock('#lib/search/snapshot-sort.svelte.js', async () => {
  // The tri-state cycle itself is pinned in `snapshot-sort.svelte.test.ts`; here
  // we only assert the ROUTING, so the real cycle is kept and the mutator spied.
  const real = await vi.importActual<typeof import('#lib/search/snapshot-sort.svelte.js')>(
    '#lib/search/snapshot-sort.svelte.js',
  )
  return { nextSnapshotSort: real.nextSnapshotSort, sortSnapshot: sortSnapshotSpy }
})
vi.mock('#lib/search/snapshot-store.svelte.js', () => ({
  getSnapshot: getSnapshotSpy,
  snapshotIdFromPanePath: (path: string) =>
    path.startsWith('search-results://') ? path.slice('search-results://'.length) : null,
}))
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  getDirectorySortMode: getDirectorySortModeSpy,
}))

import { createSortOperations, type SortOperationsDeps } from './sort-operations'

/** Minimal FilePaneAPI stub carrying only the methods the sort path touches. */
function makePaneRef(overrides: Record<string, unknown> = {}) {
  const rows = createPaneRowState({
    getListingId: () => ref.getListingId(),
    getLoading: () => false,
    getOperationActive: () => false,
    getIncludeHidden: () => false,
  })
  const ref = {
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
    getRowState: () => rows,
    applyRowResult: vi.fn(),
    ...overrides,
  }
  rows.initialize()
  return ref
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
    // The volume honors every column unless a test says otherwise.
    resortListingSpy.mockImplementation((_listingId, column) =>
      Promise.resolve({
        newCursorIndex: null,
        newSelectedIndices: null,
        sequence: 1,
        totalCount: 3,
        hiddenCount: 0,
        effectiveSortBy: column as SortColumn,
      }),
    )
    getDirectorySortModeSpy.mockReturnValue('foldersFirst')
    sortSnapshotSpy.mockResolvedValue(null)
    getSnapshotSpy.mockReturnValue(undefined)
    commitSizeSortLimitSpy.mockReturnValue(null)
  })

  it('gates comparison before the request and supplies the exact applied revision', async () => {
    const paneRef = makePaneRef()
    paneRef.getRowState().setSequence(7)
    const { deps } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const done = createSortOperations(deps).setSort('size', 'desc', 'left')
    expect(paneRef.getRowState().isReady()).toBe(false)
    await done
    expect(resortListingSpy.mock.calls[0].at(-1)).toBe(7)
  })

  it('never installs a sort answer into a replaced pane', async () => {
    const paneRef = makePaneRef()
    let current = paneRef
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    deps.getPaneRef = () => current as unknown as FilePaneAPI
    resortListingSpy.mockImplementationOnce(() => {
      current = makePaneRef()
      return Promise.resolve({
        newCursorIndex: 1,
        newSelectedIndices: [1],
        sequence: 1,
        totalCount: 3,
        hiddenCount: 0,
        effectiveSortBy: 'size',
      })
    })
    await createSortOperations(deps).setSort('size', 'desc', 'left')
    expect(setPaneSort).not.toHaveBeenCalled()
    expect(paneRef.setSelectedIndices).not.toHaveBeenCalled()
    expect(paneRef.applyRowResult).not.toHaveBeenCalled()
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
      0,
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

  it('refuses Size sort on a commit list too long to count, and says why', async () => {
    commitSizeSortLimitSpy.mockReturnValue(5000)
    const paneRef = makePaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    await ops.handleSortChange('left', 'size')
    await ops.setSort('size', 'desc', 'left')

    expect(resortListingSpy).not.toHaveBeenCalled()
    expect(setPaneSort).not.toHaveBeenCalled()
    expect(addToastForPaneSpy).toHaveBeenCalledWith(
      'left',
      'Sorting by size works for up to 5,000 commits, and this history has more.',
      expect.objectContaining({ level: 'info' }),
    )
  })

  it('still sorts the same long commit list by any other column', async () => {
    commitSizeSortLimitSpy.mockReturnValue(5000)
    const paneRef = makePaneRef()
    const { deps } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })

    await createSortOperations(deps).handleSortChange('left', 'modified')

    expect(resortListingSpy).toHaveBeenCalled()
    expect(addToastForPaneSpy).not.toHaveBeenCalled()
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
    await vi.waitFor(() => {
      expect(setPaneSort).toHaveBeenCalled()
    })

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
      0,
    )
  })

  it('setSortColumn defaults to the focused pane', async () => {
    const paneRef = makePaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    ops.setSortColumn('size')
    await vi.waitFor(() => {
      expect(setPaneSort).toHaveBeenCalled()
    })

    expect(setPaneSort).toHaveBeenCalledWith('left', 'size', 'descending')
  })

  // ── A date sort the location can't honor ──────────────────────────────────
  //
  // The backend sorts by Modified and says so; the pane keeps the column the user
  // asked for, and a sort the user started says why the rows follow Modified.

  const ADDED_FALLBACK = 'This location doesn’t record when items were added, so Cmdr sorts by date modified here.'

  function landsInModified() {
    resortListingSpy.mockImplementation(() =>
      Promise.resolve({
        newCursorIndex: null,
        newSelectedIndices: null,
        sequence: 1,
        totalCount: 3,
        hiddenCount: 0,
        effectiveSortBy: 'modified',
      }),
    )
  }

  it('says why when a sort the user picked lands in a fallback, and keeps the column asked for', async () => {
    landsInModified()
    const paneRef = makePaneRef()
    const { deps, setPaneSort } = makeDeps(paneRef, { sortBy: 'name', sortOrder: 'ascending' })

    await createSortOperations(deps).handleSortChange('left', 'added')

    expect(resortListingSpy.mock.calls[0][1]).toBe('added')
    expect(setPaneSort).toHaveBeenCalledWith('left', 'added', 'descending')
    expect(paneRef.applyRowResult).toHaveBeenCalledWith(expect.objectContaining({ effectiveSortBy: 'modified' }))
    expect(addToastForPaneSpy).toHaveBeenCalledWith('left', ADDED_FALLBACK, expect.objectContaining({ level: 'info' }))
  })

  it('names the date it was asked for', async () => {
    landsInModified()
    const { deps } = makeDeps(makePaneRef(), { sortBy: 'name', sortOrder: 'ascending' })

    await createSortOperations(deps).setSort('created', 'asc', 'right')

    expect(addToastForPaneSpy).toHaveBeenCalledWith(
      'right',
      'This location doesn’t record when items were created, so Cmdr sorts by date modified here.',
      expect.objectContaining({ level: 'info' }),
    )
  })

  it('stays quiet when the location honors the sort', async () => {
    const { deps } = makeDeps(makePaneRef(), { sortBy: 'name', sortOrder: 'ascending' })

    await createSortOperations(deps).handleSortChange('left', 'added')

    expect(addToastForPaneSpy).not.toHaveBeenCalled()
  })

  it('stays quiet on a re-sort the user didn’t start, and asks for the requested column again', async () => {
    landsInModified()
    const { deps } = makeDeps(makePaneRef(), { sortBy: 'added', sortOrder: 'descending' })

    await createSortOperations(deps).resortPaneWithCurrentSort('left')

    expect(resortListingSpy.mock.calls[0][1]).toBe('added')
    expect(addToastForPaneSpy).not.toHaveBeenCalled()
  })

  // Flipping the order of a date sort the location can't honor would otherwise
  // repeat the same sentence on every flip: the header's glyph already says it.
  it('stays quiet on an order toggle of a date sort that already fell back', async () => {
    landsInModified()
    const { deps, setPaneSort } = makeDeps(makePaneRef(), { sortBy: 'added', sortOrder: 'descending' })
    const ops = createSortOperations(deps)

    await ops.handleSortChange('left', 'added')
    ops.setSortOrder('toggle')
    await vi.waitFor(() => {
      expect(setPaneSort).toHaveBeenCalledTimes(2)
    })
    await ops.setSort('added', 'asc', 'left')

    expect(resortListingSpy).toHaveBeenCalledTimes(3)
    expect(addToastForPaneSpy).not.toHaveBeenCalled()
  })

  it('stays quiet when a snapshot pane flips the order of a date sort that already fell back', async () => {
    getSnapshotSpy.mockReturnValue({ sort: { column: 'added', order: 'ascending' } })
    sortSnapshotSpy.mockResolvedValue({ column: 'added', order: 'descending', effectiveSortBy: 'modified' })
    const { deps } = makeDeps(makeSnapshotPaneRef(), { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    await ops.handleSortChange('left', 'added')
    await ops.setSort('added', 'asc', 'left')

    expect(sortSnapshotSpy).toHaveBeenCalledTimes(2)
    expect(addToastForPaneSpy).not.toHaveBeenCalled()
  })

  it('says why when a snapshot pane’s sort lands in a fallback', async () => {
    getSnapshotSpy.mockReturnValue({ sort: null })
    sortSnapshotSpy.mockResolvedValue({ column: 'added', order: 'descending', effectiveSortBy: 'modified' })
    const { deps } = makeDeps(makeSnapshotPaneRef(), { sortBy: 'name', sortOrder: 'ascending' })

    await createSortOperations(deps).handleSortChange('left', 'added')

    expect(addToastForPaneSpy).toHaveBeenCalledWith('left', ADDED_FALLBACK, expect.objectContaining({ level: 'info' }))
  })

  // ── The header's Sort by menu ─────────────────────────────────────────────
  //
  // Opened over either pane's header, focused or not; the pick names the pane it
  // opened over, and picking the date already asked for does nothing.

  it('opens the menu for the pane it was asked over, with that pane’s requested column and the anchor', async () => {
    showDateSortMenuSpy.mockResolvedValue(undefined)
    const { deps } = makeDeps(makePaneRef(), { sortBy: 'added', sortOrder: 'descending' })

    await createSortOperations(deps).openDateSortMenu('right', { x: 10, y: 20 })

    expect(showDateSortMenuSpy).toHaveBeenCalledWith('right', 'added', { x: 10, y: 20 })
  })

  it('opens the menu with a snapshot pane’s own column, or none in the ranked order', async () => {
    showDateSortMenuSpy.mockResolvedValue(undefined)
    const { deps } = makeDeps(makeSnapshotPaneRef(), { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    getSnapshotSpy.mockReturnValue({ sort: { column: 'created', order: 'ascending' } })
    await ops.openDateSortMenu('left', null)
    getSnapshotSpy.mockReturnValue({ sort: null })
    await ops.openDateSortMenu('left', null)

    expect(showDateSortMenuSpy.mock.calls).toEqual([
      ['left', 'created', null],
      ['left', null, null],
    ])
  })

  it('sorts the pane a pick names by the new date, at its default order, whichever pane has focus', async () => {
    const { deps, setPaneSort } = makeDeps(makePaneRef(), { sortBy: 'name', sortOrder: 'ascending' })

    await createSortOperations(deps).pickSortColumn('right', 'added')

    expect(setPaneSort).toHaveBeenCalledWith('right', 'added', 'descending')
  })

  it('does nothing when the pick is the date the pane already asked for', async () => {
    const { deps, setPaneSort } = makeDeps(makePaneRef(), { sortBy: 'added', sortOrder: 'ascending' })

    await createSortOperations(deps).pickSortColumn('left', 'added')

    expect(resortListingSpy).not.toHaveBeenCalled()
    expect(setPaneSort).not.toHaveBeenCalled()
  })

  // In a fallback the pane still asked for Added, so Added is the checked item.
  it('does nothing when the pick is the requested date of a sort that fell back', async () => {
    landsInModified()
    const { deps } = makeDeps(makePaneRef(), { sortBy: 'added', sortOrder: 'descending' })

    await createSortOperations(deps).pickSortColumn('left', 'added')

    expect(resortListingSpy).not.toHaveBeenCalled()
  })

  it('sorts a snapshot pane by a picked date, and leaves its checked one alone', async () => {
    getSnapshotSpy.mockReturnValue({ sort: { column: 'modified', order: 'descending' } })
    const { deps } = makeDeps(makeSnapshotPaneRef(), { sortBy: 'name', sortOrder: 'ascending' })
    const ops = createSortOperations(deps)

    await ops.pickSortColumn('left', 'modified')
    expect(sortSnapshotSpy).not.toHaveBeenCalled()

    await ops.pickSortColumn('left', 'created')
    expect(sortSnapshotSpy).toHaveBeenCalledWith('sr-7', { column: 'created', order: 'descending' })
  })
})
