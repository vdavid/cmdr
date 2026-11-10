import type { SourceResult } from '../types.js'
import type { CountryFunnelRow } from '../../acquisition-countries.js'
import { mergeCountryRows } from '../../acquisition-countries.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerEndpoint } from './worker-endpoint.js'
import { authenticateUmami, fetchUmamiEventCountForCountry, fetchUmamiVisitorsByCountry } from './umami.js'

/**
 * The per-country 30-day funnel under the Acquisition charts, and the country chips above them. Joins
 * Umami's per-country visitors and clicks with the worker's `/admin/funnel/countries` (downloads, installs,
 * pooled D7). Each side is best-effort: one missing leaves its columns `null`, shown as a dash.
 */

/** The table's window, including today (partial), matching the worker's `?days=N` (N days ending today). */
export const countryWindowDays = 30

/**
 * How many countries get a per-country click count. Umami can't group events by country in one call, so
 * each one is its own request; past the top visitor countries the numbers are noise anyway.
 */
const clickLookupCountries = 20

export interface FunnelCountriesData {
  rows: CountryFunnelRow[]
  /** Whether each side answered, so the UI can say which columns are "not available yet". */
  umamiAvailable: boolean
  workerAvailable: boolean
}

/** One row of the worker's `/admin/funnel/countries`. */
interface WorkerCountryRow {
  country: string
  serverDownloads: number
  humanInstalls: number
  newInstalls: number
  d7Cohort: number
  d7Retained: number
}

interface CountriesEnv {
  LICENSE_SERVER_ADMIN_TOKEN: string
  WORKER_BASE_URL?: string
  UMAMI_API_URL: string
  UMAMI_USERNAME: string
  UMAMI_PASSWORD: string
  UMAMI_WEBSITE_ID: string
}

export async function fetchFunnelCountriesData(env: CountriesEnv): Promise<SourceResult<FunnelCountriesData>> {
  const cacheKey = `days:${String(countryWindowDays)}`
  const cached = await cacheGet<FunnelCountriesData>('funnel-countries', cacheKey)
  if (cached) return { ok: true, data: cached }

  const [umami, worker] = await Promise.all([
    fetchUmamiCountries(env).catch(() => null),
    fetchWorkerEndpoint<WorkerCountryRow[]>(
      env.LICENSE_SERVER_ADMIN_TOKEN,
      `/admin/funnel/countries?days=${String(countryWindowDays)}`,
      env.WORKER_BASE_URL,
    ).catch(() => null),
  ])
  if (!umami && !worker) {
    return { ok: false, error: 'Countries: both Umami and the worker failed' }
  }

  const data: FunnelCountriesData = {
    rows: mergeCountryRows(umami?.visitors ?? null, umami?.clicks ?? null, worker),
    umamiAvailable: umami !== null,
    workerAvailable: worker !== null,
  }
  await cacheSet('funnel-countries', cacheKey, data)
  return { ok: true, data }
}

/** Per-country visitors, plus clicks for the top visitor countries. Throws on any Umami error. */
async function fetchUmamiCountries(
  env: CountriesEnv,
): Promise<{ visitors: Map<string, number>; clicks: Map<string, number> }> {
  const token = await authenticateUmami(env.UMAMI_API_URL, env.UMAMI_USERNAME, env.UMAMI_PASSWORD)
  const now = new Date()
  const startAt = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() - (countryWindowDays - 1))
  const endAt = now.getTime()
  const visitors = await fetchUmamiVisitorsByCountry(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt)
  const top = [...visitors.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, clickLookupCountries)
    .map(([country]) => country)
  const counts = await Promise.all(
    top.map((country) =>
      fetchUmamiEventCountForCountry(
        env.UMAMI_API_URL,
        token,
        env.UMAMI_WEBSITE_ID,
        { startAt, endAt, country },
        'download',
      ),
    ),
  )
  return { visitors, clicks: new Map(top.map((country, i) => [country, counts[i]])) }
}
