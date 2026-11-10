import type { SourceResult } from '../types.js'
import type {
  CountrySlice,
  DownloadsBySource,
  InstallsByChannel,
  RetentionSoFar,
  UaFamilyCounts,
  WorkerCountryDay,
} from '../../funnel.js'
import { unknownCountry } from '../../acquisition-countries.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerEndpoint } from './worker-endpoint.js'
import {
  authenticateUmami,
  fetchUmamiDailyEventSeries,
  fetchUmamiDailyTraffic,
  fetchUmamiVisitorsByCountry,
} from './umami.js'
import { fetchPaddlePurchasesByDay } from './paddle.js'

/**
 * The per-UTC-day funnel behind the Acquisition page's trend charts and download breakdown pies: web-side
 * signals (Umami) joined with server-side telemetry (the api-server `/admin/funnel` endpoint) and purchases
 * (Paddle). Independent of the page's range picker. It carries the totals plus a per-country slice for the
 * top countries, so the page switches countries in the browser without asking the server again. Every
 * cell is either a real number or `null` ("couldn't get this"), so a blank is never confused with a zero.
 */

/**
 * How many UTC days we fetch: 180 complete days plus today (partial), so the 90-day stats column has a
 * full previous 90 days to compare against.
 */
export const funnelFetchDays = 181

/**
 * The most an older worker accepts (`?days=` capped at 90 before the trend charts). We retry at this when
 * the full fetch is refused, so the charts still draw; the 90-day "change" then shows a dash.
 */
const legacyWorkerMaxDays = 90

/**
 * How many countries get their own slice and chip: the top ones by 90-day visitors. At eight, "Other" was
 * over half of most days, so it's 20; `#lib/country-colors.ts` colors a few more than this, so a country
 * climbing into the top 20 usually already has its color. Each costs two Umami calls on a cold load.
 */
export const topCountryCount = 20

/** The window the top countries are ranked over. */
const rankingDays = 90

/** One UTC day of the funnel. `null` anywhere means "unknown", rendered as a dash or "Not available yet". */
export interface FunnelRow {
  /** UTC day, `YYYY-MM-DD`. */
  date: string
  /** Unique visitors to getcmdr.com that day (Umami sessions). `null` when Umami is unavailable. */
  visitors: number | null
  /** Pageviews on getcmdr.com that day (Umami). `null` when Umami is unavailable. */
  pageviews: number | null
  /** `download` button click events on getcmdr.com that day (Umami). `null` when Umami is unavailable. */
  downloadClicks: number | null
  /** Server-side DMG downloads logged that day (api-server). `null` when the worker is unavailable. */
  serverDownloads: number | null
  /** That day's server downloads by `source` (website / Homebrew / other). `null` on an older worker. */
  downloadsBySource: DownloadsBySource | null
  /**
   * That day's server downloads split by first-touch channel (`ref`): a map of ref value -> count,
   * with no-ref downloads under `"(none)"`. `null` when the worker is unavailable (or for one country,
   * which the worker doesn't split this way), `{}` for a present worker source on a day with no downloads.
   */
  downloadsByRef: Record<string, number> | null
  /**
   * That day's server downloads split by the `Referer` host of the `/download` hit: a map of host ->
   * count, illuminating the `(none)` first-touch bucket (direct links carry a referer but no `ref`).
   * `null` when the worker is unavailable (or for one country), `{}` on a day with no downloads.
   */
  downloadsByReferer: Record<string, number> | null
  /**
   * That day's server downloads split by User-Agent family (`human` / `bot` / `unknown`), as the worker
   * classifies them. Cmdr is macOS-only, so `bot` (a non-macOS UA) can't be a real install. `null` when
   * the worker is unavailable (or for one country), all-zeros on a day with no downloads.
   */
  downloadsByUaFamily: UaFamilyCounts | null
  /**
   * That day's downloads minus the provably-impossible `bot` (non-macOS UA) hits (`human + unknown`). A
   * conservative install signal next to the raw `serverDownloads`. `null` when the worker is unavailable.
   */
  humanInstalls: number | null
  /** Installs whose first-ever heartbeat landed that day (api-server). `null` when the worker is unavailable. */
  newInstalls: number | null
  /** That day's new installs by first-beat channel. `null` on an older worker. */
  newInstallsByChannel: InstallsByChannel | null
  /** D7 retention fraction (0..1) for this cohort, or `null` when too young / worker unavailable. */
  d7Retention: number | null
  /** Raw count behind `d7Retention`, or `null`. */
  d7Retained: number | null
  /** A cohort too young for D7: retained so far and as of which day. `null` otherwise or on an older worker. */
  retentionSoFar: RetentionSoFar | null
  /** D28 retention fraction (0..1), or `null` until the cohort is 29 days old (or on an older worker). */
  d28Retention: number | null
  /** Raw count behind `d28Retention`, or `null`. */
  d28Retained: number | null
  /** A cohort 8–28 days old: retained so far toward D28. `null` otherwise or on an older worker. */
  d28SoFar: RetentionSoFar | null
  /** Newsletter + beta signups that day (Listmonk via api-server). `null` when unavailable or country-scoped. */
  newsletterSignups: number | null
  /** Completed Paddle transactions that day. `null` when Paddle is unavailable or the view is country-scoped. */
  purchases: number | null
  /**
   * The per-country slices for `FunnelData.countries.top` (and `extra`), sparse: a country with nothing that
   * day is absent. Totals rows carry it; a row derived for one country carries `{}`.
   */
  byCountry: Record<string, CountrySlice>
}

