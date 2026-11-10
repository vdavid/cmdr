import { describe, it, expect } from 'vitest'
import {
  addDays,
  countStats,
  isWeekend,
  median,
  niceTicks,
  ratioStats,
  rollingRatio,
  type CountPoint,
  type RatioPoint,
} from './acquisition-stats.js'

/** `n` consecutive UTC days ending the day before `endExclusive`, oldest first. */
function days(n: number, endExclusive: string): string[] {
  return Array.from({ length: n }, (_, i) => addDays(endExclusive, i - n))
}

const today = '2026-11-10' // a Tuesday

describe('addDays', () => {
  it('moves across month boundaries in UTC', () => {
    expect(addDays('2026-11-01', -1)).toBe('2026-10-31')
    expect(addDays('2026-12-31', 1)).toBe('2027-01-01')
  })
})

describe('isWeekend', () => {
  it('reads the UTC weekday of a day string', () => {
    expect(isWeekend('2026-11-07')).toBe(true) // Saturday
    expect(isWeekend('2026-11-08')).toBe(true) // Sunday
    expect(isWeekend('2026-11-09')).toBe(false) // Monday
  })
})

describe('median', () => {
  it('takes the middle value, or the mean of the two middle ones', () => {
    expect(median([3, 1, 2])).toBe(2)
    expect(median([4, 1, 2, 3])).toBe(2.5)
    expect(median([])).toBeNull()
  })
})

describe('countStats', () => {
  it('excludes today (a partial day) from every stat', () => {
    const points: CountPoint[] = [...days(7, today).map((date) => ({ date, value: 10 })), { date: today, value: 1000 }]
    const s = countStats(points, 7, today)
    expect(s.average).toBe(10)
    expect(s.median).toBe(10)
  })

  it('splits weekdays from weekends by UTC day', () => {
    const points = days(7, today).map((date) => ({ date, value: isWeekend(date) ? 2 : 12 }))
    const s = countStats(points, 7, today)
    expect(s.weekdayAverage).toBe(12)
    expect(s.weekendAverage).toBe(2)
  })

  it('compares against the previous same-length window', () => {
    const points = days(14, today).map((date, i) => ({ date, value: i < 7 ? 10 : 15 }))
    expect(countStats(points, 7, today).change).toBeCloseTo(0.5)
  })

  it('reports no change when the previous window is not fully covered', () => {
    const points = days(10, today).map((date) => ({ date, value: 10 }))
    expect(countStats(points, 7, today).change).toBeNull()
  })

  it('returns all-null stats when the source was unavailable, never zeros', () => {
    const points = days(7, today).map((date) => ({ date, value: null }))
    const s = countStats(points, 7, today)
    expect(s).toEqual({ average: null, median: null, weekdayAverage: null, weekendAverage: null, change: null })
  })

  it('reports null for a window longer than the data', () => {
    const points = days(20, today).map((date) => ({ date, value: 5 }))
    expect(countStats(points, 30, today).average).toBeNull()
  })
})

describe('ratioStats', () => {
  it('pools as a ratio of sums, never an average of daily ratios', () => {
    // Day A: 1/1 = 100%, day B: 1/99 ≈ 1%. Average of ratios ≈ 50.5%; ratio of sums = 2/100 = 2%.
    const points: RatioPoint[] = [
      { date: addDays(today, -2), num: 1, den: 1 },
      { date: addDays(today, -1), num: 1, den: 99 },
    ]
    expect(ratioStats(points, 2, today).average).toBeCloseTo(0.02)
  })

  it('can pass 100% (installs outrun clicks)', () => {
    const points = days(7, today).map((date) => ({ date, num: 3, den: 2 }))
    expect(ratioStats(points, 7, today).average).toBeCloseTo(1.5)
  })

  it('pools weekdays and weekends separately', () => {
    const points = days(7, today).map((date) =>
      isWeekend(date) ? { date, num: 1, den: 10 } : { date, num: 3, den: 10 },
    )
    const s = ratioStats(points, 7, today)
    expect(s.weekdayAverage).toBeCloseTo(0.3)
    expect(s.weekendAverage).toBeCloseTo(0.1)
  })

  it('reports change as the absolute difference of pooled rates', () => {
    const points = days(14, today).map((date, i) => ({ date, num: i < 7 ? 1 : 2, den: 10 }))
    expect(ratioStats(points, 7, today).change).toBeCloseTo(0.1)
  })

  it('takes the median of daily rates, skipping days with no denominator', () => {
    const points: RatioPoint[] = [
      { date: addDays(today, -3), num: 1, den: 10 },
      { date: addDays(today, -2), num: 0, den: 0 },
      { date: addDays(today, -1), num: 3, den: 10 },
    ]
    expect(ratioStats(points, 3, today).median).toBeCloseTo(0.2)
  })

  it('is null when the denominator sums to zero', () => {
    const points = days(7, today).map((date) => ({ date, num: 0, den: 0 }))
    expect(ratioStats(points, 7, today).average).toBeNull()
  })

  it('anchors on a custom end, so D7 windows cover only final cohorts', () => {
    const finalEnd = addDays(today, -7)
    const points = days(14, today).map((date) => ({ date, num: date < finalEnd ? 1 : 9, den: 10 }))
    expect(ratioStats(points, 7, finalEnd).average).toBeCloseTo(0.1)
  })
})

describe('rollingRatio', () => {
  it('pools the trailing window as a ratio of sums', () => {
    const points: RatioPoint[] = [
      { date: '2026-11-01', num: 1, den: 1 },
      { date: '2026-11-02', num: 1, den: 99 },
      { date: '2026-11-03', num: 0, den: 0 },
    ]
    expect(rollingRatio(points, 2)).toEqual([null, 0.02, 1 / 99])
  })

  it('stays null until a full window of history exists', () => {
    const points = days(3, today).map((date) => ({ date, num: 1, den: 2 }))
    expect(rollingRatio(points, 7)).toEqual([null, null, null])
  })

  it('is null across a gap in the source', () => {
    const points: RatioPoint[] = [
      { date: '2026-11-01', num: 1, den: 2 },
      { date: '2026-11-02', num: null, den: 2 },
    ]
    expect(rollingRatio(points, 2)).toEqual([null, null])
  })
})

describe('niceTicks', () => {
  it('steps in clean multiples and ends at or above the max', () => {
    expect(niceTicks(109, { integer: true })).toEqual([0, 50, 100, 150])
    expect(niceTicks(26, { integer: true })).toEqual([0, 10, 20, 30])
    expect(niceTicks(0.4)).toEqual([0, 0.1, 0.2, 0.3, 0.4])
    expect(niceTicks(1.37)).toEqual([0, 0.5, 1, 1.5])
  })

  it('never steps by a fraction on an integer axis', () => {
    expect(niceTicks(3, { integer: true })).toEqual([0, 1, 2, 3])
  })

  it('draws a 0–1 axis for an all-zero series', () => {
    expect(niceTicks(0, { integer: true })).toEqual([0, 1])
  })
})
