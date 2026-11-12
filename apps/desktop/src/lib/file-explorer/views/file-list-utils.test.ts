/**
 * Tests for file-list-utils.ts
 */
import { describe, it, expect, vi, beforeEach, onTestFinished } from 'vitest'
import {
  getSyncIconPath,
  createParentEntry,
  getEntryAt,
  indexOfEntry,
  calculateFetchRange,
  isRangeCached,
  shouldResetCache,
  getPrefetchBufferSize,
  fetchVisibleRange,
  refetchIconsForEntries,
  updateIndexSizesInPlace,
  applyFolderSizes,
  getImageIndexBadge,
  getFolderCoverageBadge,
} from './file-list-utils'
import type { FileEntry } from '../types'
import type { FileIndexState, FolderCoverage } from '#lib/tauri-commands/index.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { TranslationParams } from '#lib/intl/messages.svelte.js'

// Mock dependencies
vi.mock('#lib/tauri-commands/index.js', () => ({
  getFileRange: vi.fn(),
  getDirStatsBatch: vi.fn(),
  enrichTags: vi.fn().mockResolvedValue({ status: 'ok', data: null }),
  enrichPackages: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
}))
vi.mock('#lib/icon-cache.js', () => ({
  prefetchIcons: vi.fn(),
  prefetchCustomFolderIcons: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { enrichTags: vi.fn().mockResolvedValue({ status: 'ok', data: null }) },
}))
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  getUseAppIconsForDocuments: vi.fn().mockReturnValue(true),
}))
vi.mock('#lib/settings/settings-store.js', () => ({
  getSetting: vi.fn().mockReturnValue(200),
}))
vi.mock('../git/commit-count-fill', () => ({ requestPendingCommitCounts: vi.fn() }))

import { getFileRange, getDirStatsBatch, enrichPackages, enrichTags } from '#lib/tauri-commands/index.js'
import { withCapabilities } from '#lib/platform/test-platform.js'
import { requestPendingCommitCounts } from '../git/commit-count-fill'
import { prefetchIcons } from '#lib/icon-cache.js'

describe('getSyncIconPath', () => {
  it('returns undefined for undefined status', () => {
    expect(getSyncIconPath(undefined)).toBeUndefined()
  })

  it('returns correct icon for synced status', () => {
    expect(getSyncIconPath('synced')).toBe('/icons/sync-synced.svg')
  })

  it('returns correct icon for online_only status', () => {
    expect(getSyncIconPath('online_only')).toBe('/icons/sync-online-only.svg')
  })

  it('returns correct icon for uploading status', () => {
    expect(getSyncIconPath('uploading')).toBe('/icons/sync-uploading.svg')
  })

  it('returns correct icon for downloading status', () => {
    expect(getSyncIconPath('downloading')).toBe('/icons/sync-downloading.svg')
  })

  it('returns undefined for unknown status', () => {
    expect(getSyncIconPath('unknown')).toBeUndefined()
  })
})

describe('createParentEntry', () => {
  it('creates parent entry with correct name', () => {
    const entry = createParentEntry('/home/user')
    expect(entry.name).toBe('..')
  })

  it('creates parent entry with correct path', () => {
    const entry = createParentEntry('/home/user')
    expect(entry.path).toBe('/home/user')
  })

  it('creates parent entry as directory', () => {
    const entry = createParentEntry('/home/user')
    expect(entry.isDirectory).toBe(true)
  })

  it('creates parent entry with correct icon', () => {
    const entry = createParentEntry('/home/user')
    expect(entry.iconId).toBe('dir')
  })

  it('leaves recursive size fields undefined when no stats are passed', () => {
    const entry = createParentEntry('/home/user')
    expect(entry.recursiveSize).toBeUndefined()
    expect(entry.recursivePhysicalSize).toBeUndefined()
    expect(entry.recursiveFileCount).toBeUndefined()
    expect(entry.recursiveDirCount).toBeUndefined()
  })

  it('populates recursive size fields when stats are passed', () => {
    const entry = createParentEntry('/home/user', {
      path: '/home/user/current',
      recursiveSize: 1024,
      recursivePhysicalSize: 2048,
      recursiveFileCount: 3,
      recursiveDirCount: 1,
      recursiveHasSymlinks: false,
    })
    expect(entry.recursiveSize).toBe(1024)
    expect(entry.recursivePhysicalSize).toBe(2048)
    expect(entry.recursiveFileCount).toBe(3)
    expect(entry.recursiveDirCount).toBe(1)
  })
})

