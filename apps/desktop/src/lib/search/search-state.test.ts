import { describe, it, expect } from 'vitest'
import {
  parseSizeToBytes,
  parseDateToTimestamp,
  buildSearchQuery,
  buildHistoryFilters,
  applyHistoryEntry,
  clearSearchState,
  setQuery,
  setMode,
  getMode,
  getQuery,
  setSizeFilter,
  setSizeValue,
  setSizeUnit,
  setDateFilter,
  setDateValue,
  setSizeValueMax,
  setSizeUnitMax,
  setDateValueMax,
  getScope,
  setScope,
  getSizeFilter,
  getSizeValue,
  getSizeUnit,
  getDateFilter,
  getDateValue,
  getCaseSensitive,
} from './search-state.svelte'
import type { HistoryEntry } from '$lib/tauri-commands'

describe('parseSizeToBytes', () => {
  it('converts KB to bytes', () => {
    expect(parseSizeToBytes('1', 'KB')).toBe(1024)
  })

  it('converts MB to bytes', () => {
    expect(parseSizeToBytes('1', 'MB')).toBe(1024 * 1024)
  })

  it('converts GB to bytes', () => {
    expect(parseSizeToBytes('1', 'GB')).toBe(1024 * 1024 * 1024)
  })

  it('handles decimal values', () => {
    expect(parseSizeToBytes('1.5', 'MB')).toBe(Math.round(1.5 * 1024 * 1024))
  })

  it('returns undefined for empty string', () => {
    expect(parseSizeToBytes('', 'MB')).toBeUndefined()
  })

  it('returns undefined for non-numeric input', () => {
    expect(parseSizeToBytes('abc', 'MB')).toBeUndefined()
  })

  it('returns undefined for negative values', () => {
    expect(parseSizeToBytes('-5', 'MB')).toBeUndefined()
  })

  it('returns undefined for zero (not a useful filter)', () => {
    expect(parseSizeToBytes('0', 'KB')).toBeUndefined()
    expect(parseSizeToBytes('0', 'MB')).toBeUndefined()
    expect(parseSizeToBytes('0', 'GB')).toBeUndefined()
  })
})

describe('parseDateToTimestamp', () => {
  it('converts ISO date string to unix timestamp', () => {
    const ts = parseDateToTimestamp('2025-01-01')
    expect(ts).toBeTypeOf('number')
    expect(ts).toBeGreaterThan(0)
  })

  it('returns undefined for empty string', () => {
    expect(parseDateToTimestamp('')).toBeUndefined()
  })

  it('returns undefined for invalid date', () => {
    expect(parseDateToTimestamp('not-a-date')).toBeUndefined()
  })
})

describe('buildSearchQuery', () => {
  it('builds default query with no filters', () => {
    clearSearchState()
    const query = buildSearchQuery()
    expect(query.patternType).toBe('glob')
    expect(query.limit).toBe(30)
    expect(query.namePattern).toBeNull()
    expect(query.minSize).toBeNull()
    expect(query.maxSize).toBeNull()
    expect(query.modifiedAfter).toBeNull()
    expect(query.modifiedBefore).toBeNull()
  })

  it('includes the query text as namePattern when set', () => {
    clearSearchState()
    setQuery('*.pdf')
    const query = buildSearchQuery()
    expect(query.namePattern).toBe('*.pdf')
  })

  it('includes size gte filter', () => {
    clearSearchState()
    setSizeFilter('gte')
    setSizeValue('10')
    setSizeUnit('MB')
    const query = buildSearchQuery()
    expect(query.minSize).toBe(10 * 1024 * 1024)
    expect(query.maxSize).toBeNull()
  })

  it('includes size lte filter', () => {
    clearSearchState()
    setSizeFilter('lte')
    setSizeValue('5')
    setSizeUnit('KB')
    const query = buildSearchQuery()
    expect(query.maxSize).toBe(5 * 1024)
    expect(query.minSize).toBeNull()
  })

  it('includes size between filter', () => {
    clearSearchState()
    setSizeFilter('between')
    setSizeValue('1')
    setSizeUnit('MB')
    setSizeValueMax('10')
    setSizeUnitMax('MB')
    const query = buildSearchQuery()
    expect(query.minSize).toBe(1024 * 1024)
    expect(query.maxSize).toBe(10 * 1024 * 1024)
  })

  it('treats size "0" as no filter', () => {
    clearSearchState()
    setSizeFilter('between')
    setSizeValue('0')
    setSizeUnit('KB')
    setSizeValueMax('0')
    setSizeUnitMax('KB')
    const query = buildSearchQuery()
    expect(query.minSize).toBeNull()
    expect(query.maxSize).toBeNull()
  })

  it('includes date after filter', () => {
    clearSearchState()
    setDateFilter('after')
    setDateValue('2025-01-01')
    const query = buildSearchQuery()
    expect(query.modifiedAfter).toBeTypeOf('number')
    expect(query.modifiedBefore).toBeNull()
  })

  it('includes date between filter', () => {
    clearSearchState()
    setDateFilter('between')
    setDateValue('2025-01-01')
    setDateValueMax('2025-12-31')
    const query = buildSearchQuery()
    expect(query.modifiedAfter).toBeTypeOf('number')
    expect(query.modifiedBefore).toBeTypeOf('number')
  })
})

