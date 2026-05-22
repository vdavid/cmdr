import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { HistoryEntry } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', async (orig) => {
  const actual = (await orig()) as Record<string, unknown>
  return {
    ...actual,
    getRecentSearches: vi.fn(),
  }
})

import {
  getRecentSearchesList,
  setRecentSearchesList,
  loadRecentSearches,
  getRecentSearchesLoaded,
  resetRecentSearchesForTests,
} from './recent-searches-state.svelte'
import { getRecentSearches } from '$lib/tauri-commands'

const mockGetRecent = vi.mocked(getRecentSearches)

function entry(id: string): HistoryEntry {
  return {
    id,
    timestamp: 1,
    mode: 'filename',
    query: id,
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    resultCount: 0,
  }
}

describe('recent-searches-state', () => {
  beforeEach(() => {
    resetRecentSearchesForTests()
    mockGetRecent.mockReset()
  })

  it('starts empty', () => {
    expect(getRecentSearchesList()).toEqual([])
  })

  it('setRecentSearchesList replaces the in-memory list and marks loaded', () => {
    setRecentSearchesList([entry('a'), entry('b')])
    expect(getRecentSearchesList().map((e) => e.id)).toEqual(['a', 'b'])
    expect(getRecentSearchesLoaded()).toBe(true)
  })

  it('loadRecentSearches calls the backend on first invocation', async () => {
    expect(getRecentSearchesLoaded()).toBe(false)
    mockGetRecent.mockResolvedValueOnce([entry('one')])
    await loadRecentSearches()
    expect(mockGetRecent).toHaveBeenCalledTimes(1)
    expect(getRecentSearchesList().map((e) => e.id)).toEqual(['one'])
  })

  it('loadRecentSearches is idempotent without force', async () => {
    // First call seeds + marks loaded.
    mockGetRecent.mockResolvedValueOnce([entry('one')])
    await loadRecentSearches()
    mockGetRecent.mockClear()

    // Subsequent call should be a no-op.
    await loadRecentSearches()
    expect(mockGetRecent).not.toHaveBeenCalled()
  })

  it('loadRecentSearches refetches when force=true', async () => {
    mockGetRecent.mockResolvedValueOnce([entry('one')])
    await loadRecentSearches()
    mockGetRecent.mockResolvedValueOnce([entry('two'), entry('three')])
    await loadRecentSearches(true)
    expect(getRecentSearchesList().map((e) => e.id)).toEqual(['two', 'three'])
  })

  it('loadRecentSearches survives a failing backend call', async () => {
    setRecentSearchesList([entry('keep')])
    // Force a reload that fails; the in-memory list should stay intact.
    mockGetRecent.mockRejectedValueOnce(new Error('boom'))
    await loadRecentSearches(true)
    expect(getRecentSearchesList().map((e) => e.id)).toEqual(['keep'])
  })
})
