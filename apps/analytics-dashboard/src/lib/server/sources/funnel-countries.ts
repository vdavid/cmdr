import type { SourceResult } from '../types.js'
import type { CountryFunnelRow } from '../../acquisition-countries.js'
import { mergeCountryRows } from '../../acquisition-countries.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerResponse } from './worker-endpoint.js'
import { authenticateUmami, fetchUmamiEventCountsByCountry, fetchUmamiVisitorsByCountry } from './umami.js'

/**
 * The per-country funnel under the Acquisition charts: the map and the "By country" table, over the picker's
 * window. Joins Umami's per-country visitors and clicks with the worker's `/admin/funnel/countries`
 * (downloads, installs, pooled D7 and D28). Each side is best-effort: one missing leaves its columns `null`,
 * shown as a dash.
 */

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
  // Optional: absent on a worker deployed before D28.
  d28Cohort?: number
  d28Retained?: number
}

interface CountriesEnv {
  LICENSE_SERVER_ADMIN_TOKEN: string
  WORKER_BASE_URL?: string
  UMAMI_API_URL: string
  UMAMI_USERNAME: string
  UMAMI_PASSWORD: string
  UMAMI_WEBSITE_ID: string
}

/**
 * The funnel by country over the `days` UTC days ending `end` (inclusive), or over the last `days` with
 * today (partial) included when `end` is null, as the worker's `?days=N&end=`.
 */
export async function fetchFunnelCountriesData(
  env: CountriesEnv,
  days: number,
  end: string | null = null,
): Promise<SourceResult<FunnelCountriesData>> {
  const cacheKey = end ? `days:${String(days)}:end:${end}` : `days:${String(days)}`
  const cached = await cacheGet<FunnelCountriesData>('funnel-countries-v2', cacheKey)
  if (cached) return { ok: true, data: cached }

  const [umami, worker] = await Promise.all([
    fetchUmamiCountries(env, days, end).catch(() => null),
    fetchWorkerCountries(env, days, end).catch(() => null),
  ])
  if (!umami && !worker) {
    return { ok: false, error: 'Countries: both Umami and the worker failed' }
  }

  const data: FunnelCountriesData = {
    rows: mergeCountryRows(umami?.visitors ?? null, umami?.clicks ?? null, worker),
    umamiAvailable: umami !== null,
    workerAvailable: worker !== null,
  }
  await cacheSet('funnel-countries-v2', cacheKey, data)
  return { ok: true, data }
}

/**
 * The worker's per-country rows. For a past window, only when the `X-Window` header confirms the worker
 * answered those days: an older worker ignores `end` and would hand back today's window under a past
 * title, so that throws (the caller reads it as "worker unavailable", a dash, never a wrong number).
 */
async function fetchWorkerCountries(env: CountriesEnv, days: number, end: string | null): Promise<WorkerCountryRow[]> {
  const query = end ? `days=${String(days)}&end=${end}` : `days=${String(days)}`
  const { body, headers } = await fetchWorkerResponse<WorkerCountryRow[]>(
    env.LICENSE_SERVER_ADMIN_TOKEN,
    `/admin/funnel/countries?${query}`,
    env.WORKER_BASE_URL,
  )
  if (end && !headers.get('X-Window')?.endsWith(`..${end}`)) {
    throw new Error(`Countries: the worker didn't confirm the window ending ${end}`)
  }
  return body
}

/** Per-country visitors and download clicks for every country, in two parallel calls. Throws on any Umami error. */
async function fetchUmamiCountries(
  env: CountriesEnv,
  days: number,
  end: string | null,
): Promise<{ visitors: Map<string, number>; clicks: Map<string, number> }> {
  const token = await authenticateUmami(env.UMAMI_API_URL, env.UMAMI_USERNAME, env.UMAMI_PASSWORD)
  const now = new Date()
  // A past window runs from its first day's midnight to the last millisecond of `end`.
  const last = end
    ? Date.parse(`${end}T00:00:00Z`)
    : Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())
  const startAt = last - (days - 1) * 86_400_000
  const endAt = end ? last + 86_400_000 - 1 : now.getTime()
  const [visitors, clicks] = await Promise.all([
    fetchUmamiVisitorsByCountry(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt),
    fetchUmamiEventCountsByCountry(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, { startAt, endAt }, 'download'),
  ])
  return { visitors, clicks }
}
