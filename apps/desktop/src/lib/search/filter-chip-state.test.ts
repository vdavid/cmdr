/**
 * Unit tests for the pure chip-state derivation helpers.
 *
 * Pins the "default → configured → cleared" rules from §3.2 of the search redesign plan:
 *   - A filter is "configured" only when the user has supplied a meaningful value, not just
 *     changed the comparator. (Picking "gte" without typing a number is still default.)
 *   - The clear path is just resetting the filter back to "any" plus empty values; this test
 *     pins the *display* side of clear (configured == false again).
 *   - Summaries use the en dash for ranges, never em dash. See `docs/style-guide.md`.
 */

import { describe, it, expect } from 'vitest'
import { deriveSizeChip, deriveDateChip, deriveScopeChip, derivePatternChip } from './filter-chip-state'

describe('deriveSizeChip', () => {
  it('returns default (not configured) when filter is "any"', () => {
    expect(deriveSizeChip('any', '', 'MB', '', 'MB')).toEqual({ configured: false, summary: '' })
  })

  it('returns default when comparator is set but value is empty (picked "gte" but did not type)', () => {
    expect(deriveSizeChip('gte', '', 'MB', '', 'MB')).toEqual({ configured: false, summary: '' })
  })

  it('returns default when value is zero (treated as "no value yet")', () => {
    expect(deriveSizeChip('gte', '0', 'MB', '', 'MB')).toEqual({ configured: false, summary: '' })
  })

  it('formats a gte filter as "> N UNIT"', () => {
    expect(deriveSizeChip('gte', '100', 'MB', '', 'MB')).toEqual({
      configured: true,
      summary: '> 100 MB',
    })
  })

  it('formats a lte filter as "< N UNIT"', () => {
    expect(deriveSizeChip('lte', '5', 'GB', '', 'MB')).toEqual({
      configured: true,
      summary: '< 5 GB',
    })
  })

  it('formats a fully-specified between filter with an en dash', () => {
    const result = deriveSizeChip('between', '10', 'MB', '500', 'MB')
    expect(result.configured).toBe(true)
    expect(result.summary).toContain('–') // en dash
    expect(result.summary).not.toContain('—') // never em dash
    expect(result.summary).toBe('10 MB – 500 MB')
  })

  it('between with only min behaves like gte', () => {
    expect(deriveSizeChip('between', '10', 'MB', '', 'MB').summary).toBe('> 10 MB')
  })

  it('between with only max behaves like lte', () => {
    expect(deriveSizeChip('between', '', 'MB', '500', 'MB').summary).toBe('< 500 MB')
  })
})

describe('deriveDateChip', () => {
  it('returns default when filter is "any"', () => {
    expect(deriveDateChip('any', '', '')).toEqual({ configured: false, summary: '' })
  })

  it('returns default when comparator is set but date is empty', () => {
    expect(deriveDateChip('after', '', '')).toEqual({ configured: false, summary: '' })
  })

  it('formats an after filter as "after DATE"', () => {
    expect(deriveDateChip('after', '2026-04-01', '')).toEqual({
      configured: true,
      summary: 'after 2026-04-01',
    })
  })

  it('formats a before filter as "before DATE"', () => {
    expect(deriveDateChip('before', '2026-04-01', '')).toEqual({
      configured: true,
      summary: 'before 2026-04-01',
    })
  })

  it('formats a fully-specified between with an en dash', () => {
    const result = deriveDateChip('between', '2026-01-01', '2026-03-31')
    expect(result.summary).toBe('2026-01-01 – 2026-03-31')
    expect(result.summary).not.toContain('—')
  })
})

describe('deriveScopeChip', () => {
  it('default state: no scope text and system dirs hidden', () => {
    expect(deriveScopeChip('', true)).toEqual({ configured: false, summary: '' })
  })

  it('is configured when scope is set', () => {
    const result = deriveScopeChip('~/Documents', true)
    expect(result.configured).toBe(true)
    expect(result.summary).toBe('~/Documents')
  })

  it('is configured even with empty scope when system dirs are included', () => {
    const result = deriveScopeChip('', false)
    expect(result.configured).toBe(true)
    expect(result.summary).toBe('includes system folders')
  })

  it('truncates very long scope strings for the chip display', () => {
    const longScope = '/Users/me/very/very/long/path/that/exceeds/the/chip/limit/several/times/over'
    const result = deriveScopeChip(longScope, true)
    expect(result.configured).toBe(true)
    expect(result.summary.length).toBeLessThanOrEqual(40)
    expect(result.summary.endsWith('…')).toBe(true)
  })

  it('trims surrounding whitespace before deciding configured', () => {
    expect(deriveScopeChip('   ', true)).toEqual({ configured: false, summary: '' })
  })
})

describe('derivePatternChip (search-fixup-brief clarification 5)', () => {
  it('reads from `query` in filename mode', () => {
    expect(derivePatternChip({ mode: 'filename', query: '*.pdf', aiPattern: null })).toEqual({
      configured: true,
      summary: '*.pdf',
    })
  })

  it('reads from `query` in regex mode', () => {
    expect(derivePatternChip({ mode: 'regex', query: '^foo$', aiPattern: '*.pdf' })).toEqual({
      configured: true,
      summary: '^foo$',
    })
  })

  it('reads from `aiPattern` in AI mode (the bar holds the prompt)', () => {
    expect(derivePatternChip({ mode: 'ai', query: 'find my pdfs', aiPattern: '*.pdf' })).toEqual({
      configured: true,
      summary: '*.pdf',
    })
  })

  it('is unconfigured when the chosen pattern slot is empty', () => {
    expect(derivePatternChip({ mode: 'filename', query: '', aiPattern: null })).toEqual({
      configured: false,
      summary: '',
    })
    expect(derivePatternChip({ mode: 'ai', query: 'big pdfs', aiPattern: null })).toEqual({
      configured: false,
      summary: '',
    })
  })

  it('truncates very long patterns to keep the chip tidy', () => {
    const long = '*'.repeat(80)
    const result = derivePatternChip({ mode: 'filename', query: long, aiPattern: null })
    expect(result.configured).toBe(true)
    expect(result.summary.length).toBeLessThanOrEqual(40)
    expect(result.summary.endsWith('…')).toBe(true)
  })
})