/** Which countries carry a slice, and which sides could answer per country. */
export interface FunnelCountries {
  /**
   * The top countries by 90-day getcmdr.com visitors (by 90-day worker installs when Umami is down),
   * biggest first, never `(unknown)`. Their colors are fixed in `#lib/country-colors.ts`.
   */
  top: string[]
  /** A `?country=` outside `top` that this load also carries, so a link to it still works. */
  extra: string | null
  /** Whether Umami answered per country (visitors, pageviews, and clicks for `top` and `extra`). */
  umami: boolean
  /** Whether the worker sends per-country facts (`byCountry`); an older worker doesn't. */
  worker: boolean
}

export interface FunnelData {
  /** Oldest first, ending today (partial). Up to `funnelFetchDays` long; shorter if a source capped it. */
  rows: FunnelRow[]
  countries: FunnelCountries
  /** `unavailable` when the worker failed, so every worker column is `null`. */
  workerStatus: 'ok' | 'unavailable'
}

/** The api-server `/admin/funnel` response shape (a subset of its `FunnelDay`). */
export interface WorkerFunnelDay {
  date: string
  downloads: number
  downloadsByRef: Record<string, number>
  // Optional so the dashboard still maps a response from a worker deployed before migration 0010.
  downloadsByReferer?: Record<string, number>
  // Optional so the dashboard still maps a response from a worker deployed before the UA-family read side.
  downloadsByUaFamily?: UaFamilyCounts
  humanInstalls?: number
  // Optional: the trend-chart fields, absent on a worker deployed before them.
  downloadsBySource?: DownloadsBySource
  newInstallsByChannel?: InstallsByChannel
  retentionSoFar?: RetentionSoFar | null
  // Optional: D28 and the per-country split, absent on a worker deployed before them.
  d28Retained?: number | null
  d28Retention?: number | null
  d28SoFar?: RetentionSoFar | null
  byCountry?: Record<string, WorkerCountryDay>
  newInstalls: number
  d7Retention: number | null
  d7Retained: number | null
  newsletterSignups: number | null
}

// The ranked-channel helper and its type are client-safe (the page renders them), so they live in
// `#lib/funnel.ts` outside `#lib/server`. Re-exported here so server-side callers and the existing
// tests can keep importing from this module.
export { aggregateChannels, aggregateReferers, aggregateUaFamilies, type UaFamilyCounts } from '../../funnel.js'

interface FunnelEnv {
  LICENSE_SERVER_ADMIN_TOKEN: string
  WORKER_BASE_URL?: string
  UMAMI_API_URL: string
  UMAMI_USERNAME: string
  UMAMI_PASSWORD: string
  UMAMI_WEBSITE_ID: string
  PADDLE_API_KEY_LIVE: string
}