describe('clearSearchState', () => {
  it('clears all state', () => {
    setQuery('test')
    setSizeFilter('gte')
    setDateFilter('after')
    clearSearchState()
    const query = buildSearchQuery()
    expect(query.namePattern).toBeNull()
    expect(query.minSize).toBeNull()
    expect(query.modifiedAfter).toBeNull()
  })

  it('uses regex patternType when mode is regex', () => {
    clearSearchState()
    setMode('regex')
    const query = buildSearchQuery()
    expect(query.patternType).toBe('regex')
  })

  it('resets mode to filename on clearSearchState', () => {
    setMode('regex')
    clearSearchState()
    expect(getMode()).toBe('filename')
    const query = buildSearchQuery()
    expect(query.patternType).toBe('glob')
  })

  it('clears the query field', () => {
    setQuery('something')
    clearSearchState()
    const query = buildSearchQuery()
    expect(query.namePattern).toBeNull()
  })
})

describe('buildHistoryFilters', () => {
  it('returns an empty object when no filters are set', () => {
    clearSearchState()
    expect(buildHistoryFilters()).toEqual({})
  })

  it('includes sizeMin only for gte', () => {
    clearSearchState()
    setSizeFilter('gte')
    setSizeValue('1')
    setSizeUnit('MB')
    expect(buildHistoryFilters()).toEqual({ sizeMin: 1024 * 1024 })
  })

  it('includes sizeMax only for lte', () => {
    clearSearchState()
    setSizeFilter('lte')
    setSizeValue('2')
    setSizeUnit('MB')
    expect(buildHistoryFilters()).toEqual({ sizeMax: 2 * 1024 * 1024 })
  })

  it('includes both bounds for between', () => {
    clearSearchState()
    setSizeFilter('between')
    setSizeValue('1')
    setSizeUnit('MB')
    setSizeValueMax('5')
    setSizeUnitMax('MB')
    expect(buildHistoryFilters()).toEqual({
      sizeMin: 1024 * 1024,
      sizeMax: 5 * 1024 * 1024,
    })
  })

  it('includes modifiedAfter for the after date filter', () => {
    clearSearchState()
    setDateFilter('after')
    setDateValue('2026-01-01')
    expect(buildHistoryFilters()).toEqual({ modifiedAfter: '2026-01-01' })
  })
})

describe('applyHistoryEntry', () => {
  const baseEntry: HistoryEntry = {
    id: 'abc',
    timestamp: Date.UTC(2026, 4, 20),
    mode: 'filename',
    query: '*.pdf',
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    resultCount: 0,
  }

  it('restores the simple fields (query, mode, scope, flags)', () => {
    clearSearchState()
    applyHistoryEntry({
      ...baseEntry,
      mode: 'regex',
      query: 'foo.*',
      scope: '~/Documents, !node_modules',
      caseSensitive: true,
    })
    expect(getQuery()).toBe('foo.*')
    expect(getMode()).toBe('regex')
    expect(getScope()).toBe('~/Documents, !node_modules')
    expect(getCaseSensitive()).toBe(true)
  })

  it('restores size filters with the friendliest unit', () => {
    clearSearchState()
    applyHistoryEntry({
      ...baseEntry,
      filters: { sizeMin: 5 * 1024 * 1024 },
    })
    expect(getSizeFilter()).toBe('gte')
    expect(getSizeValue()).toBe('5')
    expect(getSizeUnit()).toBe('MB')
  })

  it('restores both size bounds for "between"', () => {
    clearSearchState()
    applyHistoryEntry({
      ...baseEntry,
      filters: { sizeMin: 1024, sizeMax: 10 * 1024 * 1024 },
    })
    expect(getSizeFilter()).toBe('between')
  })

  it('restores date filters when present', () => {
    clearSearchState()
    applyHistoryEntry({
      ...baseEntry,
      filters: { modifiedAfter: '2026-01-01' },
    })
    expect(getDateFilter()).toBe('after')
    expect(getDateValue()).toBe('2026-01-01')
  })

  it('clears any leftover filters on the way in', () => {
    clearSearchState()
    setSizeFilter('gte')
    setSizeValue('1')
    setSizeUnit('GB')
    applyHistoryEntry({ ...baseEntry, filters: {} })
    expect(getSizeFilter()).toBe('any')
  })

  it('round-trips through buildHistoryFilters', () => {
    clearSearchState()
    setSizeFilter('gte')
    setSizeValue('1')
    setSizeUnit('MB')
    setDateFilter('after')
    setDateValue('2026-01-01')
    const filters = buildHistoryFilters()

    clearSearchState()
    applyHistoryEntry({ ...baseEntry, filters })

    expect(buildHistoryFilters()).toEqual(filters)
  })
})

describe('scope state', () => {
  it('defaults to empty string', () => {
    clearSearchState()
    expect(getScope()).toBe('')
  })

  it('stores and retrieves scope', () => {
    clearSearchState()
    setScope('~/projects, !node_modules')
    expect(getScope()).toBe('~/projects, !node_modules')
  })

  it('resets scope on clearSearchState', () => {
    setScope('~/Documents')
    clearSearchState()
    expect(getScope()).toBe('')
  })
})
