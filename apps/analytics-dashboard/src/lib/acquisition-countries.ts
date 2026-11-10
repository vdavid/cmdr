/**
 * Client-safe shaping for the Acquisition page's per-country table and country chips. The server source
 * (`#lib/server/sources/funnel-countries.ts`) fetches; this joins and ranks, so tests reach it directly.
 */

/** The worker's label for downloads and installs with no usable country. */
export const unknownCountry = '(unknown)'

/** One country's last-30-days funnel. `null` means that side couldn't answer for this country. */
export interface CountryFunnelRow {
  /** ISO-3166 alpha-2, uppercase, or `"(unknown)"`. */
  country: string
  visitors: number | null
  /** `null` also for a country outside the top visitor countries we asked Umami about. */
  clicks: number | null
  serverDownloads: number | null
  newInstalls: number | null
  d7Cohort: number | null
  d7Retained: number | null
}

interface WorkerCountry {
  country: string
  serverDownloads: number
  newInstalls: number
  d7Cohort: number
  d7Retained: number
}

/**
 * Join Umami's per-country visitors and clicks with the worker's per-country rows, ranked by visitors then
 * installs. A side that failed (`null`) leaves its columns `null`; a present side with no entry for a country
 * is a real 0. Clicks are known only for the countries we asked about, so they stay `null` elsewhere.
 */
export function mergeCountryRows(
  visitors: Map<string, number> | null,
  clicks: Map<string, number> | null,
  worker: WorkerCountry[] | null,
): CountryFunnelRow[] {
  const normalize = (code: string) => (code === unknownCountry ? code : code.toUpperCase())
  const workerByCountry = new Map((worker ?? []).map((w) => [normalize(w.country), w]))
  const countries = new Set([...(visitors?.keys() ?? []), ...workerByCountry.keys()].map(normalize))

  const rows = [...countries].map((country): CountryFunnelRow => {
    const w = workerByCountry.get(country)
    const fromWorker = (pick: (w: WorkerCountry) => number) => (worker ? (w ? pick(w) : 0) : null)
    return {
      country,
      visitors: visitors ? (visitors.get(country) ?? 0) : null,
      clicks: clicks?.get(country) ?? null,
      serverDownloads: fromWorker((r) => r.serverDownloads),
      newInstalls: fromWorker((r) => r.newInstalls),
      d7Cohort: fromWorker((r) => r.d7Cohort),
      d7Retained: fromWorker((r) => r.d7Retained),
    }
  })
  return rows.sort((a, b) => (b.visitors ?? 0) - (a.visitors ?? 0) || (b.newInstalls ?? 0) - (a.newInstalls ?? 0))
}

/** The country chips above the charts: the top `count` real countries by visitors (never `(unknown)`). */
export function chipCountries(rows: CountryFunnelRow[], count: number): string[] {
  return rows
    .filter((r) => r.country !== unknownCountry && (r.visitors ?? 0) > 0)
    .slice(0, count)
    .map((r) => r.country)
}

/** `num ÷ den` as a fraction, or `null` when either is unknown or the denominator is zero. */
export function safeRatio(num: number | null, den: number | null): number | null {
  return num === null || den === null || den === 0 ? null : num / den
}
