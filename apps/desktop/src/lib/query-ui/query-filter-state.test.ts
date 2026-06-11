/**
 * Factory-instance counterpart to `lib/search/search-state.test.ts`. Pins the
 * cross-consumer behaviors against an instance returned by `createQueryFilterState()`.
 * Search's existing tests pass against the façade in `lib/search/search-state.svelte.ts`
 * which re-exports the named API on top of this factory.
 */

import { describe, it, expect } from 'vitest'
import {
  createQueryFilterState,
  parseSizeToBytes,
  parseDateToTimestamp,
  bytesToSize,
  deriveEnterAction,
  type LastDialogEvent,
} from './query-filter-state.svelte'

describe('parseSizeToBytes (pure helper)', () => {
  it('converts KB / MB / GB to bytes', () => {
    expect(parseSizeToBytes('1', 'KB')).toBe(1024)
    expect(parseSizeToBytes('1', 'MB')).toBe(1024 * 1024)
    expect(parseSizeToBytes('1', 'GB')).toBe(1024 * 1024 * 1024)
  })

  it('honors 0 as a literal lower / upper bound', () => {
    expect(parseSizeToBytes('0', 'KB')).toBe(0)
    expect(parseSizeToBytes('0', 'B')).toBe(0)
  })

  it('returns undefined for empty / non-numeric / negative values', () => {
    expect(parseSizeToBytes('', 'MB')).toBeUndefined()
    expect(parseSizeToBytes('abc', 'MB')).toBeUndefined()
    expect(parseSizeToBytes('-5', 'MB')).toBeUndefined()
  })
})

describe('parseDateToTimestamp (pure helper)', () => {
  it('converts ISO dates to unix seconds', () => {
    const ts = parseDateToTimestamp('2025-01-01')
    expect(ts).toBeTypeOf('number')
    expect(ts).toBeGreaterThan(0)
  })

  it('returns undefined for empty / invalid input', () => {
    expect(parseDateToTimestamp('')).toBeUndefined()
    expect(parseDateToTimestamp('not-a-date')).toBeUndefined()
  })
})

describe('bytesToSize (pure helper)', () => {
  it('picks the friendliest unit by magnitude', () => {
    expect(bytesToSize(1024)).toEqual({ value: '1', unit: 'KB' })
    expect(bytesToSize(5 * 1024 * 1024)).toEqual({ value: '5', unit: 'MB' })
    expect(bytesToSize(2 * 1024 * 1024 * 1024)).toEqual({ value: '2', unit: 'GB' })
  })
})

describe('deriveEnterAction (re-exported from enter-action.ts)', () => {
  const events: LastDialogEvent[] = ['opened', 'results-arrived', 'cursor-moved', 'query-edited', 'filter-edited']

  it('returns run-search when there are no results, regardless of last event', () => {
    for (const lastEvent of events) {
      expect(deriveEnterAction({ lastEvent, resultsCount: 0 })).toBe('run-search')
    }
  })

  it('returns go-to-file on results-arrived / cursor-moved with results', () => {
    expect(deriveEnterAction({ lastEvent: 'results-arrived', resultsCount: 3 })).toBe('go-to-file')
    expect(deriveEnterAction({ lastEvent: 'cursor-moved', resultsCount: 3 })).toBe('go-to-file')
  })
})

describe('createQueryFilterState: defaults', () => {
  it('starts on filename mode with empty query and any-filters', () => {
    const s = createQueryFilterState()
    expect(s.getMode()).toBe('filename')
    expect(s.getQuery()).toBe('')
    expect(s.getSizeFilter()).toBe('any')
    expect(s.getDateFilter()).toBe('any')
    expect(s.getCaseSensitive()).toBe(false)
    expect(s.getResults()).toEqual([])
    expect(s.getCursorIndex()).toBe(0)
  })

  it('honors a custom default mode', () => {
    const s = createQueryFilterState({ defaultMode: 'regex' })
    expect(s.getMode()).toBe('regex')
    s.clearCore()
    expect(s.getMode()).toBe('regex')
  })
})