/** UTC `YYYY-MM-DD` strings for the last `days` days, oldest first, ending today (UTC). */
export function buildFunnelDateList(days: number, now: Date): string[] {
  const todayUtc = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())
  const dates: string[] = []
  for (let i = days - 1; i >= 0; i--) {
    dates.push(new Date(todayUtc - i * 86_400_000).toISOString().slice(0, 10))
  }
  return dates
}

/** One day of a single-column source: a present source with no entry that day is a real 0. */
function dailyCount(byDay: Map<string, number> | null, date: string): number | null {
  return byDay ? (byDay.get(date) ?? 0) : null
}

/** The columns the worker funnel owns. Split out so each source's null handling stays readable. */
type WorkerColumns = Pick<
  FunnelRow,
  | 'serverDownloads'
  | 'downloadsBySource'
  | 'downloadsByRef'
  | 'downloadsByReferer'
  | 'downloadsByUaFamily'
  | 'humanInstalls'
  | 'newInstalls'
  | 'newInstallsByChannel'
  | 'd7Retention'
  | 'd7Retained'
  | 'retentionSoFar'
  | 'd28Retention'
  | 'd28Retained'
  | 'd28SoFar'
  | 'newsletterSignups'
>

const unknownWorkerColumns: WorkerColumns = {
  serverDownloads: null,
  downloadsBySource: null,
  downloadsByRef: null,
  downloadsByReferer: null,
  downloadsByUaFamily: null,
  humanInstalls: null,
  newInstalls: null,
  newInstallsByChannel: null,
  d7Retention: null,
  d7Retained: null,
  retentionSoFar: null,
  d28Retention: null,
  d28Retained: null,
  d28SoFar: null,
  newsletterSignups: null,
}

/** The worker's view of what it answered: the days it covered, and which optional fields it carries. */
export interface WorkerFunnel {
  byDay: Map<string, WorkerFunnelDay>
  /** The oldest day the worker answered for; earlier days are unknown, not zero. */
  since: string
  /** Whether this worker carries the trend-chart fields. */
  hasTrendFields: boolean
  /** Whether this worker carries the per-country split (`byCountry`). */
  hasCountries: boolean
  /** Whether this worker carries D28 (`d28Retained`, `d28SoFar`). */
  hasD28: boolean
}

/**
 * One day's worker-sourced columns. A `null` source (or a day before the worker's window) makes every one
 * of them unknown; a present source with no row that day is a real empty day (0 / `{}`). D7, D28, and
 * signups stay `null` even on a present row: a young cohort or Listmonk being down is genuinely unknown,
 * not zero. The optional fields stay `null` on an older worker that doesn't send them.
 */
function workerColumns(worker: WorkerFunnel | null, date: string): WorkerColumns {
  if (!worker || date < worker.since) return unknownWorkerColumns
  const trend = worker.hasTrendFields

  const day = worker.byDay.get(date)
  if (!day) {
    return {
      ...unknownWorkerColumns,
      serverDownloads: 0,
      downloadsBySource: trend ? { website: 0, homebrew: 0, other: 0 } : null,
      downloadsByRef: {},
      downloadsByReferer: {},
      downloadsByUaFamily: { human: 0, bot: 0, unknown: 0 },
      humanInstalls: 0,
      newInstalls: 0,
      newInstallsByChannel: trend ? { homebrew: 0, direct: 0, unknown: 0 } : null,
    }
  }

  // The `??` defaults cover a worker deployed before these fields existed.
  return {
    serverDownloads: day.downloads,
    downloadsBySource: day.downloadsBySource ?? null,
    downloadsByRef: day.downloadsByRef,
    downloadsByReferer: day.downloadsByReferer ?? {},
    downloadsByUaFamily: day.downloadsByUaFamily ?? { human: 0, bot: 0, unknown: 0 },
    humanInstalls: day.humanInstalls ?? 0,
    newInstalls: day.newInstalls,
    newInstallsByChannel: day.newInstallsByChannel ?? null,
    d7Retention: day.d7Retention,
    d7Retained: day.d7Retained,
    retentionSoFar: day.retentionSoFar ?? null,
    d28Retention: day.d28Retention ?? null,
    d28Retained: day.d28Retained ?? null,
    d28SoFar: day.d28SoFar ?? null,
    newsletterSignups: day.newsletterSignups,
  }
}

