/**
 * The snapshot pane's row order: the store's ranked/sorted model and the
 * tri-state cycle a column header walks.
 *
 * The invariant under test throughout: `snapshot.entries[i]` is what the user
 * sees at row `i`, whatever the order, because every consumer of a snapshot pane
 * (the rendered rows, F5/F6/F8, the clipboard, the context menu, MCP rows, the
 * selection remap) resolves an index against that one array.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { SearchResultEntry } from '$lib/ipc/bindings'

const { sortSearchResultsSpy, directorySortModeSpy } = vi.hoisted(() => ({
  sortSearchResultsSpy: vi.fn<() => Promise<number[]>>(),
  directorySortModeSpy: vi.fn<() => string>(() => 'likeFiles'),
}))

vi.mock('$lib/tauri-commands', () => ({ sortSearchResults: sortSearchResultsSpy }))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({ getDirectorySortMode: directorySortModeSpy }))

import {
  _resetForTesting,
  appendSnapshotEntries,
  getOrCreate,
  getSnapshot,
  removeEntryFromAllSnapshots,
  type SearchSnapshot,
} from './snapshot-store.svelte'
import { nextSnapshotSort, resortSnapshotIfSorted, sortSnapshot } from './snapshot-sort.svelte'

function makeEntry(name: string): SearchResultEntry {
  return {
    name,
    path: `/Users/test/${name}`,
    parentPath: '/Users/test',
    isDirectory: false,
    size: 100,
    modifiedAt: 1_700_000_000,
    iconId: 'ext:txt',
  }
}

function makeSnapshot(id: string, entries: SearchResultEntry[]): SearchSnapshot {
  return {
    id,
    query: 'foo',
    mode: 'filename',
    filters: {},
    scope: '',
    volumeId: 'root',
    caseSensitive: false,
    excludeSystemDirs: true,
    entries,
    totalCount: entries.length,
    createdAt: 1_700_000_000_000,
    label: 'Search: foo',
    sort: null,
  }
}

/** The names in a snapshot's rendered order. */
function renderedNames(id: string): string[] {
  return (getSnapshot(id)?.entries ?? []).map((e) => e.name)
}

beforeEach(() => {
  vi.clearAllMocks()
  directorySortModeSpy.mockReturnValue('likeFiles')
  _resetForTesting()
})

describe('a snapshot opens in the engine ranked order', () => {
  it('carries no sort, so nothing claims a column', () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt'), makeEntry('a.txt')]))

    expect(getSnapshot('sr-1')?.sort).toBeNull()
    expect(renderedNames('sr-1')).toEqual(['c.txt', 'a.txt'])
  })
})

describe('sortSnapshot', () => {
  it('replaces the rendered rows with the order the backend comparator answered', async () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt'), makeEntry('a.txt'), makeEntry('b.txt')]))
    sortSearchResultsSpy.mockResolvedValue([1, 2, 0])

    await sortSnapshot('sr-1', { column: 'name', order: 'ascending' })

    expect(renderedNames('sr-1')).toEqual(['a.txt', 'b.txt', 'c.txt'])
    expect(getSnapshot('sr-1')?.sort).toEqual({ column: 'name', order: 'ascending' })
  })

  it('sends only the fields that decide order, and the user directory-sort mode', async () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt')]))
    sortSearchResultsSpy.mockResolvedValue([0])
    directorySortModeSpy.mockReturnValue('alwaysByName')

    await sortSnapshot('sr-1', { column: 'size', order: 'descending' })

    expect(sortSearchResultsSpy).toHaveBeenCalledWith(
      [{ name: 'c.txt', isDirectory: false, size: 100, modifiedAt: 1_700_000_000 }],
      'size',
      'descending',
      'alwaysByName',
    )
  })

  it('restores the engine ranked order exactly when the sort goes back to null', async () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt'), makeEntry('a.txt'), makeEntry('b.txt')]))
    sortSearchResultsSpy.mockResolvedValue([1, 2, 0])
    await sortSnapshot('sr-1', { column: 'name', order: 'ascending' })

    await sortSnapshot('sr-1', null)

    expect(renderedNames('sr-1')).toEqual(['c.txt', 'a.txt', 'b.txt'])
    expect(getSnapshot('sr-1')?.sort).toBeNull()
  })

  it('needs no backend round trip to go back to ranked order', async () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt')]))

    await sortSnapshot('sr-1', null)

    expect(sortSearchResultsSpy).not.toHaveBeenCalled()
  })

  it('leaves a snapshot that went away alone', async () => {
    await sortSnapshot('sr-gone', { column: 'name', order: 'ascending' })

    expect(getSnapshot('sr-gone')).toBeUndefined()
  })

  it('bumps the mutation tick so the pane re-renders its rows', async () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt'), makeEntry('a.txt')]))
    sortSearchResultsSpy.mockResolvedValue([1, 0])
    const before = getSnapshot('sr-1')

    await sortSnapshot('sr-1', { column: 'name', order: 'ascending' })

    // A fresh stored object, not a field write: a `$derived` recomputing to the
    // same reference tells the deriveds below it nothing changed.
    expect(getSnapshot('sr-1')).not.toBe(before)
  })
})