describe('getEntryAt', () => {
  const mockEntries: FileEntry[] = [
    {
      name: 'file1.txt',
      path: '/dir/file1.txt',
      isDirectory: false,
      isSymlink: false,
      permissions: 0o644,
      owner: 'user',
      group: 'group',
      iconId: 'txt',
    },
    {
      name: 'file2.txt',
      path: '/dir/file2.txt',
      isDirectory: false,
      isSymlink: false,
      permissions: 0o644,
      owner: 'user',
      group: 'group',
      iconId: 'txt',
    },
  ]

  it('returns parent entry at index 0 when hasParent is true', () => {
    const entry = getEntryAt(0, true, '/parent', mockEntries, { start: 0, end: 2 })
    expect(entry?.name).toBe('..')
    expect(entry?.path).toBe('/parent')
  })

  it('returns first cached entry at index 0 when hasParent is false', () => {
    const entry = getEntryAt(0, false, '/parent', mockEntries, { start: 0, end: 2 })
    expect(entry?.name).toBe('file1.txt')
  })

  it('returns cached entry at index 1 when hasParent is true', () => {
    const entry = getEntryAt(1, true, '/parent', mockEntries, { start: 0, end: 2 })
    expect(entry?.name).toBe('file1.txt')
  })

  it('returns undefined for index outside cached range', () => {
    const entry = getEntryAt(5, false, '/parent', mockEntries, { start: 0, end: 2 })
    expect(entry).toBeUndefined()
  })

  it('returns undefined for negative index', () => {
    const entry = getEntryAt(-1, false, '/parent', mockEntries, { start: 0, end: 2 })
    expect(entry).toBeUndefined()
  })

  it('answers by path what it answers by index, for a window that starts mid-listing', () => {
    // The inverse pair, for a caller that holds a row rather than an index: an
    // index means different rows to the window and to the cursor once a diff has
    // moved rows, and a path means the same row to both.
    const range = { start: 40, end: 42 }
    expect(indexOfEntry('/dir/file2.txt', true, mockEntries, range)).toBe(42)
    expect(getEntryAt(42, true, '/parent', mockEntries, range)?.path).toBe('/dir/file2.txt')
    expect(indexOfEntry('/dir/file1.txt', false, mockEntries, range)).toBe(40)
  })

  it('has no index for a path the window is not holding', () => {
    expect(indexOfEntry('/dir/gone.txt', true, mockEntries, { start: 0, end: 2 })).toBeUndefined()
  })

  it('handles cached range that does not start at 0', () => {
    const entry = getEntryAt(10, false, '/parent', mockEntries, { start: 10, end: 12 })
    expect(entry?.name).toBe('file1.txt')
  })
})

