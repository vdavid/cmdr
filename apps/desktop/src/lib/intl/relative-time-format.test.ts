import { afterEach, describe, expect, it } from 'vitest'

import { _setLocaleForTests } from './locale'
import { formatTimeAgo, relativeTimeParts } from './relative-time-format'

const NOW = 1_800_000_000
const MINUTE = 60
const HOUR = 3600
const DAY = 86_400

describe('relativeTimeParts', () => {
  it('picks the largest unit the span fills', () => {
    expect(relativeTimeParts(10)).toEqual({ value: 0, unit: 'second' })
    expect(relativeTimeParts(5 * MINUTE + 30)).toEqual({ value: 5, unit: 'minute' })
    expect(relativeTimeParts(3 * HOUR)).toEqual({ value: 3, unit: 'hour' })
    expect(relativeTimeParts(2 * DAY + HOUR)).toEqual({ value: 2, unit: 'day' })
    expect(relativeTimeParts(15 * DAY)).toEqual({ value: 2, unit: 'week' })
    expect(relativeTimeParts(65 * DAY)).toEqual({ value: 2, unit: 'month' })
    expect(relativeTimeParts(800 * DAY)).toEqual({ value: 2, unit: 'year' })
  })

  it('reads a future moment (a clock skew) as now', () => {
    expect(relativeTimeParts(-500)).toEqual({ value: 0, unit: 'second' })
  })
})

describe('formatTimeAgo', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('says it in English idiom', () => {
    _setLocaleForTests('en')
    expect(formatTimeAgo(NOW - 5, NOW)).toBe('now')
    expect(formatTimeAgo(NOW - 5 * MINUTE, NOW)).toBe('5 minutes ago')
    expect(formatTimeAgo(NOW - DAY, NOW)).toBe('yesterday')
    expect(formatTimeAgo(NOW - 2 * DAY, NOW)).toBe('2 days ago')
  })

  it('speaks the UI language', () => {
    _setLocaleForTests('de')
    expect(formatTimeAgo(NOW - 3 * DAY, NOW)).toBe('vor 3 Tagen')
  })
})