describe('rows arriving while a sort is on', () => {
  it('lands a still-running walk new rows in sorted position, never at the tail', async () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt'), makeEntry('a.txt')]))
    sortSearchResultsSpy.mockResolvedValue([1, 0])
    await sortSnapshot('sr-1', { column: 'name', order: 'ascending' })
    expect(renderedNames('sr-1')).toEqual(['a.txt', 'c.txt'])

    appendSnapshotEntries('sr-1', [makeEntry('b.txt')], 3)
    // Ranked order is now c, a, b; sorted by name that is a, b, c.
    sortSearchResultsSpy.mockResolvedValue([1, 2, 0])
    await resortSnapshotIfSorted('sr-1')

    expect(renderedNames('sr-1')).toEqual(['a.txt', 'b.txt', 'c.txt'])
  })

  it('appends to the ranked tail with no round trip while the pane is unsorted', async () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt')]))

    appendSnapshotEntries('sr-1', [makeEntry('a.txt')], 2)
    await resortSnapshotIfSorted('sr-1')

    expect(renderedNames('sr-1')).toEqual(['c.txt', 'a.txt'])
    expect(sortSearchResultsSpy).not.toHaveBeenCalled()
  })

  it('drops a re-sort whose rows changed under it, and orders the rows that exist now', async () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt'), makeEntry('a.txt')]))
    sortSearchResultsSpy.mockResolvedValue([1, 0])
    await sortSnapshot('sr-1', { column: 'name', order: 'ascending' })

    // The first round trip answers for two rows; a third lands before it
    // returns, so its answer can't be applied — it would leave a row unplaced.
    sortSearchResultsSpy.mockClear()
    let firstCall = true
    sortSearchResultsSpy.mockImplementation(() => {
      if (firstCall) {
        firstCall = false
        appendSnapshotEntries('sr-1', [makeEntry('b.txt')], 3)
        return Promise.resolve([1, 0])
      }
      return Promise.resolve([1, 2, 0])
    })

    await resortSnapshotIfSorted('sr-1')

    expect(renderedNames('sr-1')).toEqual(['a.txt', 'b.txt', 'c.txt'])
    expect(sortSearchResultsSpy).toHaveBeenCalledTimes(2)
  })
})

describe('a purge under a sort', () => {
  it('keeps the sorted order and drops the row from the ranked order too', async () => {
    getOrCreate('sr-1', makeSnapshot('sr-1', [makeEntry('c.txt'), makeEntry('a.txt'), makeEntry('b.txt')]))
    sortSearchResultsSpy.mockResolvedValue([1, 2, 0])
    await sortSnapshot('sr-1', { column: 'name', order: 'ascending' })

    removeEntryFromAllSnapshots('/Users/test/b.txt')

    expect(renderedNames('sr-1')).toEqual(['a.txt', 'c.txt'])
    // The ranked order lost it as well, so going back to ranked can't resurrect it.
    await sortSnapshot('sr-1', null)
    expect(renderedNames('sr-1')).toEqual(['c.txt', 'a.txt'])
  })
})

describe('nextSnapshotSort', () => {
  it('sorts by a fresh column in that column default order', () => {
    expect(nextSnapshotSort(null, 'name')).toEqual({ column: 'name', order: 'ascending' })
    expect(nextSnapshotSort(null, 'size')).toEqual({ column: 'size', order: 'descending' })
  })

  it('flips the direction on a second click of the same column', () => {
    expect(nextSnapshotSort({ column: 'name', order: 'ascending' }, 'name')).toEqual({
      column: 'name',
      order: 'descending',
    })
    expect(nextSnapshotSort({ column: 'size', order: 'descending' }, 'size')).toEqual({
      column: 'size',
      order: 'ascending',
    })
  })

  it('goes back to the ranked order on a third click', () => {
    expect(nextSnapshotSort({ column: 'name', order: 'descending' }, 'name')).toBeNull()
    expect(nextSnapshotSort({ column: 'size', order: 'ascending' }, 'size')).toBeNull()
  })

  it('starts a different column fresh rather than continuing the cycle', () => {
    expect(nextSnapshotSort({ column: 'name', order: 'descending' }, 'size')).toEqual({
      column: 'size',
      order: 'descending',
    })
  })
})
