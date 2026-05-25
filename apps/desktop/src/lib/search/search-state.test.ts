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

  it('returns 0 for zero (round 2 D10: the user can explicitly pick 0 from the preset grid)', () => {
    // Previously this returned `undefined` so a "0" selection silently became "any". The
    // round-2 list-style popover lets the user explicitly pick 0 bytes as the lower bound of
    // a between range, so honoring 0 is now correct.
    expect(parseSizeToBytes('0', 'KB')).toBe(0)
    expect(parseSizeToBytes('0', 'MB')).toBe(0)
    expect(parseSizeToBytes('0', 'GB')).toBe(0)
    expect(parseSizeToBytes('0', 'B')).toBe(0)
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

  it('honors size "0" as a literal 0-byte bound (round 2 D10)', () => {
    clearSearchState()
    setSizeFilter('between')
    setSizeValue('0')
    setSizeUnit('KB')
    setSizeValueMax('0')
    setSizeUnitMax('KB')
    const query = buildSearchQuery()
    expect(query.minSize).toBe(0)
    expect(query.maxSize).toBe(0)
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

describe('per-mode buffer', () => {
  // These tests import lazily so they don't add to the static import surface
  // at the top of the file (and so we don't have to repeat the long shared
  // list of helpers).
  it('switchMode swaps the bar between mode buffers', async () => {
    const { switchMode, setQueryFromUserInput } = await import('./search-state.svelte')
    clearSearchState()
    setMode('filename')
    setQueryFromUserInput('*.pdf')
    switchMode('regex')
    expect(getMode()).toBe('regex')
    expect(getQuery()).toBe('')

    setQueryFromUserInput('foo.*bar')
    switchMode('filename')
    expect(getMode()).toBe('filename')
    expect(getQuery()).toBe('*.pdf')
    switchMode('regex')
    expect(getQuery()).toBe('foo.*bar')
  })

  it('handing AI -> filename loads the AI pattern when its kind matches and the target buffer is empty', async () => {
    const { switchMode, recordAiTranslation } = await import('./search-state.svelte')
    clearSearchState()
    setMode('ai')
    setQuery('find my pdfs') // The natural-language prompt the user typed.
    recordAiTranslation({ pattern: '*.pdf', kind: 'glob', label: 'PDFs' })

    switchMode('filename')
    expect(getMode()).toBe('filename')
    expect(getQuery()).toBe('*.pdf') // Glob → filename input.

    switchMode('regex')
    expect(getQuery()).toBe('') // Regex's hand-typed buffer is still empty.
  })

  it('switching to AI restores the original prompt the user typed', async () => {
    const { switchMode, setQueryFromUserInput, recordAiTranslation } = await import('./search-state.svelte')
    clearSearchState()
    setMode('ai')
    setQueryFromUserInput('find my pdfs')
    recordAiTranslation({ pattern: '*.pdf', kind: 'glob', label: null })
    switchMode('filename')
    expect(getQuery()).toBe('*.pdf')
    switchMode('ai')
    expect(getQuery()).toBe('find my pdfs')
  })

  // R3 B2: when AI produces a glob, it overwrites the filename buffer; when it
  // produces a regex, it overwrites the regex buffer. Round 2 stored the AI
  // pattern in a separate slot and used `switchMode` to lazily hand it to the
  // matching mode ONLY when that mode's hand-typed buffer was empty. David hit
  // a case where he typed `*.foo` in filename mode, then asked the AI for PDFs,
  // then ⌘2'd to filename mode and saw `*.foo` instead of `*.pdf`. Fix: the AI
  // takeover is opinionated. Overwrite the matching buffer on translation.
  it('R3 B2: AI translation overwrites the matching hand-typed buffer (glob -> filename)', async () => {
    const { switchMode, setQueryFromUserInput, recordAiTranslation } = await import('./search-state.svelte')
    clearSearchState()
    // User typed *.foo in filename mode by hand, then jumped to AI to refine.
    setMode('filename')
    setQueryFromUserInput('*.foo')
    setMode('ai')
    setQueryFromUserInput('find my pdfs')
    // AI runs and produces a *.pdf glob. This should clobber the filename buffer.
    recordAiTranslation({ pattern: '*.pdf', kind: 'glob', label: 'PDFs' })
    switchMode('filename')
    expect(getQuery()).toBe('*.pdf')
  })

  it('R3 B2: AI translation overwrites the matching hand-typed buffer (regex -> regex)', async () => {
    const { switchMode, setQueryFromUserInput, recordAiTranslation } = await import('./search-state.svelte')
    clearSearchState()
    setMode('regex')
    setQueryFromUserInput('old.*pattern')
    setMode('ai')
    setQueryFromUserInput('find files matching new regex')
    recordAiTranslation({ pattern: 'new.*pattern', kind: 'regex', label: 'New regex' })
    switchMode('regex')
    expect(getQuery()).toBe('new.*pattern')
  })

  it('R3 B2: AI glob does NOT touch the regex buffer, and vice versa', async () => {
    const { switchMode, setQueryFromUserInput, recordAiTranslation } = await import('./search-state.svelte')
    clearSearchState()
    // Type in both modes by hand, then have AI produce a glob.
    setMode('filename')
    setQueryFromUserInput('*.foo')
    setMode('regex')
    setQueryFromUserInput('untouched.*')
    setMode('ai')
    setQueryFromUserInput('pdfs please')
    recordAiTranslation({ pattern: '*.pdf', kind: 'glob', label: 'PDFs' })
    // Filename gets overwritten, regex stays put.
    switchMode('filename')
    expect(getQuery()).toBe('*.pdf')
    switchMode('regex')
    expect(getQuery()).toBe('untouched.*')
  })

  it('clearAiPattern wipes the AI pattern slot but leaves the prompt intact', async () => {
    const { recordAiTranslation, clearAiPattern, getLastAiPattern, getLastAiPrompt, setLastAiPrompt } =
      await import('./search-state.svelte')
    clearSearchState()
    setLastAiPrompt('find my pdfs')
    recordAiTranslation({ pattern: '*.pdf', kind: 'glob', label: 'PDFs' })
    expect(getLastAiPattern()).toBe('*.pdf')
    clearAiPattern()
    expect(getLastAiPattern()).toBeNull()
    expect(getLastAiPrompt()).toBe('find my pdfs')
  })
})
