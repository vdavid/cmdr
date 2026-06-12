import { describe, it, expect, vi, afterEach } from 'vitest'
import { resolveSelection, toTimeWindow, selectionCacheKey, selectionToWorkerRange } from './types.js'

describe('resolveSelection', () => {
  it('defaults to 7d when nothing is given', () => {
    expect(resolveSelection(null, null)).toEqual({ range: '7d', day: null })
  })

  it('accepts the known relative ranges, including today', () => {
    expect(resolveSelection('today', null)).toEqual({ range: 'today', day: null })
    expect(resolveSelection('24h', null)).toEqual({ range: '24h', day: null })
    expect(resolveSelection('30d', null)).toEqual({ range: '30d', day: null })
  })

  it('falls back to 7d for an unknown range', () => {
    expect(resolveSelection('99d', null)).toEqual({ range: '7d', day: null })
  })

  it('a valid day param wins and forces range=day, ignoring the range param', () => {
    expect(resolveSelection('30d', '2026-06-05')).toEqual({ range: 'day', day: '2026-06-05' })
  })

  it('ignores a malformed day param', () => {
    expect(resolveSelection('7d', 'yesterday')).toEqual({ range: '7d', day: null })
  })
})

describe('toTimeWindow', () => {
  afterEach(() => vi.useRealTimers())

  it('spans a specific day exactly 24h from UTC midnight', () => {
    const w = toTimeWindow({ range: 'day', day: '2026-06-05' })
    expect(new Date(w.startAt).toISOString()).toBe('2026-06-05T00:00:00.000Z')
    expect(new Date(w.endAt).toISOString()).toBe('2026-06-06T00:00:00.000Z')
  })

  it('runs today from UTC midnight to now', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-12T15:30:00Z'))
    const w = toTimeWindow({ range: 'today', day: null })
    expect(new Date(w.startAt).toISOString()).toBe('2026-06-12T00:00:00.000Z')
    expect(new Date(w.endAt).toISOString()).toBe('2026-06-12T15:30:00.000Z')
  })

  it('treats 7d as a rolling 7-day window ending now', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-12T12:00:00Z'))
    const w = toTimeWindow({ range: '7d', day: null })
    expect(w.endAt - w.startAt).toBe(7 * 86_400_000)
  })
})

describe('selectionCacheKey', () => {
  it('uses the range name for relative ranges', () => {
    expect(selectionCacheKey({ range: '7d', day: null })).toBe('7d')
    expect(selectionCacheKey({ range: 'today', day: null })).toBe('today')
  })

  it('uses day:YYYY-MM-DD for a specific day so two days never collide', () => {
    expect(selectionCacheKey({ range: 'day', day: '2026-06-05' })).toBe('day:2026-06-05')
  })
})

describe('selectionToWorkerRange', () => {
  it('keeps 7d and 30d, snaps today / 24h / a specific day to 24h', () => {
    expect(selectionToWorkerRange({ range: '7d', day: null })).toBe('7d')
    expect(selectionToWorkerRange({ range: '30d', day: null })).toBe('30d')
    expect(selectionToWorkerRange({ range: 'today', day: null })).toBe('24h')
    expect(selectionToWorkerRange({ range: '24h', day: null })).toBe('24h')
    expect(selectionToWorkerRange({ range: 'day', day: '2026-06-05' })).toBe('24h')
  })
})