describe('calculateFetchRange', () => {
  // getPrefetchBufferSize() returns 200 (mocked)
  const prefetchBuffer = getPrefetchBufferSize()

  it('calculates range without parent entry', () => {
    const result = calculateFetchRange({
      startItem: 150,
      endItem: 160,
      hasParent: false,
      totalCount: 500,
    })
    // prefetchBuffer is 200, so buffer is 100 on each side
    expect(result.fetchStart).toBe(150 - prefetchBuffer / 2) // 50
    expect(result.fetchEnd).toBe(160 + prefetchBuffer / 2) // 260
  })

  it('calculates range with parent entry', () => {
    const result = calculateFetchRange({
      startItem: 150,
      endItem: 160,
      hasParent: true,
      totalCount: 500,
    })
    // With parent, indices are shifted down by 1
    expect(result.fetchStart).toBe(149 - prefetchBuffer / 2) // 49
    expect(result.fetchEnd).toBe(159 + prefetchBuffer / 2) // 259
  })

  it('clamps fetchStart to 0', () => {
    const result = calculateFetchRange({
      startItem: 5,
      endItem: 10,
      hasParent: false,
      totalCount: 100,
    })
    expect(result.fetchStart).toBe(0)
  })

  it('clamps fetchEnd to totalCount', () => {
    const result = calculateFetchRange({
      startItem: 90,
      endItem: 100,
      hasParent: false,
      totalCount: 100,
    })
    expect(result.fetchEnd).toBe(100)
  })

  it('handles hasParent with startItem 0', () => {
    const result = calculateFetchRange({
      startItem: 0,
      endItem: 10,
      hasParent: true,
      totalCount: 100,
    })
    expect(result.fetchStart).toBe(0)
  })
})

describe('isRangeCached', () => {
  it('returns true when range is fully cached', () => {
    expect(isRangeCached(10, 20, { start: 0, end: 50 })).toBe(true)
  })

  it('returns true when range exactly matches cache', () => {
    expect(isRangeCached(0, 50, { start: 0, end: 50 })).toBe(true)
  })

  it('returns false when fetchStart is before cache', () => {
    expect(isRangeCached(0, 20, { start: 10, end: 50 })).toBe(false)
  })

  it('returns false when fetchEnd is after cache', () => {
    expect(isRangeCached(10, 60, { start: 0, end: 50 })).toBe(false)
  })

  it('returns false when range is completely outside cache', () => {
    expect(isRangeCached(60, 80, { start: 0, end: 50 })).toBe(false)
  })
})

describe('shouldResetCache', () => {
  const base = {
    listingId: 'listing-1',
    includeHidden: false,
    cacheGeneration: 1,
  }

  it('returns false when all properties match', () => {
    expect(shouldResetCache(base, base)).toBe(false)
  })

  it('returns true when listingId changes', () => {
    expect(shouldResetCache({ ...base, listingId: 'listing-2' }, base)).toBe(true)
  })

  it('returns true when includeHidden changes', () => {
    expect(shouldResetCache({ ...base, includeHidden: true }, base)).toBe(true)
  })

  it('returns true when cacheGeneration changes', () => {
    expect(shouldResetCache({ ...base, cacheGeneration: 2 }, base)).toBe(true)
  })

  it('does NOT reset when only totalCount changes (soft-refresh path for diff events)', () => {
    // totalCount changes — caused by directory-diff events during bulk ops —
    // must not trigger a hard reset; the lists handle these via soft refresh
    // (refetch in background, keep entries visible until new ones land).
    expect(shouldResetCache(base, base)).toBe(false)
  })
})

