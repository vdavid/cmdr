import { describe, expect, it } from 'vitest'
import { formatElapsedClock } from './elapsed'

describe('formatElapsedClock', () => {
  it('returns null under a second (so the clock never flashes 0:00)', () => {
    expect(formatElapsedClock(0)).toBeNull()
    expect(formatElapsedClock(999)).toBeNull()
  })

  it('returns null for non-finite or negative input', () => {
    expect(formatElapsedClock(Number.NaN)).toBeNull()
    expect(formatElapsedClock(Number.POSITIVE_INFINITY)).toBeNull()
    expect(formatElapsedClock(-5000)).toBeNull()
  })

  it('formats sub-minute durations as 0:SS', () => {
    expect(formatElapsedClock(1000)).toBe('0:01')
    expect(formatElapsedClock(42_000)).toBe('0:42')
  })

  it('zero-pads the seconds past a minute', () => {
    expect(formatElapsedClock(60_000)).toBe('1:00')
    expect(formatElapsedClock((12 * 60 + 5) * 1000)).toBe('12:05')
  })

  it('floors fractional seconds', () => {
    expect(formatElapsedClock(1900)).toBe('0:01')
  })
})
