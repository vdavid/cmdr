import type { SourceResult } from '../types.js'
import type { UaFamilyCounts } from '../../funnel.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerEndpoint } from './worker-endpoint.js'
import { authenticateUmami, fetchUmamiDailySeries, fetchUmamiDailyEventSeries } from './umami.js'
import { fetchPaddlePurchasesByDay } from './paddle.js'

/**
 * The "Daily funnel" section's data: one row per UTC day for the last ~30 days, joining web-side
 * signals (Umami) with server-side telemetry (the api-server `/admin/funnel` endpoint) and purchases
 * (Paddle). It's its own source so the funnel table is independent of the page's range picker (it
 * always shows the last 30 days). Every cell is either a real number or `null` ("couldn't get this"),
 * which the table renders as a dash so a blank is never confused with a real zero.
 */

/** How many UTC days the funnel table covers. Today is included as a partial day. */
export const funnelDays = 30

/** One UTC day in the funnel table. `null` anywhere means "unknown", rendered as a dash. */
export interface FunnelRow {
  /** UTC day, `YYYY-MM-DD`. */
  date: string
  /** Unique visitors to getcmdr.com that day (Umami sessions). `null` when Umami is unavailable. */
  visitors: number | null
  /** `download` button click events on getcmdr.com that day (Umami). `null` when Umami is unavailable. */
  downloadClicks: number | null
  /** Server-side DMG downloads logged that day (api-server). `null` when the worker is unavailable. */
  serverDownloads: number | null
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
  /** D7 retention fraction (0..1) for this cohort, or `null` when too young / worker unavailable. */
  d7Retention: number | null
  /** Raw count behind `d7Retention`, or `null`. */
  d7Retained: number | null
  /** Newsletter + beta signups that day (Listmonk via api-server). `null` when Listmonk is unavailable. */
  newsletterSignups: number | null
  /** Completed Paddle transactions that day. `null` when Paddle is unavailable. */
  purchases: number | null
}

export interface FunnelData {
  rows: FunnelRow[]
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
  newInstalls: number
  d7Retention: number | null
  d7Retained: number | null
  newsletterSignups: number | null
}

// The ranked-channel helper and its type are client-safe (the page renders them), so they live in
// `$lib/funnel.ts` outside `$lib/server`. Re-exported here so server-side callers and the existing
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
  | 'downloadsByRef'
  | 'downloadsByReferer'
  | 'downloadsByUaFamily'
  | 'humanInstalls'
  | 'newInstalls'
  | 'd7Retention'
  | 'd7Retained'
  | 'newsletterSignups'
>

/**
 * One day's worker-sourced columns. A `null` source makes every one of them unknown; a present source
 * with no row that day is a real empty day (0 / `{}`). D7 and signups stay `null` even on a present
 * row: a young cohort or Listmonk being down is genuinely unknown, not zero.
 */
function workerColumns(workerByDay: Map<string, WorkerFunnelDay> | null, date: string): WorkerColumns {
  if (!workerByDay) {
    return {
      serverDownloads: null,
      downloadsByRef: null,
      downloadsByReferer: null,
      downloadsByUaFamily: null,
      humanInstalls: null,
      newInstalls: null,
      d7Retention: null,
      d7Retained: null,
      newsletterSignups: null,
    }
  }

  const worker = workerByDay.get(date)
  if (!worker) {
    return {
      serverDownloads: 0,
      downloadsByRef: {},
      downloadsByReferer: {},
      downloadsByUaFamily: { human: 0, bot: 0, unknown: 0 },
      humanInstalls: 0,
      newInstalls: 0,
      d7Retention: null,
      d7Retained: null,
      newsletterSignups: null,
    }
  }

  // The `??` defaults cover a worker deployed before these fields existed.
  return {
    serverDownloads: worker.downloads,
    downloadsByRef: worker.downloadsByRef,
    downloadsByReferer: worker.downloadsByReferer ?? {},
    downloadsByUaFamily: worker.downloadsByUaFamily ?? { human: 0, bot: 0, unknown: 0 },
    humanInstalls: worker.humanInstalls ?? 0,
    newInstalls: worker.newInstalls,
    d7Retention: worker.d7Retention,
    d7Retained: worker.d7Retained,
    newsletterSignups: worker.newsletterSignups,
  }
}

