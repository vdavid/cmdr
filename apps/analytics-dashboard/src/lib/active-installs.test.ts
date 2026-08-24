import { describe, expect, it } from 'vitest'
import { boundsByDay, formatBound, largestUnseenShare, latestBound } from './active-installs.js'
import type { HeartbeatDauRow, UpdateActivityRow } from './server/sources/cloudflare.js'

function dau(rows: Array<[string, number]>): HeartbeatDauRow[] {
  return rows.map(([date, count]) => ({ date, dau: count, beats: count * 8 }))
}

function updates(rows: Array<[string, string, number]>): UpdateActivityRow[] {
  return rows.map(([day, version, updaters]) => ({ day, version, updaters }))
}

describe('boundsByDay', () => {
  it('sums the per-version update rows into one reach per day', () => {
    const bounds = boundsByDay(
      dau([['2026-08-02', 30]]),
      updates([
        ['2026-08-02', '0.40.0', 25],
        ['2026-08-02', '0.39.0', 13],
      ]),
    )
    expect(bounds).toEqual([{ day: '2026-08-02', floor: 30, reach: 38 }])
  })

  it('sorts oldest first, whatever order the rows arrive in', () => {
    const bounds = boundsByDay(
      dau([
        ['2026-08-02', 30],
        ['2026-08-01', 28],
      ]),
      [],
    )
    expect(bounds.map((b) => b.day)).toEqual(['2026-08-01', '2026-08-02'])
  })

  it('leaves the reach null on a day the update data does not cover', () => {
    // The two endpoints answer different ranges, and a zero here would read as "nobody checked",
    // which is a claim we can't make.
    const bounds = boundsByDay(dau([['2026-07-01', 12]]), updates([['2026-08-02', '0.40.0', 25]]))
    expect(bounds[0].reach).toBeNull()
  })
})

describe('formatBound', () => {
  it('reads as a range when the update check saw more', () => {
    expect(formatBound({ day: '2026-08-02', floor: 30, reach: 38 })).toBe('30–38')
  })

  it('falls back to a lower bound when the reach adds nothing', () => {
    // NAT collapsing a household, or an update check that didn't fire. It never means the fleet
    // shrank below what the heartbeat already proved.
    expect(formatBound({ day: '2026-08-02', floor: 30, reach: 22 })).toBe('30+')
    expect(formatBound({ day: '2026-08-02', floor: 30, reach: 30 })).toBe('30+')
    expect(formatBound({ day: '2026-08-02', floor: 30, reach: null })).toBe('30+')
  })

  it('renders a dash when there is nothing yet', () => {
    expect(formatBound(null)).toBe('–')
  })
})

describe('latestBound', () => {
  it('takes the last day', () => {
    const bounds = boundsByDay(
      dau([
        ['2026-08-01', 28],
        ['2026-08-02', 30],
      ]),
      [],
    )
    expect(latestBound(bounds)?.day).toBe('2026-08-02')
    expect(latestBound([])).toBeNull()
  })
})

describe('largestUnseenShare', () => {
  it('reports the widest gap as a share of the reach', () => {
    const bounds = boundsByDay(
      dau([
        ['2026-08-01', 30],
        ['2026-08-02', 30],
      ]),
      updates([
        ['2026-08-01', '0.40.0', 33],
        ['2026-08-02', '0.40.0', 40],
      ]),
    )
    expect(largestUnseenShare(bounds)).toBeCloseTo(0.25)
  })

  it('answers null when no day has a usable reach', () => {
    expect(largestUnseenShare(boundsByDay(dau([['2026-08-02', 30]]), []))).toBeNull()
  })
})
