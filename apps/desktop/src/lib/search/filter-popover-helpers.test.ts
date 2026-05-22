/**
 * Round 2 D10 / D11: pins the pure helpers driving the grid-style Size and Modified
 * filter popovers.
 */
import { describe, it, expect } from 'vitest'
import {
  SIZE_PRESETS,
  DATE_PRESETS,
  CUSTOM_VALUE,
  byteUnitLabel,
  kiloByteLabel,
  isSizeRangeDisabled,
  showsUpperBound,
  isDateRangeDisabled,
  showsDateUpperBound,
  resolveDatePreset,
  resolveFirstDayOfWeek,
  buildDatePresets,
} from './filter-popover-helpers'

describe('SIZE_PRESETS', () => {
  it('matches the brief verbatim (0, 1, 5, 10, 20, 50, 100, 200, 500)', () => {
    expect(SIZE_PRESETS).toEqual(['0', '1', '5', '10', '20', '50', '100', '200', '500'])
  })
})

describe('byteUnitLabel (D10)', () => {
  it("returns 'byte' when the selected value is exactly '1'", () => {
    expect(byteUnitLabel('1')).toBe('byte')
  })

  it("returns 'bytes' for every other value, including '0'", () => {
    expect(byteUnitLabel('0')).toBe('bytes')
    expect(byteUnitLabel('5')).toBe('bytes')
    expect(byteUnitLabel('200')).toBe('bytes')
  })

  it('returns the plural for an empty / custom selection', () => {
    expect(byteUnitLabel('')).toBe('bytes')
    expect(byteUnitLabel(CUSTOM_VALUE)).toBe('bytes')
  })
})

describe('kiloByteLabel (D10)', () => {
  it("uses uppercase 'KB' for binary mode (default)", () => {
    expect(kiloByteLabel('binary')).toBe('KB')
  })

  it("uses lowercase k 'kB' for SI mode", () => {
    expect(kiloByteLabel('si')).toBe('kB')
  })
})

describe('isSizeRangeDisabled / showsUpperBound (D10)', () => {
  it("disables col 2 + col 3 only when comparator is 'any'", () => {
    expect(isSizeRangeDisabled('any')).toBe(true)
    expect(isSizeRangeDisabled('gte')).toBe(false)
    expect(isSizeRangeDisabled('lte')).toBe(false)
    expect(isSizeRangeDisabled('between')).toBe(false)
  })

  it("renders the upper-bound cols only for 'between'", () => {
    expect(showsUpperBound('between')).toBe(true)
    expect(showsUpperBound('any')).toBe(false)
    expect(showsUpperBound('gte')).toBe(false)
    expect(showsUpperBound('lte')).toBe(false)
  })
})

describe('isDateRangeDisabled / showsDateUpperBound (D11)', () => {
  it("disables value col only when comparator is 'any'", () => {
    expect(isDateRangeDisabled('any')).toBe(true)
    expect(isDateRangeDisabled('after')).toBe(false)
    expect(isDateRangeDisabled('before')).toBe(false)
    expect(isDateRangeDisabled('between')).toBe(false)
  })

  it("renders the upper-bound col only for 'between'", () => {
    expect(showsDateUpperBound('between')).toBe(true)
    expect(showsDateUpperBound('any')).toBe(false)
    expect(showsDateUpperBound('after')).toBe(false)
    expect(showsDateUpperBound('before')).toBe(false)
  })
})

describe('DATE_PRESETS', () => {
  it('contains the seven brief-spec presets in order', () => {
    expect(DATE_PRESETS.map((p) => p.key)).toEqual([
      'today',
      'yesterday',
      'thisWeek',
      'lastWeek',
      'thisMonth',
      'lastMonth',
      'thisYear',
    ])
  })
})

describe('resolveDatePreset (D11)', () => {
  // Anchor: Wednesday 2026-05-20 16:00:00 local time. ISO weekday = 3 (Mon=1).
  const anchor = new Date(2026, 4, 20, 16, 0, 0)

  it('today returns the anchor day at midnight', () => {
    expect(resolveDatePreset('today', anchor)).toBe('2026-05-20')
  })

  it('yesterday returns the day before', () => {
    expect(resolveDatePreset('yesterday', anchor)).toBe('2026-05-19')
  })

  it('thisWeek returns Monday of the same week', () => {
    expect(resolveDatePreset('thisWeek', anchor)).toBe('2026-05-18')
  })

  it('lastWeek returns Monday of the previous week', () => {
    expect(resolveDatePreset('lastWeek', anchor)).toBe('2026-05-11')
  })

  it('thisMonth returns the first of the current month', () => {
    expect(resolveDatePreset('thisMonth', anchor)).toBe('2026-05-01')
  })

  it('lastMonth returns the first of the previous month', () => {
    expect(resolveDatePreset('lastMonth', anchor)).toBe('2026-04-01')
  })

  it('thisYear returns Jan 1 of the current year', () => {
    expect(resolveDatePreset('thisYear', anchor)).toBe('2026-01-01')
  })

  it('returns null for unknown keys (caller falls through to free-form)', () => {
    expect(resolveDatePreset('Custom...', anchor)).toBeNull()
    expect(resolveDatePreset('', anchor)).toBeNull()
  })

  it("anchors thisWeek to Monday when 'now' is itself a Sunday", () => {
    // Sunday 2026-05-24. Monday of this ISO week is 2026-05-18.
    const sunday = new Date(2026, 4, 24, 12, 0, 0)
    expect(resolveDatePreset('thisWeek', sunday)).toBe('2026-05-18')
  })
})

