import { describe, it, expect } from 'vitest'
import type { CountryFunnelRow } from './acquisition-countries.js'
import { shapeCountryTable, sumNullable } from './country-table.js'

const row = (country: string, visitors: number | null, extra: Partial<CountryFunnelRow> = {}): CountryFunnelRow => ({
  country,
  visitors,
  clicks: null,
  serverDownloads: 1,
  newInstalls: 1,
  d7Cohort: 1,
  d7Retained: 0,
  d28Cohort: null,
  d28Retained: null,
  ...extra,
})

describe('sumNullable', () => {
  it('sums, and turns unknown as soon as one part is unknown', () => {
    expect(sumNullable([1, 2, 3])).toBe(6)
    expect(sumNullable([1, null])).toBeNull()
    expect(sumNullable([])).toBe(0)
  })
})

describe('shapeCountryTable', () => {
  it('keeps the top countries and folds the rest into one Other row', () => {
    const rows = ['US', 'SE', 'HU', 'DE', 'FR'].map((c, i) => row(c, 50 - i * 10, { serverDownloads: i }))
    const table = shapeCountryTable(rows, 2)
    expect(table.top.map((r) => r.country)).toEqual(['US', 'SE'])
    expect(table.other).toMatchObject({ countryCount: 3, visitors: 30 + 20 + 10 })
    expect(table.other?.serverDownloads).toBe(2 + 3 + 4)
    expect(table.unknown).toBeNull()
  })

  it('has no Other row when everything fits', () => {
    expect(shapeCountryTable([row('US', 5)], 20).other).toBeNull()
  })

  it('keeps (unknown) out of the ranking and out of Other, as its own row', () => {
    const unknown = row('(unknown)', 0, { newInstalls: 9 })
    const table = shapeCountryTable([row('US', 5), unknown, row('SE', 3), row('HU', 1)], 1)
    expect(table.top.map((r) => r.country)).toEqual(['US'])
    expect(table.other?.countryCount).toBe(2)
    expect(table.unknown).toBe(unknown)
  })

  it('drops countries with nothing at all in the window', () => {
    const table = shapeCountryTable([row('US', 5), row('SE', 0, { serverDownloads: 0, newInstalls: 0 })], 20)
    expect(table.top.map((r) => r.country)).toEqual(['US'])
  })

  it('leaves an Other column unknown when any folded country is unknown there', () => {
    const table = shapeCountryTable([row('US', 5, { clicks: 2 }), row('SE', 3, { clicks: 1 }), row('HU', 2)], 1)
    expect(table.other?.clicks).toBeNull()
    expect(table.other?.d28Cohort).toBeNull()
  })

  it('sums Other clicks once every folded country has a count', () => {
    const table = shapeCountryTable(
      [row('US', 5, { clicks: 2 }), row('SE', 3, { clicks: 1 }), row('HU', 2, { clicks: 0 })],
      1,
    )
    expect(table.other?.clicks).toBe(1)
  })
})
