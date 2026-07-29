/**
 * Tests for `entries-snapshot.ts`, the pane's two materializers: the full entry
 * list the Selection dialog matches against, and the selected NAMES an operation
 * pins before it starts. They pin:
 * - the `..` row sits at index 0 so snapshot indices line up with selection indices,
 * - an empty or missing listing still yields the `..` row alone (not an empty list),
 * - a failed range fetch degrades to an empty list rather than throwing at the dialog,
 * - a search-results pane reads its rows out of the in-memory snapshot,
 * - the operation snapshot converts to backend indices, drops the `..` row, and
 *   short-circuits to `'all'` when everything is selected.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'
import { toCanonical, type CanonicalPath } from '$lib/path/canonical'
import type { SearchSnapshot } from '$lib/search/snapshot-store.svelte'

const { ipc } = vi.hoisted<{ ipc: { getFileRange: Mock; getFileAt: Mock } }>(() => ({
  ipc: { getFileRange: vi.fn(), getFileAt: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({ getFileRange: ipc.getFileRange, getFileAt: ipc.getFileAt }))

import { fetchEntriesSnapshot, fetchSelectedNames } from './entries-snapshot'

const canonical = (path: string): CanonicalPath => toCanonical(path, '/Users/test')

function backendEntry(name: string) {
  return {
    name,
    path: `/dir/${name}`,
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'user',
    group: 'staff',
    iconId: 'file',
    extendedMetadataLoaded: true,
  }
}

function snapshotOf(names: string[]): SearchSnapshot {
  return {
    id: 'sr-1',
    entries: names.map((name) => ({
      name,
      path: `/found/${name}`,
      parentPath: '/found',
      isDirectory: false,
      size: 10,
      modifiedAt: 5,
      iconId: 'file',
    })),
  } as unknown as SearchSnapshot
}

describe('fetchEntriesSnapshot', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.getFileRange.mockResolvedValue([backendEntry('a.txt'), backendEntry('b.txt')])
  })

  const base = {
    listingId: 'listing-1',
    totalCount: 2,
    hasParent: false,
    showHiddenFiles: true,
    canonicalPath: canonical('/dir/sub'),
    isSearchResultsView: false,
    searchSnapshot: undefined,
  }

  it('fetches the whole listing', async () => {
    const entries = await fetchEntriesSnapshot(base)
    expect(ipc.getFileRange).toHaveBeenCalledWith('listing-1', 0, 2, true)
    expect(entries.map((e) => e.name)).toEqual(['a.txt', 'b.txt'])
  })

  it('puts the `..` row at index 0 so indices line up with the selection', async () => {
    const entries = await fetchEntriesSnapshot({ ...base, hasParent: true })
    expect(entries.map((e) => e.name)).toEqual(['..', 'a.txt', 'b.txt'])
  })

  it('yields the `..` row alone for an empty listing', async () => {
    const entries = await fetchEntriesSnapshot({ ...base, hasParent: true, totalCount: 0 })
    expect(ipc.getFileRange).not.toHaveBeenCalled()
    expect(entries.map((e) => e.name)).toEqual(['..'])
  })

  it('yields nothing for an empty listing with no `..` row', async () => {
    expect(await fetchEntriesSnapshot({ ...base, totalCount: 0 })).toEqual([])
  })

  it('yields nothing before the home dir resolves, where `..` cannot be built', async () => {
    const entries = await fetchEntriesSnapshot({ ...base, hasParent: true, totalCount: 0, canonicalPath: null })
    expect(entries).toEqual([])
  })

  it('degrades to an empty list when the range fetch throws', async () => {
    ipc.getFileRange.mockRejectedValue(new Error('gone'))
    expect(await fetchEntriesSnapshot(base)).toEqual([])
  })

  it('honours the hidden-files toggle', async () => {
    await fetchEntriesSnapshot({ ...base, showHiddenFiles: false })
    expect(ipc.getFileRange).toHaveBeenCalledWith('listing-1', 0, 2, false)
  })

  describe('on a search-results pane', () => {
    it('reads the rows out of the in-memory snapshot', async () => {
      const entries = await fetchEntriesSnapshot({
        ...base,
        isSearchResultsView: true,
        searchSnapshot: snapshotOf(['/found/one.txt']),
      })
      expect(ipc.getFileRange).not.toHaveBeenCalled()
      expect(entries).toHaveLength(1)
      expect(entries[0]?.extendedMetadataLoaded).toBe(true)
    })

    it('yields nothing when the snapshot is gone', async () => {
      const entries = await fetchEntriesSnapshot({ ...base, isSearchResultsView: true, searchSnapshot: undefined })
      expect(entries).toEqual([])
    })
  })
})

describe('fetchSelectedNames', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.getFileAt.mockImplementation((_id: string, index: number) =>
      Promise.resolve(backendEntry(`file-${String(index)}`)),
    )
  })

  it('short-circuits to `all` when everything is selected', async () => {
    const names = await fetchSelectedNames({
      listingId: 'listing-1',
      includeHidden: true,
      hasParent: false,
      isAllSelected: true,
      selectedIndices: [0, 1, 2],
    })
    expect(names).toBe('all')
    expect(ipc.getFileAt).not.toHaveBeenCalled()
  })

  it('resolves each selected index to its name', async () => {
    const names = await fetchSelectedNames({
      listingId: 'listing-1',
      includeHidden: true,
      hasParent: false,
      isAllSelected: false,
      selectedIndices: [0, 2],
    })
    expect(names).toEqual(['file-0', 'file-2'])
  })

  it('converts to backend indices across the `..` row and skips the row itself', async () => {
    const names = await fetchSelectedNames({
      listingId: 'listing-1',
      includeHidden: true,
      hasParent: true,
      isAllSelected: false,
      selectedIndices: [0, 1, 3],
    })
    expect(ipc.getFileAt).toHaveBeenCalledTimes(2)
    expect(names).toEqual(['file-0', 'file-2'])
  })

  it('drops entries the backend no longer has', async () => {
    ipc.getFileAt.mockResolvedValue(null)
    const names = await fetchSelectedNames({
      listingId: 'listing-1',
      includeHidden: true,
      hasParent: false,
      isAllSelected: false,
      selectedIndices: [0],
    })
    expect(names).toEqual([])
  })
})