// R3 U4: brand-new dynamic preset labels. Anchor: Wednesday 2026-05-20.
describe('buildDatePresets (R3 U4)', () => {
  const anchor = new Date(2026, 4, 20, 16, 0, 0) // Wed May 20 2026, ISO weekday 3.

  it('today and yesterday carry the "0:00" suffix', () => {
    const list = buildDatePresets(anchor, 'en-GB')
    expect(list[0]).toEqual({ key: 'today', label: 'today 0:00', resolved: '2026-05-20' })
    expect(list[1]).toEqual({ key: 'yesterday', label: 'yesterday 0:00', resolved: '2026-05-19' })
  })

  it('this/last week label reads "this Monday 0:00" / "last Monday 0:00" for Monday-start locales', () => {
    const list = buildDatePresets(anchor, 'en-GB')
    expect(list[2]).toEqual({ key: 'thisWeek', label: 'this Monday 0:00', resolved: '2026-05-18' })
    expect(list[3]).toEqual({ key: 'lastWeek', label: 'last Monday 0:00', resolved: '2026-05-11' })
  })

  it('this month label reads "1st of May 0:00" without the year (current year is implicit)', () => {
    const list = buildDatePresets(anchor, 'en-GB')
    const thisMonth = list.find((p) => p.key === 'thisMonth')
    expect(thisMonth).toEqual({ key: 'thisMonth', label: '1st of May 0:00', resolved: '2026-05-01' })
  })

  it('last month label always includes the year', () => {
    const list = buildDatePresets(anchor, 'en-GB')
    const lastMonth = list.find((p) => p.key === 'lastMonth')
    expect(lastMonth).toEqual({
      key: 'lastMonth',
      label: '1st of April, 2026, 0:00',
      resolved: '2026-04-01',
    })
  })

  it('year start preset appears when neither current nor last month is January', () => {
    const list = buildDatePresets(anchor, 'en-GB')
    const yearStart = list.find((p) => p.key === 'yearStart')
    expect(yearStart).toEqual({
      key: 'yearStart',
      label: '1st of January, 2026, 0:00',
      resolved: '2026-01-01',
    })
  })

  it('year start preset is OMITTED when the current month is January', () => {
    const jan = new Date(2026, 0, 15, 12, 0, 0)
    const list = buildDatePresets(jan, 'en-GB')
    expect(list.find((p) => p.key === 'yearStart')).toBeUndefined()
    // The thisMonth preset already covers the year-start date.
    const thisMonth = list.find((p) => p.key === 'thisMonth')
    expect(thisMonth?.resolved).toBe('2026-01-01')
  })

  it('year start preset is OMITTED when last month is January (current is February)', () => {
    const feb = new Date(2026, 1, 10, 12, 0, 0)
    const list = buildDatePresets(feb, 'en-GB')
    expect(list.find((p) => p.key === 'yearStart')).toBeUndefined()
    // The lastMonth preset already covers the year-start date.
    const lastMonth = list.find((p) => p.key === 'lastMonth')
    expect(lastMonth?.resolved).toBe('2026-01-01')
  })
})

describe('resolveFirstDayOfWeek (R3 U4)', () => {
  it('returns 1 (Monday) as a sane fallback when language is undefined', () => {
    expect(resolveFirstDayOfWeek(undefined)).toBe(1)
  })

  it('returns 1 (Monday) when the language is invalid', () => {
    // Intl.Locale would throw on an invalid tag; the helper must catch it.
    expect(resolveFirstDayOfWeek('not-a-real-locale-tag!!')).toBe(1)
  })

  // We don't pin the return value for "en-US" because the WebKit/Chrome
  // weekInfo API isn't reliably present in jsdom; the contract is that it
  // never throws and always returns 1..7.
  it('returns a value in the 1..7 range for any input', () => {
    const v = resolveFirstDayOfWeek('en-US')
    expect(v).toBeGreaterThanOrEqual(1)
    expect(v).toBeLessThanOrEqual(7)
  })
})
