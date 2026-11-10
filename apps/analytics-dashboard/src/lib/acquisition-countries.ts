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
  clicks: number | null
  serverDownloads: number | null
  newInstalls: number | null
  d7Cohort: number | null
  d7Retained: number | null
  /** `null` also while the worker doesn't send D28 yet. */
  d28Cohort: number | null
  d28Retained: number | null
}

interface WorkerCountry {
  country: string
  serverDownloads: number
  newInstalls: number
  d7Cohort: number
  d7Retained: number
  /** Optional: older workers don't send D28. */
  d28Cohort?: number
  d28Retained?: number
}

/**
 * Join Umami's per-country visitors and clicks with the worker's per-country rows, ranked by visitors then
 * installs. A side that failed (`null`) leaves its columns `null`; a present side with no entry for a country
 * is a real 0. Clicks cover every country (`(unknown)` holds the ones Umami geolocated nowhere).
 */
export function mergeCountryRows(
  visitors: Map<string, number> | null,
  clicks: Map<string, number> | null,
  worker: WorkerCountry[] | null,
): CountryFunnelRow[] {
  const normalize = (code: string) => (code === unknownCountry ? code : code.toUpperCase())
  const workerByCountry = new Map((worker ?? []).map((w) => [normalize(w.country), w]))
  const workerHasD28 = (worker ?? []).some((w) => w.d28Cohort !== undefined)
  const countries = new Set([...(visitors?.keys() ?? []), ...workerByCountry.keys()].map(normalize))

  const rows = [...countries].map((country): CountryFunnelRow => {
    const w = workerByCountry.get(country)
    const fromWorker = (pick: (w: WorkerCountry) => number) => (worker ? (w ? pick(w) : 0) : null)
    // D28 is newer than the rest: a worker that sends it on no row hasn't shipped it, so it's unknown, not 0.
    const fromD28 = (pick: (w: WorkerCountry) => number | undefined) =>
      worker && workerHasD28 ? (w ? (pick(w) ?? 0) : 0) : null
    return {
      country,
      visitors: visitors ? (visitors.get(country) ?? 0) : null,
      clicks: clicks ? (clicks.get(country) ?? 0) : null,
      serverDownloads: fromWorker((r) => r.serverDownloads),
      newInstalls: fromWorker((r) => r.newInstalls),
      d7Cohort: fromWorker((r) => r.d7Cohort),
      d7Retained: fromWorker((r) => r.d7Retained),
      d28Cohort: fromD28((r) => r.d28Cohort),
      d28Retained: fromD28((r) => r.d28Retained),
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
