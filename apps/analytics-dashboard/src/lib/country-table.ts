/**
 * Client-safe shaping for the "By country" table: the top countries, one "Other" row summing the rest,
 * and the worker's "(unknown)" bucket kept apart. Rows arrive ranked from `mergeCountryRows`.
 */
import { type CountryFunnelRow, unknownCountry } from './acquisition-countries.js'

/** The "Other" row: the summed columns plus how many countries it folds. */
export interface OtherCountriesRow extends Omit<CountryFunnelRow, 'country'> {
  countryCount: number
}

export interface CountryTable {
  top: CountryFunnelRow[]
  other: OtherCountriesRow | null
  /** The worker's bucket for downloads and installs with no usable country; never ranked or folded. */
  unknown: CountryFunnelRow | null
}

/** A sum that's unknown (`null`) as soon as any part is: a partial sum would pass for the whole. */
export function sumNullable(values: (number | null)[]): number | null {
  let total = 0
  for (const v of values) {
    if (v === null) return null
    total += v
  }
  return total
}

/** Whether a row has anything to show at all in the window. */
function hasActivity(r: CountryFunnelRow): boolean {
  return (r.visitors ?? 0) > 0 || (r.serverDownloads ?? 0) > 0 || (r.newInstalls ?? 0) > 0
}

export function shapeCountryTable(rows: CountryFunnelRow[], topCount: number): CountryTable {
  const active = rows.filter(hasActivity)
  const unknown = active.find((r) => r.country === unknownCountry) ?? null
  const real = active.filter((r) => r.country !== unknownCountry)
  const rest = real.slice(topCount)
  const sum = (pick: (r: CountryFunnelRow) => number | null) => sumNullable(rest.map(pick))
  const other: OtherCountriesRow | null =
    rest.length === 0
      ? null
      : {
          countryCount: rest.length,
          visitors: sum((r) => r.visitors),
          clicks: sum((r) => r.clicks),
          serverDownloads: sum((r) => r.serverDownloads),
          newInstalls: sum((r) => r.newInstalls),
          d7Cohort: sum((r) => r.d7Cohort),
          d7Retained: sum((r) => r.d7Retained),
          d28Cohort: sum((r) => r.d28Cohort),
          d28Retained: sum((r) => r.d28Retained),
        }
  return { top: real.slice(0, topCount), other, unknown }
}