/**
 * Merge the four per-day inputs into one row array over the canonical `dates` list. Pure for testing.
 * Each source is passed as a `Map<day, number>` or `null` (the whole source failed); a missing day
 * inside a present source is a real 0, while a `null` source makes that whole column `null` (a dash).
 * The worker funnel is the one source that carries multiple columns (downloads, installs, D7, signups);
 * its per-column `null`s (e.g. young D7 cohorts) pass straight through.
 */
export function assembleFunnelRows(
  dates: string[],
  workerByDay: Map<string, WorkerFunnelDay> | null,
  visitorsByDay: Map<string, number> | null,
  clicksByDay: Map<string, number> | null,
  purchasesByDay: Map<string, number> | null,
): FunnelRow[] {
  return dates.map((date) => ({
    date,
    visitors: dailyCount(visitorsByDay, date),
    downloadClicks: dailyCount(clicksByDay, date),
    ...workerColumns(workerByDay, date),
    purchases: dailyCount(purchasesByDay, date),
  }))
}

export async function fetchFunnelData(env: FunnelEnv): Promise<SourceResult<FunnelData>> {
  const cached = await cacheGet<FunnelData>('funnel', '30d')
  if (cached) return { ok: true, data: cached }

  const now = new Date()
  const dates = buildFunnelDateList(funnelDays, now)
  const sinceDate = dates[0]

  // Each side is best-effort and independent: one failing degrades its columns to dashes, never the
  // whole table. So we settle each separately rather than failing the section on the first error.
  const workerByDay = await fetchWorkerFunnel(env).catch(() => null)
  const [visitorsByDay, clicksByDay] = await fetchUmamiFunnelSeries(env, sinceDate).catch(
    () => [null, null] as [Map<string, number> | null, Map<string, number> | null],
  )
  const purchasesByDay = await fetchPaddlePurchasesByDay(
    { PADDLE_API_KEY_LIVE: env.PADDLE_API_KEY_LIVE },
    sinceDate,
  ).catch(() => null)

  // If literally everything failed, surface an error so the section shows "Couldn't load" rather than a
  // table of all-dashes that looks like a real empty period.
  if (!workerByDay && !visitorsByDay && !clicksByDay && !purchasesByDay) {
    return { ok: false, error: 'Funnel: every source failed (worker, Umami, and Paddle)' }
  }

  const rows = assembleFunnelRows(dates, workerByDay, visitorsByDay, clicksByDay, purchasesByDay)
  const data: FunnelData = { rows }
  await cacheSet('funnel', '30d', data)
  return { ok: true, data }
}

/** Fetch the api-server funnel and index it by day. Throws on any worker error (caller catches). */
async function fetchWorkerFunnel(env: FunnelEnv): Promise<Map<string, WorkerFunnelDay>> {
  const rows = await fetchWorkerEndpoint<WorkerFunnelDay[]>(
    env.LICENSE_SERVER_ADMIN_TOKEN,
    `/admin/funnel?days=${String(funnelDays)}`,
    env.WORKER_BASE_URL,
  )
  return new Map(rows.map((r) => [r.date, r]))
}

/**
 * Fetch getcmdr.com per-day visitors and per-day `download`-click counts from Umami in one auth.
 * Returns `[null, null]` on any error (caller catches). The two series are independent enough that a
 * partial Umami failure isn't worth modeling separately here.
 */
async function fetchUmamiFunnelSeries(
  env: FunnelEnv,
  sinceDate: string,
): Promise<[Map<string, number>, Map<string, number>]> {
  const token = await authenticateUmami(env.UMAMI_API_URL, env.UMAMI_USERNAME, env.UMAMI_PASSWORD)
  // Window: from the start of the oldest funnel day (UTC) to now.
  const startAt = Date.parse(`${sinceDate}T00:00:00Z`)
  const endAt = Date.now()
  const [visitors, clicks] = await Promise.all([
    fetchUmamiDailySeries(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt),
    fetchUmamiDailyEventSeries(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt, 'download'),
  ])
  return [visitors, clicks]
}