/** One country's Umami daily series. */
export interface UmamiCountrySeries {
  visitors: Map<string, number>
  pageviews: Map<string, number>
  clicks: Map<string, number>
}

/** What Umami answered: the totals, the ranked top countries (`null` if ranking failed), and their series. */
export interface UmamiFunnel {
  totals: UmamiCountrySeries
  top: string[] | null
  byCountry: Map<string, UmamiCountrySeries> | null
}

/** The per-day inputs `assembleFunnelRows` joins. `null` means that whole source failed. */
export interface FunnelInputs {
  worker: WorkerFunnel | null
  visitors: Map<string, number> | null
  pageviews: Map<string, number> | null
  clicks: Map<string, number> | null
  purchases: Map<string, number> | null
  /** The countries that get a slice. Defaults to none. */
  countries?: string[]
  /** Umami's series per country in `countries`, or `null` when Umami couldn't answer per country. */
  countryUmami?: Map<string, UmamiCountrySeries> | null
}

/** One day's sparse per-country slices for `codes`. */
export function countrySlices(
  codes: string[],
  date: string,
  workerDay: WorkerFunnelDay | undefined,
  umami: Map<string, UmamiCountrySeries> | null | undefined,
): Record<string, CountrySlice> {
  const out: Record<string, CountrySlice> = {}
  for (const code of codes) {
    const slice: CountrySlice = {}
    const series = umami?.get(code)
    const visitors = series?.visitors.get(date)
    const pageviews = series?.pageviews.get(date)
    const clicks = series?.clicks.get(date)
    if (visitors !== undefined) slice.visitors = visitors
    if (pageviews !== undefined) slice.pageviews = pageviews
    if (clicks !== undefined) slice.downloadClicks = clicks
    const worker = workerDay?.byCountry?.[code]
    if (worker) slice.worker = worker
    if (Object.keys(slice).length > 0) out[code] = slice
  }
  return out
}

/**
 * Merge the per-day inputs into one row array over the canonical `dates` list. Pure for testing. A missing
 * day inside a present source is a real 0, while a `null` source makes that whole column `null` (a dash).
 * The worker funnel is the one source that carries multiple columns; its per-column `null`s (young D7
 * cohorts, fields an older worker lacks) pass straight through.
 */
export function assembleFunnelRows(dates: string[], inputs: FunnelInputs): FunnelRow[] {
  return dates.map((date) => ({
    date,
    visitors: dailyCount(inputs.visitors, date),
    pageviews: dailyCount(inputs.pageviews, date),
    downloadClicks: dailyCount(inputs.clicks, date),
    ...workerColumns(inputs.worker, date),
    purchases: dailyCount(inputs.purchases, date),
    byCountry: countrySlices(inputs.countries ?? [], date, inputs.worker?.byDay.get(date), inputs.countryUmami),
  }))
}

/** Index a worker response by day, recording what it covers. Pure for testing. */
export function indexWorkerFunnel(rows: WorkerFunnelDay[], requestedSince: string): WorkerFunnel {
  return {
    byDay: new Map(rows.map((r) => [r.date, r])),
    since: rows.length > 0 && rows[0].date > requestedSince ? rows[0].date : requestedSince,
    hasTrendFields: rows.some((r) => r.newInstallsByChannel !== undefined),
    hasCountries: rows.some((r) => r.byCountry !== undefined),
    hasD28: rows.some((r) => r.d28SoFar !== undefined || r.d28Retained !== undefined),
  }
}

/**
 * The top `count` countries, biggest first, never `(unknown)`: by Umami's visitors when it answered, else by
 * the worker's installs (then downloads) over the days from `since`. Pure for testing.
 */
