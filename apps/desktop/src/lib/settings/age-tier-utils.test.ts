import { describe, expect, it } from 'vitest'
import { tierForYear, tierForMonth, tierForDay, tierForTime } from './age-tier-utils'

// Fixed "now": Sunday, 2026-05-11 12:00 local (matches the date in CLAUDE memory).
// Using a local construction so getFullYear/getMonth/getDate are timezone-stable.
const NOW = new Date(2026, 4, 11, 12, 0, 0)
const NOW_MS = NOW.getTime()
const toSec = (d: Date) => d.getTime() / 1000

describe('tierForYear', () => {
  it('returns null for missing or non-finite timestamps', () => {
    expect(tierForYear(null, NOW_MS)).toBeNull()
    expect(tierForYear(undefined, NOW_MS)).toBeNull()
    expect(tierForYear(Number.NaN, NOW_MS)).toBeNull()
    expect(tierForYear(Number.POSITIVE_INFINITY, NOW_MS)).toBeNull()
  })

  it('returns age-fresh for the current year', () => {
    expect(tierForYear(toSec(new Date(2026, 0, 1)), NOW_MS)).toBe('age-fresh')
    expect(tierForYear(toSec(new Date(2026, 11, 31)), NOW_MS)).toBe('age-fresh')
  })

  it('returns age-recent for last year', () => {
    expect(tierForYear(toSec(new Date(2025, 6, 15)), NOW_MS)).toBe('age-recent')
  })

  it('returns age-aging for two years ago', () => {
    expect(tierForYear(toSec(new Date(2024, 6, 15)), NOW_MS)).toBe('age-aging')
  })

  it('returns age-old for three or more years ago', () => {
    expect(tierForYear(toSec(new Date(2023, 6, 15)), NOW_MS)).toBe('age-old')
    expect(tierForYear(toSec(new Date(2010, 0, 1)), NOW_MS)).toBe('age-old')
  })

  it('clamps future years to age-fresh', () => {
    expect(tierForYear(toSec(new Date(2030, 0, 1)), NOW_MS)).toBe('age-fresh')
  })
})

describe('tierForMonth', () => {
  it('returns null when the file is in a different year', () => {
    expect(tierForMonth(toSec(new Date(2025, 4, 15)), NOW_MS)).toBeNull()
    expect(tierForMonth(toSec(new Date(2027, 0, 1)), NOW_MS)).toBeNull()
  })

  it('returns null for missing timestamps', () => {
    expect(tierForMonth(null, NOW_MS)).toBeNull()
    expect(tierForMonth(undefined, NOW_MS)).toBeNull()
  })

  it('returns age-fresh for the current month (May 2026)', () => {
    expect(tierForMonth(toSec(new Date(2026, 4, 1)), NOW_MS)).toBe('age-fresh')
  })

  it('returns age-recent for last month (April 2026)', () => {
    expect(tierForMonth(toSec(new Date(2026, 3, 15)), NOW_MS)).toBe('age-recent')
  })

  it('returns age-aging for two months ago', () => {
    expect(tierForMonth(toSec(new Date(2026, 2, 15)), NOW_MS)).toBe('age-aging')
  })

  it('returns age-old for three+ months ago in the same year', () => {
    expect(tierForMonth(toSec(new Date(2026, 1, 15)), NOW_MS)).toBe('age-old')
    expect(tierForMonth(toSec(new Date(2026, 0, 1)), NOW_MS)).toBe('age-old')
  })

  it('clamps future months in the same year to age-fresh', () => {
    expect(tierForMonth(toSec(new Date(2026, 8, 1)), NOW_MS)).toBe('age-fresh')
  })
})

describe('tierForDay', () => {
  it('returns null when year or month differs', () => {
    expect(tierForDay(toSec(new Date(2025, 4, 11)), NOW_MS)).toBeNull()
    expect(tierForDay(toSec(new Date(2026, 3, 11)), NOW_MS)).toBeNull()
  })

  it('returns null for missing timestamps', () => {
    expect(tierForDay(null, NOW_MS)).toBeNull()
  })

  it('returns age-fresh for today (May 11, 2026)', () => {
    expect(tierForDay(toSec(new Date(2026, 4, 11, 0, 0, 0)), NOW_MS)).toBe('age-fresh')
    expect(tierForDay(toSec(new Date(2026, 4, 11, 23, 0, 0)), NOW_MS)).toBe('age-fresh')
  })

  it('returns age-recent for yesterday (May 10)', () => {
    expect(tierForDay(toSec(new Date(2026, 4, 10, 12, 0, 0)), NOW_MS)).toBe('age-recent')
  })

  it('returns age-aging for two days ago (May 9)', () => {
    expect(tierForDay(toSec(new Date(2026, 4, 9, 12, 0, 0)), NOW_MS)).toBe('age-aging')
  })

  it('returns age-old for three+ days ago in the same month', () => {
    expect(tierForDay(toSec(new Date(2026, 4, 8, 12, 0, 0)), NOW_MS)).toBe('age-old')
    expect(tierForDay(toSec(new Date(2026, 4, 1, 12, 0, 0)), NOW_MS)).toBe('age-old')
  })

  it('clamps future days in the same month to age-fresh', () => {
    expect(tierForDay(toSec(new Date(2026, 4, 20, 12, 0, 0)), NOW_MS)).toBe('age-fresh')
  })
})

describe('tierForTime', () => {
  it('returns null when not the same date as now', () => {
    expect(tierForTime(toSec(new Date(2026, 4, 10, 12, 0, 0)), NOW_MS)).toBeNull()
    expect(tierForTime(toSec(new Date(2026, 3, 11, 12, 0, 0)), NOW_MS)).toBeNull()
  })

  it('returns null for missing timestamps', () => {
    expect(tierForTime(null, NOW_MS)).toBeNull()
  })

  it('returns age-fresh within the last hour', () => {
    expect(tierForTime(toSec(new Date(2026, 4, 11, 11, 30, 0)), NOW_MS)).toBe('age-fresh')
    expect(tierForTime(toSec(new Date(2026, 4, 11, 12, 0, 0)), NOW_MS)).toBe('age-fresh')
  })

  it('returns age-recent within 1-2 hours ago', () => {
    expect(tierForTime(toSec(new Date(2026, 4, 11, 10, 30, 0)), NOW_MS)).toBe('age-recent')
  })

  it('returns age-aging within 2-3 hours ago', () => {
    expect(tierForTime(toSec(new Date(2026, 4, 11, 9, 30, 0)), NOW_MS)).toBe('age-aging')
  })

  it('returns age-old for 3+ hours ago today', () => {
    expect(tierForTime(toSec(new Date(2026, 4, 11, 8, 0, 0)), NOW_MS)).toBe('age-old')
    expect(tierForTime(toSec(new Date(2026, 4, 11, 0, 0, 0)), NOW_MS)).toBe('age-old')
  })

  it('clamps future times today to age-fresh', () => {
    expect(tierForTime(toSec(new Date(2026, 4, 11, 14, 0, 0)), NOW_MS)).toBe('age-fresh')
  })
})