describe('createQueryFilterState: buildBaseSearchQuery', () => {
  it('builds a glob query by default', () => {
    const s = createQueryFilterState()
    const q = s.buildBaseSearchQuery()
    expect(q.patternType).toBe('glob')
    expect(q.namePattern).toBeNull()
    expect(q.limit).toBe(30)
    expect(q.minSize).toBeNull()
    expect(q.maxSize).toBeNull()
  })

  it('reads namePattern from the query field', () => {
    const s = createQueryFilterState()
    s.setQuery('*.pdf')
    expect(s.buildBaseSearchQuery().namePattern).toBe('*.pdf')
  })

  it('produces a regex query when mode is regex', () => {
    const s = createQueryFilterState()
    s.setMode('regex')
    expect(s.buildBaseSearchQuery().patternType).toBe('regex')
  })

  it('emits caseSensitive=true only when set (else leaves it undefined)', () => {
    const s = createQueryFilterState()
    expect(s.buildBaseSearchQuery().caseSensitive).toBeUndefined()
    s.setCaseSensitive(true)
    expect(s.buildBaseSearchQuery().caseSensitive).toBe(true)
  })

  it('layers size and date predicates onto the query', () => {
    const s = createQueryFilterState()
    s.setSizeFilter('gte')
    s.setSizeValue('10')
    s.setSizeUnit('MB')
    s.setDateFilter('after')
    s.setDateValue('2026-01-01')
    const q = s.buildBaseSearchQuery()
    expect(q.minSize).toBe(10 * 1024 * 1024)
    expect(q.modifiedAfter).toBeTypeOf('number')
  })
})

describe('createQueryFilterState: switchMode + per-mode buffers', () => {
  it('swaps the bar between mode buffers via switchMode', () => {
    const s = createQueryFilterState()
    s.setMode('filename')
    s.setQueryFromUserInput('*.pdf')
    s.switchMode('regex')
    expect(s.getMode()).toBe('regex')
    expect(s.getQuery()).toBe('')

    s.setQueryFromUserInput('foo.*bar')
    s.switchMode('filename')
    expect(s.getQuery()).toBe('*.pdf')
    s.switchMode('regex')
    expect(s.getQuery()).toBe('foo.*bar')
  })

  it('restores the original AI prompt when swapping back to AI', () => {
    const s = createQueryFilterState()
    s.setMode('ai')
    s.setQueryFromUserInput('find my pdfs')
    s.recordAiTranslation({ pattern: '*.pdf', kind: 'glob' })
    s.switchMode('filename')
    expect(s.getQuery()).toBe('*.pdf') // AI's glob fills the empty filename buffer.
    s.switchMode('ai')
    expect(s.getQuery()).toBe('find my pdfs')
  })

  it('AI translation overwrites the matching hand-typed buffer (glob to filename)', () => {
    const s = createQueryFilterState()
    s.setMode('filename')
    s.setQueryFromUserInput('*.foo')
    s.setMode('ai')
    s.setQueryFromUserInput('find my pdfs')
    s.recordAiTranslation({ pattern: '*.pdf', kind: 'glob' })
    s.switchMode('filename')
    expect(s.getQuery()).toBe('*.pdf')
  })

  it('AI translation of a regex overwrites the regex buffer, not filename', () => {
    const s = createQueryFilterState()
    s.setMode('filename')
    s.setQueryFromUserInput('*.foo') // hand-typed glob
    s.setMode('regex')
    s.setQueryFromUserInput('untouched.*')
    s.setMode('ai')
    s.setQueryFromUserInput('AI prompt')
    s.recordAiTranslation({ pattern: '*.png', kind: 'glob' })
    // glob lands in filename; regex stays put
    s.switchMode('filename')
    expect(s.getQuery()).toBe('*.png')
    s.switchMode('regex')
    expect(s.getQuery()).toBe('untouched.*')
  })

  it('switchMode uses the injected aiPatternProbe to seed an empty target buffer', () => {
    const s = createQueryFilterState()
    s.setAiPatternProbe((forMode) => (forMode === 'filename' ? '*.injected' : null))
    s.setMode('ai')
    s.setQueryFromUserInput('AI prompt')
    s.switchMode('filename')
    expect(s.getQuery()).toBe('*.injected')
  })
})

