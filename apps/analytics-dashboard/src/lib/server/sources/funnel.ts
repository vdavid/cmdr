import type { SourceResult } from '../types.js'
import type { DownloadsBySource, InstallsByChannel, RetentionSoFar, UaFamilyCounts } from '../../funnel.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerEndpoint } from './worker-endpoint.js'
import { authenticateUmami, fetchUmamiDailyTraffic, fetchUmamiDailyEventSeries } from './umami.js'
import { fetchPaddlePurchasesByDay } from './paddle.js'

/**
 * The per-UTC-day funnel behind the Acquisition page's trend charts and the old daily table: web-side
 * signals (Umami) joined with server-side telemetry (the api-server `/admin/funnel` endpoint) and purchases
 * (Paddle), optionally scoped to one country. Independent of the page's range picker. Every cell is either a
 * real number or `null` ("couldn't get this"), so a blank is never confused with a real zero.
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
   * with no-ref downloads under `"(none)"`. `null` when the worker is unavailable, `{}` for a present
   * worker source on a day with no downloads.
   */
  downloadsByRef: Record<string, number> | null
  /**
   * That day's server downloads split by the `Referer` host of the `/download` hit: a map of host ->
   * count, illuminating the `(none)` first-touch bucket (direct links carry a referer but no `ref`).
   * `null` when the worker is unavailable, `{}` for a present worker source on a day with no downloads.
   */
  downloadsByReferer: Record<string, number> | null
  /**
   * That day's server downloads split by User-Agent family (`human` / `bot` / `unknown`), as the worker
   * classifies them. Cmdr is macOS-only, so `bot` (a non-macOS UA) can't be a real install. `null` when
   * the worker is unavailable, all-zeros for a present worker source on a day with no downloads.
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
  /** Newsletter + beta signups that day (Listmonk via api-server). `null` when unavailable or country-scoped. */
  newsletterSignups: number | null
  /** Completed Paddle transactions that day. `null` when Paddle is unavailable or the view is country-scoped. */
  purchases: number | null
}

export interface FunnelData {
  /** Oldest first, ending today (partial). Up to `funnelFetchDays` long; shorter if a source capped it. */
  rows: FunnelRow[]
  /** The country this data is scoped to, or `null` for all countries. */
  country: string | null
  /**
   * Why the worker columns are all `null`, when they are: `unavailable` (the worker failed) or
   * `country-unsupported` (an older worker ignores `?country=` and would hand back global numbers).
   */
  workerStatus: 'ok' | 'unavailable' | 'country-unsupported'
}

/** The api-server `/admin/funnel` response shape (a subset of its `FunnelDay`). */
interface WorkerFunnelDay {
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
  newsletterSignups: null,
}

/** The worker's view of what it answered: the days it covered, and whether it carries the new fields. */
interface WorkerFunnel {
  byDay: Map<string, WorkerFunnelDay>
  /** The oldest day the worker answered for; earlier days are unknown, not zero. */
  since: string
  /** Whether this worker carries the trend-chart fields (and so honors `?country=`). */
  hasTrendFields: boolean
}

/**
 * One day's worker-sourced columns. A `null` source (or a day before the worker's window) makes every one
 * of them unknown; a present source with no row that day is a real empty day (0 / `{}`). D7 and signups
 * stay `null` even on a present row: a young cohort or Listmonk being down is genuinely unknown, not zero.
 * The trend-chart fields stay `null` on an older worker that doesn't send them.
 */
function workerColumns(worker: WorkerFunnel | null, date: string): WorkerColumns {
  if (!worker || date < worker.since) return unknownWorkerColumns
  const trend = worker.hasTrendFields

  const day = worker.byDay.get(date)
  if (!day) {
    return {
      serverDownloads: 0,
      downloadsBySource: trend ? { website: 0, homebrew: 0, other: 0 } : null,
      downloadsByRef: {},
      downloadsByReferer: {},
      downloadsByUaFamily: { human: 0, bot: 0, unknown: 0 },
      humanInstalls: 0,
      newInstalls: 0,
      newInstallsByChannel: trend ? { homebrew: 0, direct: 0, unknown: 0 } : null,
      d7Retention: null,
      d7Retained: null,
      retentionSoFar: null,
      newsletterSignups: null,
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
    newsletterSignups: day.newsletterSignups,
  }
}

/** The per-day inputs `assembleFunnelRows` joins. `null` means that whole source failed. */
export interface FunnelInputs {
  worker: WorkerFunnel | null
  visitors: Map<string, number> | null
  pageviews: Map<string, number> | null
  clicks: Map<string, number> | null
  purchases: Map<string, number> | null
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
  }))
}

