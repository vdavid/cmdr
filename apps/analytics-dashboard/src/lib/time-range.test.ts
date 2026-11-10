import { describe, expect, it } from 'vitest'
import {
  earliestStart,
  formatWindow,
  funnelDaysFor,
  normalizeEnd,
  selectionQuery,
  stepEnd,
  viewWindow,
  windowDays,
} from './time-range.js'

const today = '2026-11-10'

describe('viewWindow', () => {
  it('draws the range of complete days plus today (partial) for the current window', () => {
    expect(viewWindow('30d', null, today)).toEqual({ start: '2026-10-11', end: '2026-11-10', current: true })
    expect(viewWindow('7d', null, today)).toEqual({ start: '2026-11-03', end: '2026-11-10', current: true })
  })

  it('draws exactly the range, ending `end`, for a past window', () => {
    expect(viewWindow('30d', '2026-10-10', today)).toEqual({ start: '2026-09-11', end: '2026-10-10', current: false })
    expect(viewWindow('7d', '2026-11-02', today)).toEqual({ start: '2026-10-27', end: '2026-11-02', current: false })
  })
})

describe('windowDays', () => {
  it('counts the current window as the range plus today, so the country table covers what the charts draw', () => {
    expect(windowDays(viewWindow('7d', null, today))).toBe(8)
    expect(windowDays(viewWindow('30d', null, today))).toBe(31)
    expect(windowDays(viewWindow('90d', null, today))).toBe(91)
  })

  it('counts a past window as exactly the range', () => {
    expect(windowDays(viewWindow('30d', '2026-10-10', today))).toBe(30)
  })
})

describe('formatWindow', () => {
  it('joins the ISO days with an en dash', () => {
    expect(formatWindow({ start: '2026-09-11', end: '2026-10-10', current: false })).toBe('2026-09-11 – 2026-10-10')
  })
})

describe('stepEnd', () => {
  it('steps back from the current window to the range just before its complete days', () => {
    // The current 30d window's complete days are 2026-10-11..2026-11-09.
    expect(stepEnd('30d', null, today, 'back')).toBe('2026-10-10')
    expect(stepEnd('7d', null, today, 'back')).toBe('2026-11-02')
  })

  it('steps back and forward by the range length, landing on the current window at the end', () => {
    expect(stepEnd('30d', '2026-10-10', today, 'back')).toBe('2026-09-10')
    expect(stepEnd('30d', '2026-09-10', today, 'forward')).toBe('2026-10-10')
    expect(stepEnd('30d', '2026-10-10', today, 'forward')).toBeNull()
  })

  it('lands on the current window when a step forward would reach yesterday or later', () => {
    expect(stepEnd('7d', '2026-11-05', today, 'forward')).toBeNull()
  })

  it("can't step forward from the current window", () => {
    expect(stepEnd('30d', null, today, 'forward')).toBeUndefined()
  })

  it("can't step back past the history the deepest funnel fetch covers", () => {
    // 90d windows ending today: back 1 → ends 2026-08-11, 2 → 2026-05-13, 3 → 2026-02-12; 4 would start before the floor.
    expect(stepEnd('90d', '2026-05-13', today, 'back')).toBe('2026-02-12')
    expect(stepEnd('90d', '2026-02-12', today, 'back')).toBeUndefined()
    const floor = earliestStart(today)
    expect(viewWindow('90d', '2026-02-12', today).start >= floor).toBe(true)
  })
})

describe('earliestStart', () => {
  it('leaves six days before the window for the rolling line inside 400 fetched days', () => {
    // 400 days ending 2026-11-10 start 2025-10-07; plus six rolling days.
    expect(earliestStart(today)).toBe('2025-10-13')
  })
})

describe('normalizeEnd', () => {
  it('reads no `end`, a malformed one, or an impossible day as the current window', () => {
    for (const raw of [null, '', 'yesterday', '2026-13-01', '2026-02-30', '2026-1-01']) {
      expect(normalizeEnd('30d', raw, today), String(raw)).toBeNull()
    }
  })

  it('reads yesterday or later as the current window', () => {
    expect(normalizeEnd('30d', '2026-11-09', today)).toBeNull()
    expect(normalizeEnd('30d', '2027-01-01', today)).toBeNull()
  })

  it('keeps a past day', () => {
    expect(normalizeEnd('30d', '2026-10-10', today)).toBe('2026-10-10')
  })

  it('pulls a day too old for the history up to the oldest window that fits', () => {
    expect(normalizeEnd('30d', '2024-01-01', today)).toBe('2025-11-11')
    expect(viewWindow('30d', '2025-11-11', today).start).toBe(earliestStart(today))
  })
})

describe('funnelDaysFor', () => {
  it('fetches the usual 181 days while the window and its rolling line fit in them', () => {
    expect(funnelDaysFor(viewWindow('90d', null, today), today)).toBe(181)
    expect(funnelDaysFor(viewWindow('30d', '2026-08-01', today), today)).toBe(181)
  })

  it('fetches the 400-day history once the window reaches further back', () => {
    expect(funnelDaysFor(viewWindow('90d', '2026-08-11', today), today)).toBe(400)
  })
})

describe('selectionQuery', () => {
  it('writes the range, the end when set, and the country when set', () => {
    expect(selectionQuery({ range: '30d', end: null }, null)).toBe('?range=30d')
    expect(selectionQuery({ range: '7d', end: '2026-10-10' }, 'SE')).toBe('?range=7d&end=2026-10-10&country=SE')
  })
})
