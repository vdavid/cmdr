import { describe, it, expect, beforeEach } from 'vitest'
import type { SearchResultEntry } from '$lib/ipc/bindings'
import {
  _resetForTesting,
  decrementRef,
  getDebugStats,
  getLastAttemptId,
  getOrCreate,
  getRefCount,
  getSnapshot,
  incrementRef,
  nextSnapshotId,
  setLastAttemptId,
  SNAPSHOT_ENTRIES_CAP,
  type SearchSnapshot,
} from './snapshot-store.svelte'

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

function makeSnapshot(id: string, overrides: Partial<SearchSnapshot> = {}): SearchSnapshot {
  return {
    id,
    query: 'foo',
    mode: 'filename',
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    entries: [makeEntry('a.txt'), makeEntry('b.txt')],
    totalCount: 2,
    createdAt: 1_700_000_000_000,
    label: 'Search: foo',
    ...overrides,
  }
}

describe('snapshot-store', () => {
  beforeEach(() => {
    _resetForTesting()
  })

  describe('nextSnapshotId', () => {
    it('returns monotonically increasing sr-N ids', () => {
      expect(nextSnapshotId()).toBe('sr-1')
      expect(nextSnapshotId()).toBe('sr-2')
      expect(nextSnapshotId()).toBe('sr-3')
    })
  })

  describe('getOrCreate', () => {
    it('stores a new snapshot with refCount 0 and returns it', () => {
      const snap = makeSnapshot('sr-1')
      const result = getOrCreate('sr-1', snap)
      expect(result.id).toBe('sr-1')
      expect(result.entries).toHaveLength(2)
      expect(getRefCount('sr-1')).toBe(0)
    })

    it('returns the existing snapshot without overwriting on duplicate id', () => {
      const snap1 = makeSnapshot('sr-1', { query: 'first' })
      getOrCreate('sr-1', snap1)
      incrementRef('sr-1')

      const snap2 = makeSnapshot('sr-1', { query: 'second', entries: [makeEntry('z.txt')] })
      const result = getOrCreate('sr-1', snap2)

      expect(result.query).toBe('first')
      expect(result.entries).toHaveLength(2)
      // Refcount survives the duplicate call too.
      expect(getRefCount('sr-1')).toBe(1)
    })

    it('truncates entries beyond the cap and annotates the label', () => {
      const bigEntries: SearchResultEntry[] = []
      for (let i = 0; i < SNAPSHOT_ENTRIES_CAP + 47; i++) {
        bigEntries.push(makeEntry(`f${String(i)}.txt`))
      }
      const totalCount = bigEntries.length
      const snap = makeSnapshot('sr-big', {
        entries: bigEntries,
        totalCount,
        label: 'Search: *.txt',
      })
      const stored = getOrCreate('sr-big', snap)
      expect(stored.entries).toHaveLength(SNAPSHOT_ENTRIES_CAP)
      expect(stored.label).toBe(`Search: *.txt (first ${String(SNAPSHOT_ENTRIES_CAP)} of ${String(totalCount)})`)
      // Caller's array is untouched (no mutation of the input).
      expect(bigEntries).toHaveLength(SNAPSHOT_ENTRIES_CAP + 47)
    })

    it('does not annotate the label when entries fit under the cap', () => {
      const snap = makeSnapshot('sr-small', { label: 'Search: foo' })
      const stored = getOrCreate('sr-small', snap)
      expect(stored.label).toBe('Search: foo')
    })
  })

  describe('getSnapshot', () => {
    it('returns the stored snapshot by id', () => {
      const snap = makeSnapshot('sr-1', { query: 'hello' })
      getOrCreate('sr-1', snap)
      expect(getSnapshot('sr-1')?.query).toBe('hello')
    })

    it('returns undefined for an unknown id', () => {
      expect(getSnapshot('sr-missing')).toBeUndefined()
    })
  })

  describe('incrementRef / decrementRef', () => {
    it('increments the refcount', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1'))
      incrementRef('sr-1')
      incrementRef('sr-1')
      expect(getRefCount('sr-1')).toBe(2)
    })

    it('decrements the refcount', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1'))
      incrementRef('sr-1')
      incrementRef('sr-1')
      decrementRef('sr-1')
      expect(getRefCount('sr-1')).toBe(1)
      // Snapshot is still in the store.
      expect(getSnapshot('sr-1')).toBeDefined()
    })

    it('deletes the snapshot when refcount drops to 0', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1'))
      incrementRef('sr-1')
      decrementRef('sr-1')
      expect(getRefCount('sr-1')).toBe(0)
      expect(getSnapshot('sr-1')).toBeUndefined()
    })

    it('does not delete a snapshot while refcount is still > 0', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1'))
      incrementRef('sr-1')
      incrementRef('sr-1')
      decrementRef('sr-1')
      // Still 1 ref left.
      expect(getRefCount('sr-1')).toBe(1)
      expect(getSnapshot('sr-1')).toBeDefined()
    })

    it('is a safe no-op when decrementing an unknown id', () => {
      expect(() => {
        decrementRef('sr-missing')
      }).not.toThrow()
      expect(getRefCount('sr-missing')).toBe(0)
    })

    it('does not go negative on excess decrements', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1'))
      incrementRef('sr-1')
      decrementRef('sr-1') // deletes
      decrementRef('sr-1') // no-op (already evicted)
      expect(getRefCount('sr-1')).toBe(0)
    })
  })

  describe('setLastAttemptId', () => {
    it('increments the new id and is a no-op when called with the same id', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1'))
      setLastAttemptId('sr-1')
      expect(getRefCount('sr-1')).toBe(1)
      expect(getLastAttemptId()).toBe('sr-1')

      // Re-setting the same id should not double-count.
      setLastAttemptId('sr-1')
      expect(getRefCount('sr-1')).toBe(1)
    })

    it('swaps refs: decrements the old id and increments the new one', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1'))
      getOrCreate('sr-2', makeSnapshot('sr-2'))

      setLastAttemptId('sr-1')
      expect(getRefCount('sr-1')).toBe(1)

      setLastAttemptId('sr-2')
      // sr-1 was holding only this slot, so it evicts.
      expect(getRefCount('sr-1')).toBe(0)
      expect(getSnapshot('sr-1')).toBeUndefined()
      expect(getRefCount('sr-2')).toBe(1)
      expect(getLastAttemptId()).toBe('sr-2')
    })

    it('does not evict the old id if other refs are still alive', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1'))
      getOrCreate('sr-2', makeSnapshot('sr-2'))
      incrementRef('sr-1') // simulate a pane history reference

      setLastAttemptId('sr-1')
      expect(getRefCount('sr-1')).toBe(2)

      setLastAttemptId('sr-2')
      // sr-1 still has the pane reference, so it stays alive.
      expect(getRefCount('sr-1')).toBe(1)
      expect(getSnapshot('sr-1')).toBeDefined()
    })

    it('setLastAttemptId(null) releases the current slot', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1'))
      setLastAttemptId('sr-1')
      expect(getRefCount('sr-1')).toBe(1)

      setLastAttemptId(null)
      expect(getRefCount('sr-1')).toBe(0)
      expect(getSnapshot('sr-1')).toBeUndefined()
      expect(getLastAttemptId()).toBeNull()
    })

    it('setLastAttemptId(null) twice is idempotent', () => {
      setLastAttemptId(null)
      setLastAttemptId(null)
      expect(getLastAttemptId()).toBeNull()
    })
  })

  describe('getDebugStats', () => {
    it('returns a sane shape with no snapshots', () => {
      const stats = getDebugStats()
      expect(stats).toEqual({ count: 0, totalEntries: 0, maxRefCount: 0, idsWithRefCount: [] })
    })

    it('aggregates counts and tracks the max refcount across all snapshots', () => {
      getOrCreate('sr-1', makeSnapshot('sr-1', { entries: [makeEntry('a.txt')] }))
      getOrCreate('sr-2', makeSnapshot('sr-2', { entries: [makeEntry('a.txt'), makeEntry('b.txt')] }))
      incrementRef('sr-1')
      incrementRef('sr-2')
      incrementRef('sr-2')
      incrementRef('sr-2')

      const stats = getDebugStats()
      expect(stats.count).toBe(2)
      expect(stats.totalEntries).toBe(3)
      expect(stats.maxRefCount).toBe(3)
      const map = new Map(stats.idsWithRefCount)
      expect(map.get('sr-1')).toBe(1)
      expect(map.get('sr-2')).toBe(3)
    })
  })
})