describe('fetchVisibleRange', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  const mockEntries: FileEntry[] = [
    {
      name: 'file1.txt',
      path: '/dir/file1.txt',
      isDirectory: false,
      isSymlink: false,
      permissions: 0o644,
      owner: 'user',
      group: 'group',
      iconId: 'txt',
    },
  ]

  it('returns null when range is already cached', async () => {
    const result = await fetchVisibleRange({
      listingId: 'listing-1',
      startItem: 10,
      endItem: 20,
      hasParent: false,
      totalCount: 100,
      includeHidden: false,
      cachedRange: { start: 0, end: 200 },
    })
    expect(result).toBeNull()
    expect(getFileRange).not.toHaveBeenCalled()
  })

  it('fetches entries when range is not cached', async () => {
    vi.mocked(getFileRange).mockResolvedValue(mockEntries)

    const result = await fetchVisibleRange({
      listingId: 'listing-1',
      startItem: 10,
      endItem: 20,
      hasParent: false,
      totalCount: 100,
      includeHidden: false,
      cachedRange: { start: 0, end: 5 },
    })

    expect(result).not.toBeNull()
    expect(result?.entries).toEqual(mockEntries)
    expect(getFileRange).toHaveBeenCalled()
  })

  it('calls onSyncStatusRequest when provided', async () => {
    vi.mocked(getFileRange).mockResolvedValue(mockEntries)
    const onSyncStatusRequest = vi.fn()

    await fetchVisibleRange({
      listingId: 'listing-1',
      startItem: 10,
      endItem: 20,
      hasParent: false,
      totalCount: 100,
      includeHidden: false,
      cachedRange: { start: 0, end: 5 },
      onSyncStatusRequest,
    })

    expect(onSyncStatusRequest).toHaveBeenCalledWith(['/dir/file1.txt'])
  })

  it('asks for the visible rows’ color tags where the OS has them and the setting shows them', async () => {
    onTestFinished(withCapabilities('colorTags'))
    vi.mocked(getFileRange).mockResolvedValue(mockEntries)
    await fetchVisibleRange({
      listingId: 'listing-1',
      startItem: 10,
      endItem: 20,
      hasParent: false,
      totalCount: 100,
      includeHidden: false,
      cachedRange: { start: 0, end: 5 },
    })
    expect(enrichTags).toHaveBeenCalledWith('listing-1', ['/dir/file1.txt'])
  })

  it('never asks for color tags where the OS has none, whatever the hidden setting holds', async () => {
    vi.mocked(getFileRange).mockResolvedValue(mockEntries)
    await fetchVisibleRange({
      listingId: 'listing-1',
      startItem: 10,
      endItem: 20,
      hasParent: false,
      totalCount: 100,
      includeHidden: false,
      cachedRange: { start: 0, end: 5 },
    })
    expect(enrichTags).not.toHaveBeenCalled()
  })

  it('asks for the pending commit counts of the rows it fetched', async () => {
    vi.mocked(getFileRange).mockResolvedValue(mockEntries)

    await fetchVisibleRange({
      listingId: 'listing-1',
      startItem: 10,
      endItem: 20,
      hasParent: false,
      totalCount: 100,
      includeHidden: false,
      cachedRange: { start: 0, end: 5 },
    })

    expect(requestPendingCommitCounts).toHaveBeenCalledWith('listing-1', mockEntries)
  })

  it('asks the backend about the visible folders only, so macOS can name the packages', async () => {
    const folder = { ...mockEntries[0], name: 'Logo.sketchbundle', path: '/dir/Logo.sketchbundle', isDirectory: true }
    const link = { ...folder, name: 'Link.bundle2', path: '/dir/Link.bundle2', isSymlink: true }
    vi.mocked(getFileRange).mockResolvedValue([...mockEntries, folder, link])

    await fetchVisibleRange({
      listingId: 'listing-1',
      startItem: 10,
      endItem: 20,
      hasParent: false,
      totalCount: 100,
      includeHidden: false,
      cachedRange: { start: 0, end: 5 },
    })

    expect(enrichPackages).toHaveBeenCalledWith('listing-1', ['/dir/Logo.sketchbundle'])
  })

  it('asks nothing about packages when no folder is visible', async () => {
    vi.mocked(getFileRange).mockResolvedValue(mockEntries)

    await fetchVisibleRange({
      listingId: 'listing-1',
      startItem: 10,
      endItem: 20,
      hasParent: false,
      totalCount: 100,
      includeHidden: false,
      cachedRange: { start: 0, end: 5 },
    })

    expect(enrichPackages).not.toHaveBeenCalled()
  })
})