export function rankTopCountries(
  visitors: Map<string, number> | null,
  worker: WorkerFunnel | null,
  since: string,
  count = topCountryCount,
): string[] {
  const score = new Map<string, number>()
  if (visitors) {
    for (const [code, n] of visitors) score.set(code.toUpperCase(), n)
  } else if (worker?.hasCountries) {
    for (const [date, day] of worker.byDay) {
      if (date < since) continue
      for (const [code, c] of Object.entries(day.byCountry ?? {})) {
        // Installs dominate; downloads break ties between countries with equal installs.
        score.set(code, (score.get(code) ?? 0) + c.newInstalls * 1_000_000 + c.serverDownloads)
      }
    }
  }
  return [...score.entries()]
    .filter(([code, n]) => code !== unknownCountry && /^[A-Z]{2}$/.test(code) && n > 0)
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, count)
    .map(([code]) => code)
}

/**
 * The funnel for every country plus the slices for the top countries, and for `country` too when it's
 * outside them (a link from the country table). The base is cached once for every country; the extra
 * country's slices are cached on their own, so switching chips never refetches.
 */
export async function fetchFunnelData(env: FunnelEnv, country: string | null): Promise<SourceResult<FunnelData>> {
  const base = await fetchFunnelBase(env)
  if (!base.ok || !country || base.data.countries.top.includes(country)) return base
  const extra = await fetchExtraCountry(env, country).catch(() => null)
  if (!extra) return base
  const rows = base.data.rows.map((row) => {
    const slices = extra.get(row.date)
    return slices ? { ...row, byCountry: { ...row.byCountry, ...slices } } : row
  })
  return { ok: true, data: { ...base.data, rows, countries: { ...base.data.countries, extra: country } } }
}

async function fetchFunnelBase(env: FunnelEnv): Promise<SourceResult<FunnelData>> {
  const cacheKey = `days:${String(funnelFetchDays)}`
  const cached = await cacheGet<FunnelData>('funnel-v3', cacheKey)
  if (cached) return { ok: true, data: cached }

  const now = new Date()
  const dates = buildFunnelDateList(funnelFetchDays, now)
  const sinceDate = dates[0]
  const rankingSince = dates[dates.length - rankingDays]

  // Each side is best-effort and independent: one failing degrades its columns, never the whole page.
  const [worker, umami, purchases] = await Promise.all([
    fetchWorkerFunnel(env, now).catch(() => null),
    fetchUmamiFunnel(env, sinceDate, rankingSince, now, (visitors) =>
      // Umami's own ranking decides the top countries; the worker can only stand in when Umami is down.
      rankTopCountries(visitors, null, rankingSince),
    ).catch(() => null),
    fetchPaddlePurchasesByDay({ PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE }, sinceDate).catch(() => null),
  ])

  // If literally everything failed, surface an error so the section shows "Couldn't load" rather than
  // charts of nothing that look like a real empty period.
  if (!worker && !umami && !purchases) {
    return { ok: false, error: 'Funnel: every source failed (worker, Umami, and Paddle)' }
  }

  const data = assembleFunnelData(dates, rankingSince, worker, umami, purchases)
  await cacheSet('funnel-v3', cacheKey, data)
  return { ok: true, data }
}

/** Join what each side answered into the page's `FunnelData`. Pure for testing. */
export function assembleFunnelData(
  dates: string[],
  rankingSince: string,
  worker: WorkerFunnel | null,
  umami: UmamiFunnel | null,
  purchases: Map<string, number> | null,
): FunnelData {
  const top = umami?.top ?? rankTopCountries(null, worker, rankingSince)
  const rows = assembleFunnelRows(dates, {
    worker,
    visitors: umami?.totals.visitors ?? null,
    pageviews: umami?.totals.pageviews ?? null,
    clicks: umami?.totals.clicks ?? null,
    purchases,
    countries: top,
    countryUmami: umami?.byCountry ?? null,
  })
  return {
    rows,
    countries: { top, extra: null, umami: Boolean(umami?.byCountry), worker: worker?.hasCountries ?? false },
    workerStatus: worker ? 'ok' : 'unavailable',
  }
}

/**
 * One country outside the top ones, by day: its worker slice and its Umami series. Throws when the worker
 * fails; Umami failing leaves that side out (the client then shows its columns as unknown).
 */
