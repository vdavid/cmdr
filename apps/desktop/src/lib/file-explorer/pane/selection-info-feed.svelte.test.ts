/**
 * Tests for `selection-info-feed.svelte.ts`, the pane's cursor-entry + listing-stats
 * feed. They pin:
 * - the `..` row resolves synthetically at index 0 instead of hitting the backend,
 * - the parent-row offset (`hasParent`) on both the entry fetch and the stats indices,
 * - an empty listing and a missing listing id both resolve to `null` without an IPC,
 * - a folder under the cursor gets its recursive size enriched, `..` never does,
 * - stats send `undefined` for an empty selection, backend-adjusted indices otherwise,
 * - the cursor effect debounces the fetch and pushes to MCP, but only with a settled listing,
 * - the search-results pane mirrors the snapshot row under the cursor, and clamps
 *   past-the-end cursors to `null`,
 * - `cleanup()` drops the pending debounce and throttle.
 *
 * Uses Svelte runes, so the filename carries the `.svelte.` infix.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest'
import { flushSync } from 'svelte'
import { toCanonical, type CanonicalPath } from '$lib/path/canonical'
import type { SearchSnapshot } from '$lib/search/snapshot-store.svelte'

const { ipc, listUtils } = vi.hoisted<{
  ipc: { getFileAt: Mock; getListingStats: Mock }
  listUtils: { updateIndexSizesInPlace: Mock }
}>(() => ({
  ipc: { getFileAt: vi.fn(), getListingStats: vi.fn() },
  listUtils: { updateIndexSizesInPlace: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({
  getFileAt: ipc.getFileAt,
  getListingStats: ipc.getListingStats,
}))
vi.mock('../views/file-list-utils', () => ({ updateIndexSizesInPlace: listUtils.updateIndexSizesInPlace }))

import { createSelectionInfoFeed, type SelectionInfoFeed } from './selection-info-feed.svelte'

const canonical = (path: string): CanonicalPath => toCanonical(path, '/Users/test')

function fileEntry(name: string, isDirectory = false) {
  return {
    name,
    path: `/dir/${name}`,
    isDirectory,
    isSymlink: false,
    permissions: 0o644,
    owner: 'user',
    group: 'staff',
    iconId: isDirectory ? 'dir' : 'file',
    extendedMetadataLoaded: true,
  }
}

function snapshot(entries: { name: string; path: string }[]): SearchSnapshot {
  return {
    id: 'sr-1',
    entries: entries.map((e) => ({
      name: e.name,
      path: e.path,
      parentPath: '/dir',
      isDirectory: false,
      size: 12,
      modifiedAt: 1700,
      iconId: 'file',
    })),
  } as unknown as SearchSnapshot
}

describe('createSelectionInfoFeed', () => {
  let dispose: (() => void) | undefined

  beforeEach(() => {
    vi.useFakeTimers()
    ipc.getFileAt.mockReset().mockResolvedValue(fileEntry('a.txt'))
    ipc.getListingStats.mockReset().mockResolvedValue({ totalCount: 3 })
    listUtils.updateIndexSizesInPlace.mockReset()
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
    vi.useRealTimers()
  })

  function create(
    opts: {
      listingId?: string
      loading?: boolean
      totalCount?: number
      cursorIndex?: number
      hasParent?: boolean
      canonicalPath?: CanonicalPath | null
      isSearchResultsView?: boolean
      searchSnapshot?: SearchSnapshot | undefined
      selectedIndices?: number[]
    } = {},
  ) {
    let listingId = $state(opts.listingId ?? 'listing-1')
    const loading = $state(opts.loading ?? false)
    const totalCount = $state(opts.totalCount ?? 10)
    let cursorIndex = $state(opts.cursorIndex ?? 0)
    const hasParent = $state(opts.hasParent ?? false)
    const canonicalPath = $state<CanonicalPath | null>(opts.canonicalPath ?? canonical('/dir'))
    const isSearchResultsView = $state(opts.isSearchResultsView ?? false)
    let searchSnapshot = $state<SearchSnapshot | undefined>(opts.searchSnapshot)
    let selectedIndices = $state<number[]>(opts.selectedIndices ?? [])
    const syncMcp = vi.fn()
    const onCursorRow = vi.fn()
    let feed!: SelectionInfoFeed
    dispose = $effect.root(() => {
      feed = createSelectionInfoFeed({
        getListingId: () => listingId,
        getLoading: () => loading,
        getTotalCount: () => totalCount,
        getCursorIndex: () => cursorIndex,
        getHasParent: () => hasParent,
        getCanonicalPath: () => canonicalPath,
        getIncludeHidden: () => true,
        getIsSearchResultsView: () => isSearchResultsView,
        getSearchSnapshot: () => searchSnapshot,
        getSelectedIndices: () => selectedIndices,
        getSelectionSize: () => selectedIndices.length,
        syncMcp,
        onCursorRow,
      })
    })
    flushSync()
    return {
      feed,
      syncMcp,
      onCursorRow,
      setCursorIndex: (v: number) => {
        cursorIndex = v
        flushSync()
      },
      setListingId: (v: string) => {
        listingId = v
        flushSync()
      },
      setSelectedIndices: (v: number[]) => {
        selectedIndices = v
        flushSync()
      },
      setSnapshot: (v: SearchSnapshot | undefined) => {
        searchSnapshot = v
        flushSync()
      },
    }
  }

  describe('the entry under the cursor', () => {
    it('resolves the `..` row synthetically, without an IPC', async () => {
      const { feed } = create({ hasParent: true, cursorIndex: 0, canonicalPath: canonical('/dir/sub') })
      await feed.fetchEntry()
      expect(ipc.getFileAt).not.toHaveBeenCalled()
      expect(feed.entry?.name).toBe('..')
      expect(feed.entry?.path).toBe('/dir')
    })

    it('offsets the backend index by the `..` row', async () => {
      const { feed } = create({ hasParent: true, cursorIndex: 3 })
      await feed.fetchEntry()
      expect(ipc.getFileAt).toHaveBeenCalledWith('listing-1', 2, true)
    })

    it('uses the raw index when there is no `..` row', async () => {
      const { feed } = create({ hasParent: false, cursorIndex: 3 })
      await feed.fetchEntry()
      expect(ipc.getFileAt).toHaveBeenCalledWith('listing-1', 3, true)
    })

    it('clears without an IPC when the pane has no listing', async () => {
      const { feed } = create({ listingId: '' })
      await feed.fetchEntry()
      expect(ipc.getFileAt).not.toHaveBeenCalled()
      expect(feed.entry).toBeNull()
    })

    it('clears without an IPC on an empty listing (no spurious index-mismatch log)', async () => {
      const { feed } = create({ totalCount: 0 })
      await feed.fetchEntry()
      expect(ipc.getFileAt).not.toHaveBeenCalled()
      expect(feed.entry).toBeNull()
    })

    it('reports the row it read, with the index it read it at', async () => {
      ipc.getFileAt.mockResolvedValue(fileEntry('b.txt'))
      const { feed, onCursorRow } = create({ hasParent: true, cursorIndex: 3 })
      await feed.fetchEntry()
      expect(onCursorRow).toHaveBeenCalledWith(3, '/dir/b.txt')
    })

    it('reports nothing when the cursor moved while the row was being read', async () => {
      let resolveRead!: (entry: unknown) => void
      ipc.getFileAt.mockReturnValue(new Promise((resolve) => (resolveRead = resolve)))
      const { feed, onCursorRow, setCursorIndex } = create({ cursorIndex: 3 })
      const read = feed.fetchEntry()
      setCursorIndex(4)
      resolveRead(fileEntry('d.txt'))
      await read
      expect(onCursorRow).not.toHaveBeenCalled()
    })

    it('reports nothing when the listing changed while the row was being read', async () => {
      let resolveRead!: (entry: unknown) => void
      ipc.getFileAt.mockReturnValue(new Promise((resolve) => (resolveRead = resolve)))
      const { feed, onCursorRow, setListingId } = create({ cursorIndex: 3 })
      const read = feed.fetchEntry()
      setListingId('listing-2')
      resolveRead(fileEntry('d.txt'))
      await read
      expect(onCursorRow).not.toHaveBeenCalled()
    })

    it('reports no row for `..`, whose path is the parent folder', async () => {
      const { feed, onCursorRow } = create({ hasParent: true, cursorIndex: 0, canonicalPath: canonical('/dir/sub') })
      await feed.fetchEntry()
      expect(onCursorRow).not.toHaveBeenCalled()
    })

    it('falls back to null when the fetch throws', async () => {
      ipc.getFileAt.mockRejectedValue(new Error('gone'))
      const { feed } = create()
      await feed.fetchEntry()
      expect(feed.entry).toBeNull()
    })

    it('enriches a folder under the cursor with its recursive size', async () => {
      ipc.getFileAt.mockResolvedValue(fileEntry('sub', true))
      const { feed } = create()
      await feed.fetchEntry()
      expect(listUtils.updateIndexSizesInPlace).toHaveBeenCalledTimes(1)
    })

    it('never enriches `..`, whose path points at the wrong folder', async () => {
      const { feed } = create({ hasParent: true, cursorIndex: 0, canonicalPath: canonical('/dir/sub') })
      await feed.fetchEntry()
      expect(listUtils.updateIndexSizesInPlace).not.toHaveBeenCalled()
    })

    it('leaves a plain file unenriched', async () => {
      const { feed } = create()
      await feed.fetchEntry()
      expect(listUtils.updateIndexSizesInPlace).not.toHaveBeenCalled()
    })

    it('clearEntry drops the current entry', async () => {
      const { feed } = create()
      await feed.fetchEntry()
      expect(feed.entry).not.toBeNull()
      feed.clearEntry()
      expect(feed.entry).toBeNull()
    })
  })

  describe('listing stats', () => {
    it('sends `undefined` indices for an empty selection', async () => {
      const { feed } = create()
      await feed.fetchStats()
      expect(ipc.getListingStats).toHaveBeenCalledWith('listing-1', true, undefined)
      expect(feed.stats).toEqual({ totalCount: 3 })
    })

    it('converts selected indices to backend indices across the `..` row', async () => {
      const { feed } = create({ hasParent: true, selectedIndices: [1, 2, 4] })
      await feed.fetchStats()
      expect(ipc.getListingStats).toHaveBeenCalledWith('listing-1', true, [0, 1, 3])
    })

    it('clears stats when the pane has no listing', async () => {
      const { feed } = create({ listingId: '' })
      await feed.fetchStats()
      expect(ipc.getListingStats).not.toHaveBeenCalled()
      expect(feed.stats).toBeNull()
    })

    it('falls back to null stats when the fetch throws', async () => {
      ipc.getListingStats.mockRejectedValue(new Error('gone'))
      const { feed } = create()
      await feed.fetchStats()
      expect(feed.stats).toBeNull()
    })
  })

  describe('reactive triggers', () => {
    it('debounces the cursor-move fetch and syncs MCP', async () => {
      const created = create()
      created.syncMcp.mockClear()
      ipc.getFileAt.mockClear()

      created.setCursorIndex(1)
      created.setCursorIndex(2)
      expect(created.syncMcp).toHaveBeenCalledTimes(2)
      expect(ipc.getFileAt).not.toHaveBeenCalled()

      await vi.advanceTimersByTimeAsync(20)
      expect(ipc.getFileAt).toHaveBeenCalledTimes(1)
      expect(ipc.getFileAt).toHaveBeenCalledWith('listing-1', 2, true)
    })

    it('stays quiet while the pane has no listing', async () => {
      const created = create({ listingId: '' })
      created.syncMcp.mockClear()
      created.setCursorIndex(1)
      await vi.advanceTimersByTimeAsync(50)
      expect(created.syncMcp).not.toHaveBeenCalled()
      expect(ipc.getFileAt).not.toHaveBeenCalled()
    })

    it('throttles the stats refetch on a selection change', async () => {
      const created = create()
      ipc.getListingStats.mockClear()
      created.setSelectedIndices([1])
      await vi.advanceTimersByTimeAsync(200)
      expect(ipc.getListingStats).toHaveBeenCalled()
    })
  })

  describe('the search-results mirror', () => {
    it('mirrors the snapshot row under the cursor as a FileEntry', () => {
      const created = create({
        isSearchResultsView: true,
        listingId: '',
        searchSnapshot: snapshot([{ name: 'one.txt', path: '/dir/one.txt' }]),
      })
      expect(created.feed.entry?.name).toBe('one.txt')
      expect(created.feed.entry?.path).toBe('/dir/one.txt')
      expect(created.feed.entry?.extendedMetadataLoaded).toBe(true)
    })

    it('reports each snapshot row the cursor lands on', () => {
      const created = create({
        isSearchResultsView: true,
        listingId: '',
        searchSnapshot: snapshot([
          { name: 'one.txt', path: '/dir/one.txt' },
          { name: 'two.txt', path: '/x/two.txt' },
        ]),
      })
      expect(created.onCursorRow).toHaveBeenLastCalledWith(0, '/dir/one.txt')
      created.setCursorIndex(1)
      expect(created.onCursorRow).toHaveBeenLastCalledWith(1, '/x/two.txt')
    })

    it('follows the cursor across snapshot rows', () => {
      const created = create({
        isSearchResultsView: true,
        listingId: '',
        searchSnapshot: snapshot([
          { name: 'one.txt', path: '/dir/one.txt' },
          { name: 'two.txt', path: '/dir/two.txt' },
        ]),
      })
      created.setCursorIndex(1)
      expect(created.feed.entry?.name).toBe('two.txt')
    })

    it('clears when the cursor points past the snapshot (post delete-sync)', () => {
      const created = create({
        isSearchResultsView: true,
        listingId: '',
        searchSnapshot: snapshot([{ name: 'one.txt', path: '/dir/one.txt' }]),
      })
      created.setCursorIndex(5)
      expect(created.feed.entry).toBeNull()
    })

    it('clears when the snapshot is gone', () => {
      const created = create({ isSearchResultsView: true, listingId: '', searchSnapshot: undefined })
      expect(created.feed.entry).toBeNull()
    })

    it('leaves a normal pane alone', async () => {
      const created = create({ searchSnapshot: snapshot([{ name: 'one.txt', path: '/dir/one.txt' }]) })
      await created.feed.fetchEntry()
      expect(created.feed.entry?.name).toBe('a.txt')
    })
  })

  describe('the search-results stats', () => {
    const twoRows = () =>
      snapshot([
        { name: 'one.txt', path: '/dir/one.txt' },
        { name: 'two.txt', path: '/dir/two.txt' },
      ])

    it('folds the snapshot instead of asking the backend, which has no listing to answer for', () => {
      const created = create({ isSearchResultsView: true, listingId: '', searchSnapshot: twoRows() })
      expect(ipc.getListingStats).not.toHaveBeenCalled()
      expect(created.feed.stats?.totalFiles).toBe(2)
      expect(created.feed.stats?.totalSize).toBe(24)
    })

    it('follows the selection without waiting out the backend throttle', () => {
      const created = create({ isSearchResultsView: true, listingId: '', searchSnapshot: twoRows() })
      created.setSelectedIndices([1])
      expect(created.feed.stats?.selectedFiles).toBe(1)
      expect(created.feed.stats?.selectedSize).toBe(12)
    })

    it('follows a still-running walk as rows land in the snapshot', () => {
      const created = create({
        isSearchResultsView: true,
        listingId: '',
        searchSnapshot: snapshot([{ name: 'one.txt', path: '/dir/one.txt' }]),
      })
      expect(created.feed.stats?.totalFiles).toBe(1)
      created.setSnapshot(twoRows())
      expect(created.feed.stats?.totalFiles).toBe(2)
    })

    it('clears the stats when the snapshot is gone', () => {
      const created = create({ isSearchResultsView: true, listingId: '', searchSnapshot: twoRows() })
      created.setSnapshot(undefined)
      expect(created.feed.stats).toBeNull()
    })
  })

  it('cleanup drops the pending debounce and throttle', async () => {
    const created = create()
    ipc.getFileAt.mockClear()
    created.setCursorIndex(4)
    created.feed.cleanup()
    await vi.advanceTimersByTimeAsync(500)
    expect(ipc.getFileAt).not.toHaveBeenCalled()
  })
})
