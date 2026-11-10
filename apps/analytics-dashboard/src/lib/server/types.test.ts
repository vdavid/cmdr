import { describe, it, expect, vi, afterEach } from 'vitest'
import { resolveSelection, toTimeWindow, selectionCacheKey } from './types.js'

describe('resolveSelection', () => {
  it('defaults to 30d when nothing is given', () => {
    expect(resolveSelection(null)).toEqual({ range: '30d', end: null })
  })

  it('accepts the known ranges', () => {
    expect(resolveSelection('7d')).toEqual({ range: '7d', end: null })
    expect(resolveSelection('30d')).toEqual({ range: '30d', end: null })
    expect(resolveSelection('90d')).toEqual({ range: '90d', end: null })
  })

  it('falls back to 30d for an unknown or removed range', () => {
    expect(resolveSelection('99d')).toEqual({ range: '30d', end: null })
    expect(resolveSelection('today')).toEqual({ range: '30d', end: null })
    expect(resolveSelection('24h')).toEqual({ range: '30d', end: null })
  })

  it('keeps a past `end` and reads anything else as the window ending today', () => {
    expect(resolveSelection('7d', '2026-10-10', '2026-11-10')).toEqual({ range: '7d', end: '2026-10-10' })
    expect(resolveSelection('7d', '2026-11-09', '2026-11-10')).toEqual({ range: '7d', end: null })
    expect(resolveSelection('7d', 'garbage', '2026-11-10')).toEqual({ range: '7d', end: null })
  })
})

describe('toTimeWindow', () => {
  afterEach(() => vi.useRealTimers())

  it('treats each range as a rolling window ending now', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-12T12:00:00Z'))
    for (const [range, days] of [
      ['7d', 7],
      ['30d', 30],
      ['90d', 90],
    ] as const) {
      const w = toTimeWindow({ range, end: null })
      expect(w.endAt).toBe(Date.parse('2026-06-12T12:00:00Z'))
      expect(w.endAt - w.startAt).toBe(days * 86_400_000)
    }
  })
})

describe('selectionCacheKey', () => {
  it('uses the range name', () => {
    expect(selectionCacheKey({ range: '7d', end: null })).toBe('7d')
    expect(selectionCacheKey({ range: '90d', end: null })).toBe('90d')
  })
})
