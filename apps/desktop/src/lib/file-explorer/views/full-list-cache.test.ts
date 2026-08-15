/**
 * The Full view's prefetch buffer. These pin the refresh policy, which is the part
 * that goes wrong invisibly: a hard reset where a soft refresh belonged flickers the
 * pane empty mid-bulk-operation, and a missed reset leaves the previous directory's
 * rows on screen.
 *
 * `file-list-utils` is mocked: it has its own suite, and stubbing it is what lets
 * these assert on WHEN a fetch happens and with what, not on IPC shapes.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FileEntry } from '../types'

const utils = vi.hoisted(() => ({
  fetchVisibleRange: vi.fn(),
  refetchIconsForEntries: vi.fn(),
  updateIndexSizesInPlace: vi.fn(),
  getDirStatsBatch: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({ getDirStatsBatch: utils.getDirStatsBatch }))
vi.mock('./file-list-utils', async () => {
  const actual = await vi.importActual<typeof import('./file-list-utils')>('./file-list-utils')
  return {
    ...actual,
    fetchVisibleRange: utils.fetchVisibleRange,
    refetchIconsForEntries: utils.refetchIconsForEntries,
    updateIndexSizesInPlace: utils.updateIndexSizesInPlace,
  }
})

import { createFullListCache, type FullListCacheDeps } from './full-list-cache.svelte'

function entry(name: string, overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name,
    path: `/dir/${name}`,
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'me',
    group: 'staff',
    iconId: `icon-${name}`,
    extendedMetadataLoaded: false,
    ...overrides,
  }
}

interface Props {
  listingId: string
  totalCount: number
  includeHidden: boolean
  hasParent: boolean
  parentPath: string
  currentPath: string
  cacheGeneration: number
  softRefreshTick: number
  staticEntries: FileEntry[] | undefined
}

let props: Props

function makeCache() {
  const deps: FullListCacheDeps = {
    listingId: () => props.listingId,
    volumeId: () => 'root',
    totalCount: () => props.totalCount,
    includeHidden: () => props.includeHidden,
    hasParent: () => props.hasParent,
    parentPath: () => props.parentPath,
    currentPath: () => props.currentPath,
    cacheGeneration: () => props.cacheGeneration,
    softRefreshTick: () => props.softRefreshTick,
    staticEntries: () => props.staticEntries,
    onSyncStatusRequest: () => undefined,
    onIndexStatusRequest: () => undefined,
    onFolderCoverageRequest: () => undefined,
  }
  return createFullListCache(deps)
}

beforeEach(() => {
  vi.clearAllMocks()
  props = {
    listingId: 'listing-1',
    totalCount: 100,
    includeHidden: false,
    hasParent: true,
    parentPath: '/',
    currentPath: '/dir',
    cacheGeneration: 0,
    softRefreshTick: 0,
    staticEntries: undefined,
  }
  utils.fetchVisibleRange.mockResolvedValue({ entries: [entry('a.txt'), entry('b.txt')], range: { start: 0, end: 2 } })
  utils.updateIndexSizesInPlace.mockResolvedValue(null)
  utils.getDirStatsBatch.mockResolvedValue([null])
})

describe('syncToProps', () => {
  it('is idle until the container has been measured', () => {
    const cache = makeCache()

    expect(cache.syncToProps(false)).toBe('idle')
  })

  it('is idle without a listing id', () => {
    props.listingId = ''

    expect(makeCache().syncToProps(true)).toBe('idle')
  })

  it('resets on the first pass, then settles', () => {
    const cache = makeCache()

    expect(cache.syncToProps(true)).toBe('reset')
    expect(cache.syncToProps(true)).toBe('none')
  })

  it.each([
    ['a navigation', () => (props.listingId = 'listing-2')],
    ['a hidden-files toggle', () => (props.includeHidden = true)],
    ['an explicit refresh or sort', () => (props.cacheGeneration = 1)],
  ])('hard-resets on %s', (_label, change) => {
    const cache = makeCache()
    cache.syncToProps(true)

    change()

    expect(cache.syncToProps(true)).toBe('reset')
  })

  it('wipes the entries on a hard reset so stale rows cannot survive a nav', async () => {
    const cache = makeCache()
    cache.syncToProps(true)
    await cache.fetch(0, 10)
    expect(cache.entries).toHaveLength(2)

    props.listingId = 'listing-2'
    cache.syncToProps(true)

    expect(cache.entries).toEqual([])
    expect(cache.range).toEqual({ start: 0, end: 0 })
  })

  it.each([
    ['a directory-diff burst', () => (props.softRefreshTick = 1)],
    ['an entry count change', () => (props.totalCount = 101)],
  ])('soft-refreshes on %s, keeping the rows on screen', async (_label, change) => {
    const cache = makeCache()
    cache.syncToProps(true)
    await cache.fetch(0, 10)

    change()

    expect(cache.syncToProps(true)).toBe('refresh')
    expect(cache.entries).toHaveLength(2)
  })

  it('stays idle on a static-entries pane, whatever the listing props do', () => {
    props.staticEntries = [entry('hit.txt')]
    const cache = makeCache()

    props.cacheGeneration = 7
    props.softRefreshTick = 3

    expect(cache.syncToProps(true)).toBe('idle')
  })
})

describe('fetch', () => {
  it('stores the fetched window', async () => {
    const cache = makeCache()

    await cache.fetch(0, 10)

    expect(cache.entries.map((e) => e.name)).toEqual(['a.txt', 'b.txt'])
    expect(cache.range).toEqual({ start: 0, end: 2 })
  })

  it('short-circuits when the range is already covered', async () => {
    utils.fetchVisibleRange.mockResolvedValue({ entries: [entry('a.txt')], range: { start: 0, end: 100 } })
    const cache = makeCache()
    await cache.fetch(0, 10)

    await cache.fetch(0, 10)

    expect(utils.fetchVisibleRange).toHaveBeenCalledOnce()
  })

  it('refetches a covered range when forced, because a diff can stale it in place', async () => {
    utils.fetchVisibleRange.mockResolvedValue({ entries: [entry('a.txt')], range: { start: 0, end: 100 } })
    const cache = makeCache()
    await cache.fetch(0, 10)

    await cache.fetch(0, 10, true)

    expect(utils.fetchVisibleRange).toHaveBeenCalledTimes(2)
    expect(utils.fetchVisibleRange).toHaveBeenLastCalledWith(expect.objectContaining({ force: true }))
  })

  it('never has two fetches in flight at once', async () => {
    let release: (value: unknown) => void = () => {}
    utils.fetchVisibleRange.mockReturnValue(
      new Promise((resolve) => {
        release = resolve
      }),
    )
    const cache = makeCache()

    const first = cache.fetch(0, 10)
    await cache.fetch(20, 30)

    expect(utils.fetchVisibleRange).toHaveBeenCalledOnce()
    release({ entries: [], range: { start: 0, end: 0 } })
    await first
  })

  it('swallows a fetch rejection instead of leaving the guard stuck', async () => {
    utils.fetchVisibleRange.mockRejectedValueOnce(new Error('listing gone'))
    const cache = makeCache()

    await expect(cache.fetch(0, 10)).resolves.toBeUndefined()

    utils.fetchVisibleRange.mockResolvedValue({ entries: [entry('a.txt')], range: { start: 0, end: 1 } })
    await cache.fetch(0, 10)
    expect(cache.entries).toHaveLength(1)
  })

  it('makes no IPC call on a static-entries pane', async () => {
    props.staticEntries = [entry('hit.txt')]

    await makeCache().fetch(0, 10)

    expect(utils.fetchVisibleRange).not.toHaveBeenCalled()
  })
})

describe('windowRows', () => {
  it('puts the synthetic ".." row at index 0 and shifts real files by one', async () => {
    const cache = makeCache()
    await cache.fetch(0, 10)

    const rows = cache.windowRows(0, 3)

    expect(rows.map((r) => [r.globalIndex, r.file.name])).toEqual([
      [0, '..'],
      [1, 'a.txt'],
      [2, 'b.txt'],
    ])
  })

  it('uses raw indices when there is no parent row', async () => {
    props.hasParent = false
    const cache = makeCache()
    await cache.fetch(0, 10)

    expect(cache.windowRows(0, 2).map((r) => [r.globalIndex, r.file.name])).toEqual([
      [0, 'a.txt'],
      [1, 'b.txt'],
    ])
  })

  it('skips rows outside the fetched range rather than rendering blanks', async () => {
    const cache = makeCache()
    await cache.fetch(0, 10)

    expect(cache.windowRows(1, 50)).toHaveLength(2)
  })
})

describe('getEntryAt', () => {
  it('resolves the ".." row and a real row', async () => {
    const cache = makeCache()
    await cache.fetch(0, 10)

    expect(cache.getEntryAt(0)?.name).toBe('..')
    expect(cache.getEntryAt(2)?.name).toBe('b.txt')
    expect(cache.getEntryAt(80)).toBeUndefined()
  })
})

describe('syncStaticEntries', () => {
  it('adopts the host pane array as the whole cache', () => {
    props.staticEntries = [entry('hit-1.txt'), entry('hit-2.txt')]
    const cache = makeCache()

    cache.syncStaticEntries()

    expect(cache.range).toEqual({ start: 0, end: 2 })
    expect(cache.windowRows(1, 3).map((r) => r.file.name)).toEqual(['hit-1.txt', 'hit-2.txt'])
  })

  it('leaves a normal pane cache untouched', async () => {
    const cache = makeCache()
    await cache.fetch(0, 10)

    cache.syncStaticEntries()

    expect(cache.entries).toHaveLength(2)
  })
})

describe('parent directory stats', () => {
  it('loads the current folder total for the ".." row', async () => {
    utils.getDirStatsBatch.mockResolvedValue([{ size: 42 }])
    const cache = makeCache()

    cache.syncParentDirStats()
    await vi.waitFor(() => {
      expect(cache.parentDirStats).toEqual({ size: 42 })
    })
    expect(utils.getDirStatsBatch).toHaveBeenCalledWith(['/dir'])
  })

  it.each([
    ['at a volume root', () => (props.hasParent = false)],
    ['with no current path', () => (props.currentPath = '')],
    ['on a search-results pane', () => (props.staticEntries = [])],
  ])('clears them %s', (_label, change) => {
    change()
    const cache = makeCache()

    cache.syncParentDirStats()

    expect(cache.parentDirStats).toBeNull()
    expect(utils.getDirStatsBatch).not.toHaveBeenCalled()
  })

  it('stays quiet when the index is not up yet', async () => {
    utils.getDirStatsBatch.mockRejectedValue(new Error('not indexed'))
    const cache = makeCache()

    cache.syncParentDirStats()
    await Promise.resolve()

    expect(cache.parentDirStats).toBeNull()
  })
})

describe('enrichment passes', () => {
  it('refreshes index sizes for the cached rows and the current folder', async () => {
    const cache = makeCache()
    await cache.fetch(0, 10)

    cache.refreshIndexSizes()

    expect(utils.updateIndexSizesInPlace).toHaveBeenCalledWith(expect.any(Array), '/dir')
  })

  it('skips the current folder at a volume root', async () => {
    props.hasParent = false
    const cache = makeCache()
    await cache.fetch(0, 10)

    cache.refreshIndexSizes()

    expect(utils.updateIndexSizesInPlace).toHaveBeenCalledWith(expect.any(Array), undefined)
  })

  it('does nothing with an empty cache and no ".." row', () => {
    props.hasParent = false
    const cache = makeCache()

    cache.refreshIndexSizes()
    cache.refetchIcons()

    expect(utils.updateIndexSizesInPlace).not.toHaveBeenCalled()
    expect(utils.refetchIconsForEntries).not.toHaveBeenCalled()
  })

  it('re-fetches icons for the cached rows', async () => {
    const cache = makeCache()
    await cache.fetch(0, 10)

    cache.refetchIcons()

    expect(utils.refetchIconsForEntries).toHaveBeenCalledOnce()
  })
})
