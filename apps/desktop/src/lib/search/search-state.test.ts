import { describe, it, expect } from 'vitest'
import {
  parseSizeToBytes,
  parseDateToTimestamp,
  buildSearchQuery,
  clearSearchState,
  setNamePattern,
  setSizeFilter,
  setSizeValue,
  setSizeUnit,
  setDateFilter,
  setDateValue,
  setSizeValueMax,
  setSizeUnitMax,
  setDateValueMax,
  setPatternType,
  getScope,
  setScope,
} from './search-state.svelte'

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

  it('includes name pattern when set', () => {
    clearSearchState()
    setNamePattern('*.pdf')
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
    setNamePattern('test')
    setSizeFilter('gte')
    setDateFilter('after')
    clearSearchState()
    const query = buildSearchQuery()
    expect(query.namePattern).toBeNull()
    expect(query.minSize).toBeNull()
    expect(query.modifiedAfter).toBeNull()
  })

  it('uses regex pattern type when set', () => {
    clearSearchState()
    setPatternType('regex')
    const query = buildSearchQuery()
    expect(query.patternType).toBe('regex')
  })

  it('resets pattern type to glob on clearSearchState', () => {
    setPatternType('regex')
    clearSearchState()
    const query = buildSearchQuery()
    expect(query.patternType).toBe('glob')
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
