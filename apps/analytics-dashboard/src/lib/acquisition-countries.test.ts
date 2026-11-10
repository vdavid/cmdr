import { describe, it, expect } from 'vitest'
import { chipCountries, mergeCountryRows, safeRatio } from './acquisition-countries.js'

const workerRow = (country: string, newInstalls: number) => ({
  country,
  serverDownloads: newInstalls * 3,
  newInstalls,
  d7Cohort: newInstalls,
  d7Retained: 1,
})

describe('mergeCountryRows', () => {
  it('joins both sides and ranks by visitors, then installs', () => {
    const rows = mergeCountryRows(
      new Map([
        ['US', 50],
        ['SE', 10],
      ]),
      new Map([['US', 4]]),
      [workerRow('SE', 2), workerRow('(unknown)', 5)],
    )
    expect(rows.map((r) => r.country)).toEqual(['US', 'SE', '(unknown)'])
    expect(rows[0]).toMatchObject({ visitors: 50, clicks: 4, newInstalls: 0 })
    // Umami answered for every country at once, so a country with no clicks entry clicked 0 times.
    expect(rows[1].clicks).toBe(0)
    expect(rows[2].visitors).toBe(0)
  })

  it('gives (unknown) the clicks that carried no country', () => {
    const rows = mergeCountryRows(
      new Map([['US', 5]]),
      new Map([
        ['US', 2],
        ['(unknown)', 1],
      ]),
      [workerRow('(unknown)', 3)],
    )
    expect(rows.find((r) => r.country === '(unknown)')?.clicks).toBe(1)
  })

  it('leaves a failed side null, never zero', () => {
    const rows = mergeCountryRows(null, null, [workerRow('DE', 1)])
    expect(rows[0]).toMatchObject({ country: 'DE', visitors: null, clicks: null, newInstalls: 1 })
    const umamiOnly = mergeCountryRows(new Map([['DE', 3]]), null, null)
    expect(umamiOnly[0].newInstalls).toBeNull()
  })

  it('matches the worker country codes case-insensitively', () => {
    const rows = mergeCountryRows(new Map([['DE', 3]]), null, [workerRow('de', 1)])
    expect(rows).toHaveLength(1)
    expect(rows[0].newInstalls).toBe(1)
  })
})

describe('mergeCountryRows D28', () => {
  it('is null on every row while the worker sends no D28 at all', () => {
    const rows = mergeCountryRows(new Map([['SE', 3]]), null, [workerRow('DE', 1)])
    expect(rows.map((r) => [r.d28Cohort, r.d28Retained])).toEqual([
      [null, null],
      [null, null],
    ])
  })

  it('passes D28 through once the worker sends it, and is 0 for a country it has no row for', () => {
    const rows = mergeCountryRows(new Map([['SE', 3]]), null, [{ ...workerRow('DE', 4), d28Cohort: 4, d28Retained: 2 }])
    const byCountry = new Map(rows.map((r) => [r.country, r]))
    expect(byCountry.get('DE')).toMatchObject({ d28Cohort: 4, d28Retained: 2 })
    expect(byCountry.get('SE')).toMatchObject({ d28Cohort: 0, d28Retained: 0 })
  })
})

describe('chipCountries', () => {
  it('takes the top real countries by visitors, skipping (unknown)', () => {
    const rows = mergeCountryRows(
      new Map([
        ['US', 50],
        ['SE', 10],
        ['HU', 5],
      ]),
      null,
      [workerRow('(unknown)', 99)],
    )
    expect(chipCountries(rows, 2)).toEqual(['US', 'SE'])
  })
})

describe('safeRatio', () => {
  it('is null for an unknown side or a zero denominator', () => {
    expect(safeRatio(1, 4)).toBe(0.25)
    expect(safeRatio(1, 0)).toBeNull()
    expect(safeRatio(null, 4)).toBeNull()
  })
})
