import { describe, it, expect, vi } from 'vitest'
import type { SelectionHistoryEntry } from '$lib/tauri-commands'

// Module under test imports `$lib/tauri-commands`; stub `getRecentSelections`
// so the factory's IPC dereference at import time doesn't throw.
vi.mock('$lib/tauri-commands', () => ({
  getRecentSelections: vi.fn(() => Promise.resolve([])),
}))

import { applySelectionHistoryEntry } from './selection-history-state.svelte'
import { createQueryFilterState } from '$lib/query-ui/query-filter-state.svelte'

function entry(over: Partial<SelectionHistoryEntry> = {}): SelectionHistoryEntry {
  return {
    id: 'e1',
    timestamp: 0,
    mode: 'filename',
    query: '*.png',
    filters: { sizeMin: 1024 * 1024, sizeMax: null, modifiedAfter: null, modifiedBefore: null },
    caseSensitive: false,
    matchCount: 12,
    ...over,
  }
}

describe('applySelectionHistoryEntry', () => {
  it('round-trips query, mode, caseSensitive, and size filter into state', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    applySelectionHistoryEntry(state, entry({ mode: 'regex', query: '^foo.*', caseSensitive: true }))
    expect(state.getMode()).toBe('regex')
    expect(state.getQuery()).toBe('^foo.*')
    expect(state.getCaseSensitive()).toBe(true)
    expect(state.getSizeFilter()).toBe('gte')
    // 1 MB → "1" MB through bytesToSize.
    expect(state.getSizeValue()).toBe('1')
    expect(state.getSizeUnit()).toBe('MB')
  })

  it('clears the date filter when the entry has no date', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    // Pre-populate with a date filter to verify it gets cleared.
    state.setDateFilter('after')
    state.setDateValue('2026-01-01')
    applySelectionHistoryEntry(state, entry({ filters: {} }))
    expect(state.getDateFilter()).toBe('any')
    expect(state.getDateValue()).toBe('')
  })

  it('hands the typed query into the matching hand-typed buffer', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    applySelectionHistoryEntry(state, entry({ mode: 'filename', query: '*.log' }))
    expect(state.getHandTypedBuffer('filename')).toBe('*.log')
    expect(state.getHandTypedBuffer('regex')).toBe('')
  })

  it('persists the AI prompt as `query` so it survives the apply', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    applySelectionHistoryEntry(state, entry({ mode: 'ai', query: 'all the image files', filters: {} }))
    expect(state.getMode()).toBe('ai')
    expect(state.getQuery()).toBe('all the image files')
    expect(state.getHandTypedBuffer('ai')).toBe('all the image files')
  })
})