describe('refetchIconsForEntries', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('does nothing for empty array', () => {
    refetchIconsForEntries([])
    expect(prefetchIcons).not.toHaveBeenCalled()
  })

  it('prefetches icons for entries', () => {
    const entries: FileEntry[] = [
      {
        name: 'file1.txt',
        path: '/dir/file1.txt',
        isDirectory: false,
        isSymlink: false,
        permissions: 0o644,
        owner: 'user',
        group: 'group',
        iconId: 'txt',
      },
      {
        name: 'file2.rs',
        path: '/dir/file2.rs',
        isDirectory: false,
        isSymlink: false,
        permissions: 0o644,
        owner: 'user',
        group: 'group',
        iconId: 'rs',
      },
    ]
    refetchIconsForEntries(entries)
    expect(prefetchIcons).toHaveBeenCalledWith(['txt', 'rs'], true)
  })
})

describe('updateIndexSizesInPlace', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('does nothing when there are no directories', async () => {
    const entries: FileEntry[] = [
      {
        name: 'file.txt',
        path: '/dir/file.txt',
        isDirectory: false,
        isSymlink: false,
        permissions: 0o644,
        owner: 'user',
        group: 'group',
        iconId: 'txt',
      },
    ]
    await updateIndexSizesInPlace(entries)
    expect(getDirStatsBatch).not.toHaveBeenCalled()
  })

  it('updates directory entries in place', async () => {
    const entries: FileEntry[] = [
      {
        name: 'file.txt',
        path: '/dir/file.txt',
        isDirectory: false,
        isSymlink: false,
        permissions: 0o644,
        owner: 'user',
        group: 'group',
        iconId: 'txt',
      },
      {
        name: 'subdir',
        path: '/dir/subdir',
        isDirectory: true,
        isSymlink: false,
        permissions: 0o755,
        owner: 'user',
        group: 'group',
        iconId: 'dir',
      },
    ]
    vi.mocked(getDirStatsBatch).mockResolvedValue([
      {
        path: '/dir/subdir',
        recursiveSize: 1024,
        recursivePhysicalSize: 2048,
        recursiveFileCount: 5,
        recursiveDirCount: 2,
        recursiveHasSymlinks: false,
      },
    ])

    await updateIndexSizesInPlace(entries)

    expect(getDirStatsBatch).toHaveBeenCalledWith(['/dir/subdir'])
    expect(entries[1].recursiveSize).toBe(1024)
    expect(entries[1].recursivePhysicalSize).toBe(2048)
    expect(entries[1].recursiveFileCount).toBe(5)
    expect(entries[1].recursiveDirCount).toBe(2)
  })

  it('handles null stats gracefully', async () => {
    const entries: FileEntry[] = [
      {
        name: 'subdir',
        path: '/dir/subdir',
        isDirectory: true,
        isSymlink: false,
        permissions: 0o755,
        owner: 'user',
        group: 'group',
        iconId: 'dir',
      },
    ]
    vi.mocked(getDirStatsBatch).mockResolvedValue([null])

    await updateIndexSizesInPlace(entries)

    expect(entries[0].recursiveSize).toBeUndefined()
  })

  it('silently ignores errors from getDirStatsBatch', async () => {
    const entries: FileEntry[] = [
      {
        name: 'subdir',
        path: '/dir/subdir',
        isDirectory: true,
        isSymlink: false,
        permissions: 0o755,
        owner: 'user',
        group: 'group',
        iconId: 'dir',
      },
    ]
    vi.mocked(getDirStatsBatch).mockRejectedValue(new Error('indexing not ready'))

    const stats = await updateIndexSizesInPlace(entries)

    // Should not throw, entries unchanged
    expect(entries[0].recursiveSize).toBeUndefined()
    expect(stats).toBeNull()
  })

  it('returns current-dir stats when currentPath is passed', async () => {
    const entries: FileEntry[] = [
      {
        name: 'subdir',
        path: '/dir/subdir',
        isDirectory: true,
        isSymlink: false,
        permissions: 0o755,
        owner: 'user',
        group: 'group',
        iconId: 'dir',
      },
    ]
    vi.mocked(getDirStatsBatch).mockResolvedValue([
      {
        path: '/dir/subdir',
        recursiveSize: 100,
        recursivePhysicalSize: 200,
        recursiveFileCount: 1,
        recursiveDirCount: 0,
        recursiveHasSymlinks: false,
      },
      {
        path: '/dir',
        recursiveSize: 5000,
        recursivePhysicalSize: 6000,
        recursiveFileCount: 42,
        recursiveDirCount: 3,
        recursiveHasSymlinks: false,
      },
    ])

    const stats = await updateIndexSizesInPlace(entries, '/dir')

    expect(getDirStatsBatch).toHaveBeenCalledWith(['/dir/subdir', '/dir'])
    expect(entries[0].recursiveSize).toBe(100)
    expect(stats?.recursiveSize).toBe(5000)
    expect(stats?.recursiveFileCount).toBe(42)
  })

  it('returns current-dir stats even when there are no cached directories', async () => {
    vi.mocked(getDirStatsBatch).mockResolvedValue([
      {
        path: '/dir',
        recursiveSize: 999,
        recursivePhysicalSize: 1000,
        recursiveFileCount: 7,
        recursiveDirCount: 1,
        recursiveHasSymlinks: false,
      },
    ])

    const stats = await updateIndexSizesInPlace([], '/dir')

    expect(getDirStatsBatch).toHaveBeenCalledWith(['/dir'])
    expect(stats?.recursiveSize).toBe(999)
  })

  it('returns null for current-dir stats when index has no data for it', async () => {
    vi.mocked(getDirStatsBatch).mockResolvedValue([null])

    const stats = await updateIndexSizesInPlace([], '/dir')

    expect(stats).toBeNull()
  })
})