async function fetchExtraCountry(env: FunnelEnv, country: string): Promise<Map<string, Record<string, CountrySlice>>> {
  type Cached = [string, Record<string, CountrySlice>][]
  const cached = await cacheGet<Cached>('funnel-extra-v1', `days:${String(funnelFetchDays)}`, `country=${country}`)
  if (cached) return new Map(cached)

  const now = new Date()
  const dates = buildFunnelDateList(funnelFetchDays, now)
  const [worker, umami] = await Promise.all([
    fetchWorkerFunnel(env, now),
    (async () => {
      const token = await authenticateUmami(env.UMAMI_API_URL, env.UMAMI_USERNAME, env.UMAMI_PASSWORD)
      const series = await fetchUmamiCountrySeries(env, token, window(dates[0], now), country)
      return new Map([[country, series]])
    })().catch(() => null),
  ])
  const byDay: Cached = dates.map((date) => [date, countrySlices([country], date, worker.byDay.get(date), umami)])
  await cacheSet('funnel-extra-v1', `days:${String(funnelFetchDays)}`, byDay, `country=${country}`)
  return new Map(byDay)
}

/**
 * Fetch the api-server funnel and index it by day. Asks for the full window first and falls back to the
 * older worker's 90-day cap if that's refused. Throws when both fail (caller catches).
 */
async function fetchWorkerFunnel(env: FunnelEnv, now: Date): Promise<WorkerFunnel> {
  const get = (days: number) =>
    fetchWorkerEndpoint<WorkerFunnelDay[]>(
      env.LICENSE_SERVER_ADMIN_TOKEN,
      `/admin/funnel?days=${String(days)}`,
      env.WORKER_BASE_URL,
    )
  try {
    return indexWorkerFunnel(await get(funnelFetchDays), buildFunnelDateList(funnelFetchDays, now)[0])
  } catch {
    return indexWorkerFunnel(await get(legacyWorkerMaxDays), buildFunnelDateList(legacyWorkerMaxDays, now)[0])
  }
}

/** From the start of `sinceDate` (UTC) to now. */
function window(sinceDate: string, now: Date): { startAt: number; endAt: number } {
  return { startAt: Date.parse(`${sinceDate}T00:00:00Z`), endAt: now.getTime() }
}

/** One country's daily visitors, pageviews, and `download` clicks: two Umami calls. */
async function fetchUmamiCountrySeries(
  env: FunnelEnv,
  token: string,
  span: { startAt: number; endAt: number },
  country: string,
): Promise<UmamiCountrySeries> {
  const scoped = { ...span, country }
  const [traffic, clicks] = await Promise.all([
    fetchUmamiDailyTraffic(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, scoped),
    fetchUmamiDailyEventSeries(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, scoped, 'download'),
  ])
  return { ...traffic, clicks }
}

/**
 * getcmdr.com's per-day visitors, pageviews, and `download` clicks from Umami, in one auth: the totals, the
 * 90-day country ranking, and the same daily series for each top country (Umami filters every endpoint by
 * `country=`). The totals are required (throws, caller catches); the per-country half is best-effort, so a
 * failing country call leaves `byCountry` `null` rather than taking the totals down with it.
 */
async function fetchUmamiFunnel(
  env: FunnelEnv,
  sinceDate: string,
  rankingSince: string,
  now: Date,
  rank: (visitors: Map<string, number>) => string[],
): Promise<UmamiFunnel> {
  const token = await authenticateUmami(env.UMAMI_API_URL, env.UMAMI_USERNAME, env.UMAMI_PASSWORD)
  const span = window(sinceDate, now)
  const ranking = window(rankingSince, now)
  const [traffic, clicks, top] = await Promise.all([
    fetchUmamiDailyTraffic(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, { ...span, country: null }),
    fetchUmamiDailyEventSeries(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, { ...span, country: null }, 'download'),
    fetchUmamiVisitorsByCountry(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, ranking.startAt, ranking.endAt)
      .then(rank)
      .catch(() => null),
  ])
  const byCountry = top
    ? await Promise.all(top.map((code) => fetchUmamiCountrySeries(env, token, span, code)))
        .then((series) => new Map(top.map((code, i) => [code, series[i]])))
        .catch(() => null)
    : null
  return { totals: { ...traffic, clicks }, top, byCountry }
}
