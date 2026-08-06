/**
 * How a saved search renders in the dropdown, and what picking or removing one does.
 *
 * The anchor worth having here: **picking LOADS, it never runs.** An AI entry that
 * re-translated on pick would re-bill the user for a keystroke.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { HistoryEntry } from '$lib/ipc/bindings'

const { removeRecentSearchMock, getRecentSearchesMock } = vi.hoisted(() => ({
  removeRecentSearchMock: vi.fn(() => Promise.resolve()),
  getRecentSearchesMock: vi.fn(() => Promise.resolve([] as HistoryEntry[])),
}))

vi.mock('$lib/tauri-commands', () => ({
  removeRecentSearch: removeRecentSearchMock,
  getRecentSearches: getRecentSearchesMock,
}))

import {
  activateHistoryEntry,
  removeHistoryEntry,
  searchRecentAdapter,
  searchRecentKey,
} from './recent-search-adapter'
import { getRecentSearchesList, resetRecentSearchesForTests, setRecentSearchesList } from './recent-searches-state.svelte'
import { clearSearchState, getMode, getQuery, getScope, searchQueryState } from './search-state.svelte'

function entry(overrides: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    id: 'h1',
    timestamp: Date.now(),
    mode: 'filename',
    query: '*.pdf',
    filters: { sizeMin: null, sizeMax: null, modifiedAfter: null, modifiedBefore: null },
    scope: '~/docs',
    caseSensitive: false,
    excludeSystemDirs: true,
    resultCount: 3,
    ...overrides,
  }
}

beforeEach(() => {
  clearSearchState()
  resetRecentSearchesForTests()
  removeRecentSearchMock.mockClear()
  getRecentSearchesMock.mockClear()
})

describe('searchRecentAdapter', () => {
  it('renders the entry as a row the shared dropdown can draw', () => {
    const view = searchRecentAdapter(entry({ query: 'report*' }))
    expect(view.label).toBe('report*')
    expect(view.mode).toBe('filename')
    expect(view.ariaLabel).toContain('report*')
    expect(view.ageLabel).not.toBe('')
    expect(searchRecentKey(entry({ id: 'h9' }))).toBe('h9')
  })
})

describe('activateHistoryEntry', () => {
  it('loads the saved query, mode, and scope, and does NOT arm a run', () => {
    activateHistoryEntry(entry({ query: 'invoices', mode: 'regex', scope: '~/docs' }))
    expect(getQuery()).toBe('invoices')
    expect(getMode()).toBe('regex')
    expect(getScope()).toBe('~/docs')
    // Picking is navigation: the user lands in the field with the search ready to tweak.
    expect(searchQueryState.getRunOnMount()).toBe(false)
  })
})

describe('removeHistoryEntry', () => {
  it('drops the row immediately, then reconciles with what the backend reports', async () => {
    setRecentSearchesList([entry({ id: 'a' }), entry({ id: 'b' })])
    getRecentSearchesMock.mockResolvedValueOnce([entry({ id: 'b' })])

    removeHistoryEntry(entry({ id: 'a' }))

    // Optimistic: gone from the list before the IPC answers.
    expect(getRecentSearchesList().map((e) => e.id)).toEqual(['b'])
    expect(removeRecentSearchMock).toHaveBeenCalledWith('a')

    await vi.waitFor(() => {
      expect(getRecentSearchesMock).toHaveBeenCalled()
    })
    expect(getRecentSearchesList().map((e) => e.id)).toEqual(['b'])
  })

  it('keeps the optimistic list when the re-read fails', async () => {
    setRecentSearchesList([entry({ id: 'a' }), entry({ id: 'b' })])
    getRecentSearchesMock.mockRejectedValueOnce(new Error('offline'))

    removeHistoryEntry(entry({ id: 'a' }))
    await vi.waitFor(() => {
      expect(getRecentSearchesMock).toHaveBeenCalled()
    })
    expect(getRecentSearchesList().map((e) => e.id)).toEqual(['b'])
  })
})