describe('createQueryFilterState: history filters round-trip', () => {
  it('applies and reads back size and date filters', () => {
    const s = createQueryFilterState()
    s.setSizeFilter('between')
    s.setSizeValue('1')
    s.setSizeUnit('MB')
    s.setSizeValueMax('10')
    s.setSizeUnitMax('MB')
    s.setDateFilter('after')
    s.setDateValue('2026-01-01')
    const filters = s.readHistoryFilters()
    expect(filters).toEqual({
      sizeMin: 1024 * 1024,
      sizeMax: 10 * 1024 * 1024,
      modifiedAfter: '2026-01-01',
    })

    const fresh = createQueryFilterState()
    fresh.applyHistoryFilters(filters)
    expect(fresh.readHistoryFilters()).toEqual(filters)
    expect(fresh.getSizeFilter()).toBe('between')
    expect(fresh.getDateFilter()).toBe('after')
  })

  it('resets size + date on each applyHistoryFilters call', () => {
    const s = createQueryFilterState()
    s.setSizeFilter('gte')
    s.setSizeValue('5')
    s.setSizeUnit('GB')
    s.applyHistoryFilters({})
    expect(s.getSizeFilter()).toBe('any')
    expect(s.getSizeValue()).toBe('')
  })

  // `eq` persists as `size_min == size_max` (no Rust comparator field) and, by deliberate
  // decision, ALWAYS rehydrates as `eq` (not `between`): the two are semantically identical
  // and `= x` is the friendlier label.
  it('round-trips an eq filter (persists as min==max, restores as eq)', () => {
    const s = createQueryFilterState()
    s.setSizeFilter('eq')
    s.setSizeValue('5')
    s.setSizeUnit('MB')
    const filters = s.readHistoryFilters()
    expect(filters).toEqual({ sizeMin: 5 * 1024 * 1024, sizeMax: 5 * 1024 * 1024 })

    const fresh = createQueryFilterState()
    fresh.applyHistoryFilters(filters)
    expect(fresh.getSizeFilter()).toBe('eq')
    expect(fresh.getSizeValue()).toBe('5')
    expect(fresh.getSizeUnit()).toBe('MB')
    expect(fresh.getSizeValueMax()).toBe('')
  })

  it('round-trips eq 0 B (find empty files)', () => {
    const s = createQueryFilterState()
    s.setSizeFilter('eq')
    s.setSizeValue('0')
    s.setSizeUnit('B')
    const filters = s.readHistoryFilters()
    expect(filters).toEqual({ sizeMin: 0, sizeMax: 0 })

    const fresh = createQueryFilterState()
    fresh.applyHistoryFilters(filters)
    expect(fresh.getSizeFilter()).toBe('eq')
    expect(fresh.getSizeValue()).toBe('0')
    expect(fresh.getSizeUnit()).toBe('B')
  })
})

describe('createQueryFilterState: factory isolation', () => {
  it('two instances do not share state', () => {
    const a = createQueryFilterState()
    const b = createQueryFilterState()
    a.setQuery('left')
    b.setQuery('right')
    expect(a.getQuery()).toBe('left')
    expect(b.getQuery()).toBe('right')
    a.switchMode('regex')
    expect(a.getMode()).toBe('regex')
    expect(b.getMode()).toBe('filename')
  })
})

describe('createQueryFilterState: clearCore', () => {
  it('resets every core field to defaults', () => {
    const s = createQueryFilterState()
    s.setQuery('something')
    s.setMode('regex')
    s.setSizeFilter('gte')
    s.setDateFilter('after')
    s.setCaseSensitive(true)
    s.setLastAiPrompt('prompt')
    s.setLastAiCaveat('caveat')
    s.setResults([])
    s.setCursorIndex(5)
    s.setHandTypedBuffer('filename', '*.foo')
    s.setRunOnMount(true)
    s.setLastRunQuery('last')

    s.clearCore()
    expect(s.getQuery()).toBe('')
    expect(s.getMode()).toBe('filename')
    expect(s.getSizeFilter()).toBe('any')
    expect(s.getDateFilter()).toBe('any')
    expect(s.getCaseSensitive()).toBe(false)
    expect(s.getLastAiPrompt()).toBeNull()
    expect(s.getLastAiCaveat()).toBeNull()
    expect(s.getResults()).toEqual([])
    expect(s.getCursorIndex()).toBe(0)
    expect(s.getRunOnMount()).toBe(false)
    expect(s.getLastRunQuery()).toBeNull()
    expect(s.getHandTypedBuffer('filename')).toBe('')
  })
})

describe('createQueryFilterState: recordAiTranslation contract', () => {
  it('writes ONLY to the matching hand-typed buffer (glob to filename)', () => {
    const s = createQueryFilterState()
    s.recordAiTranslation({ pattern: '*.pdf', kind: 'glob' })
    expect(s.getHandTypedBuffer('filename')).toBe('*.pdf')
    expect(s.getHandTypedBuffer('regex')).toBe('')
    expect(s.getHandTypedBuffer('ai')).toBe('')
  })

  it('writes ONLY to the regex buffer for kind=regex', () => {
    const s = createQueryFilterState()
    s.recordAiTranslation({ pattern: 'foo.*', kind: 'regex' })
    expect(s.getHandTypedBuffer('regex')).toBe('foo.*')
    expect(s.getHandTypedBuffer('filename')).toBe('')
  })

  it('leaves the buffers alone when pattern is empty or null', () => {
    const s = createQueryFilterState()
    s.setHandTypedBuffer('filename', '*.preserved')
    s.recordAiTranslation({ pattern: null, kind: 'glob' })
    expect(s.getHandTypedBuffer('filename')).toBe('*.preserved')
    s.recordAiTranslation({ pattern: '   ', kind: 'glob' })
    expect(s.getHandTypedBuffer('filename')).toBe('*.preserved')
  })
})