/** Index a worker response by day, recording what it covers. Pure for testing. */
export function indexWorkerFunnel(rows: WorkerFunnelDay[], requestedSince: string): WorkerFunnel {
  return {
    byDay: new Map(rows.map((r) => [r.date, r])),
    since: rows.length > 0 && rows[0].date > requestedSince ? rows[0].date : requestedSince,
    hasTrendFields: rows.some((r) => r.newInstallsByChannel !== undefined),
  }
}

export async function fetchFunnelData(env: FunnelEnv, country: string | null): Promise<SourceResult<FunnelData>> {
  const cacheKey = `days:${String(funnelFetchDays)}`
  const cacheExtra = `country=${country ?? 'all'}`
  const cached = await cacheGet<FunnelData>('funnel-v2', cacheKey, cacheExtra)
  if (cached) return { ok: true, data: cached }

  const now = new Date()
  const dates = buildFunnelDateList(funnelFetchDays, now)
  const sinceDate = dates[0]

  // Each side is best-effort and independent: one failing degrades its columns, never the whole page.
  const [workerFetched, umami, purchases] = await Promise.all([
    fetchWorkerFunnel(env, country, now).catch(() => null),
    fetchUmamiFunnelSeries(env, sinceDate, country).catch(() => null),
    fetchFunnelPurchases(env, sinceDate, country),
  ])

  const workerStatus = workerStatusOf(workerFetched, country)
  const worker = workerStatus === 'ok' ? workerFetched : null

  // If literally everything failed, surface an error so the section shows "Couldn't load" rather than
  // charts of nothing that look like a real empty period.
  if (workerStatus === 'unavailable' && !umami && !purchases) {
    return { ok: false, error: 'Funnel: every source failed (worker, Umami, and Paddle)' }
  }

  const rows = assembleFunnelRows(dates, {
    worker,
    visitors: umami?.visitors ?? null,
    pageviews: umami?.pageviews ?? null,
    clicks: umami?.clicks ?? null,
    purchases,
  })
  const data: FunnelData = { rows, country, workerStatus }
  await cacheSet('funnel-v2', cacheKey, data, cacheExtra)
  return { ok: true, data }
}

/**
 * Whether the worker's numbers can be shown. An older worker ignores `?country=` and answers with global
 * numbers; showing those as one country's would be a lie, so they're dropped and the UI says "not
 * available yet". Pure for testing.
 */
export function workerStatusOf(worker: WorkerFunnel | null, country: string | null): FunnelData['workerStatus'] {
  if (!worker) return 'unavailable'
  return country !== null && !worker.hasTrendFields ? 'country-unsupported' : 'ok'
}

/** Paddle purchases by day, or `null` when Paddle fails. Paddle has no country, so a country view gets `null`. */
function fetchFunnelPurchases(
  env: FunnelEnv,
  sinceDate: string,
  country: string | null,
): Promise<Map<string, number> | null> {
  if (country) return Promise.resolve(null)
  return fetchPaddlePurchasesByDay({ PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE }, sinceDate).catch(() => null)
}

/**
 * Fetch the api-server funnel and index it by day. Asks for the full window first and falls back to the
 * older worker's 90-day cap if that's refused. Throws when both fail (caller catches).
 */
async function fetchWorkerFunnel(env: FunnelEnv, country: string | null, now: Date): Promise<WorkerFunnel> {
  const countryParam = country ? `&country=${country}` : ''
  const get = (days: number) =>
    fetchWorkerEndpoint<WorkerFunnelDay[]>(
      env.LICENSE_SERVER_ADMIN_TOKEN,
      `/admin/funnel?days=${String(days)}${countryParam}`,
      env.WORKER_BASE_URL,
    )
  try {
    return indexWorkerFunnel(await get(funnelFetchDays), buildFunnelDateList(funnelFetchDays, now)[0])
  } catch {
    return indexWorkerFunnel(await get(legacyWorkerMaxDays), buildFunnelDateList(legacyWorkerMaxDays, now)[0])
  }
}

/**
 * Fetch getcmdr.com per-day visitors, pageviews, and `download`-click counts from Umami in one auth,
 * optionally scoped to one country. Throws on any error (caller catches): the series are independent
 * enough that a partial Umami failure isn't worth modeling separately here.
 */
async function fetchUmamiFunnelSeries(
  env: FunnelEnv,
  sinceDate: string,
  country: string | null,
): Promise<{ visitors: Map<string, number>; pageviews: Map<string, number>; clicks: Map<string, number> }> {
  const token = await authenticateUmami(env.UMAMI_API_URL, env.UMAMI_USERNAME, env.UMAMI_PASSWORD)
  // Window: from the start of the oldest funnel day (UTC) to now.
  const window = { startAt: Date.parse(`${sinceDate}T00:00:00Z`), endAt: Date.now(), country }
  const [traffic, clicks] = await Promise.all([
    fetchUmamiDailyTraffic(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, window),
    fetchUmamiDailyEventSeries(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, window, 'download'),
  ])
  return { ...traffic, clicks }
}
