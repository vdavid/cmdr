import { describe, it, expect } from 'vitest'
import {
  formatAverage,
  formatMultiple,
  formatMultipleChange,
  formatPercent,
  formatPointChange,
  formatRelativeChange,
  formatUtcDateTime,
} from './format.js'

describe('rate and change formatters', () => {
  it('formats averages with a decimal only while small', () => {
    expect(formatAverage(12.345)).toBe('12.3')
    expect(formatAverage(1234.5)).toBe('1,235')
  })

  it('formats percents, including past 100%', () => {
    expect(formatPercent(0.0456)).toBe('4.6%')
    expect(formatPercent(0.456)).toBe('46%')
    expect(formatPercent(1.35)).toBe('135%')
  })

  it('formats multiples', () => {
    expect(formatMultiple(3.21)).toBe('3.2×')
    expect(formatMultiple(14.6)).toBe('15×')
  })

  it('signs changes with a real minus and a "±0" for no change', () => {
    expect(formatRelativeChange(0.5)).toBe('+50%')
    expect(formatRelativeChange(-0.123)).toBe('−12%')
    expect(formatRelativeChange(0.001)).toBe('±0%')
    expect(formatPointChange(0.0123)).toBe('+1.2 pp')
    expect(formatPointChange(-0.05)).toBe('−5 pp')
    expect(formatMultipleChange(0.44)).toBe('+0.4×')
  })
})

describe('formatUtcDateTime', () => {
  it('prints an ISO timestamp as a UTC day and time', () => {
    expect(formatUtcDateTime('2026-09-16T10:04:02.000Z')).toBe('2026-09-16 10:04')
  })

  it('stays on UTC for a timestamp written in another offset', () => {
    expect(formatUtcDateTime('2026-09-16T23:30:00+02:00')).toBe('2026-09-16 21:30')
  })

  it('returns an unparseable timestamp verbatim', () => {
    expect(formatUtcDateTime('not a date')).toBe('not a date')
  })
})