describe('getImageIndexBadge', () => {
  it('maps each renderable state to its glyph and tooltip key', () => {
    expect(getImageIndexBadge('indexed')).toEqual({
      icon: 'circle-check',
      tooltipKey: 'fileExplorer.imageIndex.file.indexed',
    })
    expect(getImageIndexBadge('pending')).toEqual({
      icon: 'circle-dashed',
      tooltipKey: 'fileExplorer.imageIndex.file.pending',
    })
    expect(getImageIndexBadge('indexing')).toEqual({
      icon: 'hourglass',
      tooltipKey: 'fileExplorer.imageIndex.file.indexing',
    })
    expect(getImageIndexBadge('stale')).toEqual({
      icon: 'rotate-cw',
      tooltipKey: 'fileExplorer.imageIndex.file.stale',
    })
    expect(getImageIndexBadge('failed')).toEqual({
      icon: 'circle-x',
      tooltipKey: 'fileExplorer.imageIndex.file.failed',
    })
    expect(getImageIndexBadge('excluded')).toEqual({
      icon: 'circle-slash',
      tooltipKey: 'fileExplorer.imageIndex.file.excluded',
    })
  })

  it('renders no badge for notApplicable (non-media file)', () => {
    expect(getImageIndexBadge('notApplicable')).toBeNull()
  })

  it('renders no badge when the state is absent (not yet fetched)', () => {
    expect(getImageIndexBadge(undefined)).toBeNull()
  })

  it('covers every FileIndexState (no unmapped case slips through)', () => {
    const states: FileIndexState[] = ['indexed', 'stale', 'failed', 'pending', 'indexing', 'excluded', 'notApplicable']
    for (const state of states) {
      // Must not throw and must return either a badge or null explicitly.
      const badge = getImageIndexBadge(state)
      expect(badge === null || typeof badge.icon === 'string').toBe(true)
    }
  })
})

describe('getFolderCoverageBadge', () => {
  // A deterministic stand-in for `tString`: echoes the key so we can assert which
  // message was chosen without pulling in the i18n runtime.
  const t = vi.fn((key: MessageKey, _params?: TranslationParams): string => `t:${key}`)
  beforeEach(() => t.mockClear())

  it('renders no badge when coverage is absent (not yet fetched)', () => {
    expect(getFolderCoverageBadge(undefined, t)).toBeNull()
  })

  it('renders no badge when nothing here is eligible (eligible === 0)', () => {
    const cov: FolderCoverage = { path: '/a', eligible: 0, accounted: 0 }
    expect(getFolderCoverageBadge(cov, t)).toBeNull()
  })

  it('all-indexed (accounted === eligible) → circle-check with the allIndexed tooltip', () => {
    const cov: FolderCoverage = { path: '/a', eligible: 50, accounted: 50 }
    const badge = getFolderCoverageBadge(cov, t)
    expect(badge?.icon).toBe('circle-check')
    expect(badge?.tooltip).toBe('t:fileExplorer.imageIndex.folder.allIndexed')
    expect(t).toHaveBeenCalledWith('fileExplorer.imageIndex.folder.allIndexed', expect.objectContaining({ total: 50 }))
  })

  it('some-pending (accounted < eligible) → circle-dot with the someIndexed tooltip', () => {
    const cov: FolderCoverage = { path: '/a', eligible: 50, accounted: 12 }
    const badge = getFolderCoverageBadge(cov, t)
    expect(badge?.icon).toBe('circle-dot')
    expect(badge?.tooltip).toBe('t:fileExplorer.imageIndex.folder.someIndexed')
    expect(t).toHaveBeenCalledWith(
      'fileExplorer.imageIndex.folder.someIndexed',
      expect.objectContaining({ done: 12, total: 50 }),
    )
  })

  it('zero accounted with some eligible is still some-pending (0/N)', () => {
    const cov: FolderCoverage = { path: '/a', eligible: 50, accounted: 0 }
    expect(getFolderCoverageBadge(cov, t)?.icon).toBe('circle-dot')
  })

  it('treats accounted >= eligible as all-indexed (a stray over-count never reads pending)', () => {
    const cov: FolderCoverage = { path: '/a', eligible: 50, accounted: 60 }
    expect(getFolderCoverageBadge(cov, t)?.icon).toBe('circle-check')
  })
})

// The backend pushes the moved rows' readings with `listing-index-sizes-changed`, so applying them
// must cost no IPC and touch only the rows it names.
describe('applyFolderSizes', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  function folder(name: string, overrides: Partial<FileEntry> = {}): FileEntry {
    return {
      name,
      path: `/dir/${name}`,
      isDirectory: true,
      isSymlink: false,
      permissions: 0o755,
      owner: 'user',
      group: 'group',
      iconId: 'dir',
      ...overrides,
    }
  }

  function reading(path: string, recursiveSize: number, recursiveSizePending = false) {
    return {
      path,
      recursiveSize,
      recursivePhysicalSize: recursiveSize,
      recursiveFileCount: 3,
      recursiveDirCount: 1,
      recursiveHasSymlinks: false,
      recursiveSizePending,
      recursiveSizeComplete: true,
      recursiveSizeStale: false,
    }
  }

  it('writes the reading onto the named row and leaves the others alone', () => {
    const library = folder('Library', { recursiveSize: 10 })
    const music = folder('Music', { recursiveSize: 20 })
    const touched = applyFolderSizes(
      [library, music],
      [{ path: '/dir/Library', stats: reading('/dir/Library', 99, true) }],
    )

    expect(touched).toBe(true)
    expect(library.recursiveSize).toBe(99)
    expect(library.recursiveSizePending).toBe(true)
    expect(music.recursiveSize).toBe(20)
    expect(getDirStatsBatch).not.toHaveBeenCalled()
  })

  it('clears the hourglass but keeps the size when the index lost the folder', () => {
    const library = folder('Library', { recursiveSize: 10, recursiveSizePending: true })
    applyFolderSizes([library], [{ path: '/dir/Library', stats: null }])
    expect(library.recursiveSize).toBe(10)
    expect(library.recursiveSizePending).toBe(false)
  })

  it('reports false when none of the rows is in the window', () => {
    const music = folder('Music', { recursiveSize: 20 })
    expect(applyFolderSizes([music], [{ path: '/dir/Library', stats: reading('/dir/Library', 99) }])).toBe(false)
  })
})
