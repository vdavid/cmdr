import type { DashboardSelection, SourceResult } from '../types.js'
import { toTimeWindow, selectionCacheKey } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'

export interface UmamiSiteStats {
  pageviews: { value: number; prev: number }
  visitors: { value: number; prev: number }
  visits: { value: number; prev: number }
  bounces: { value: number; prev: number }
  totaltime: { value: number; prev: number }
}

export interface UmamiMetricItem {
  x: string
  y: number
}

export interface UmamiData {
  personalSite: UmamiSiteStats
  website: UmamiSiteStats
  /** getprvw.com stats */
  prvw: UmamiSiteStats
  /** Top referrers for getcmdr.com */
  websiteReferrers: UmamiMetricItem[]
  /** Top pages for getcmdr.com */
  websitePages: UmamiMetricItem[]
  /** Top countries for getcmdr.com */
  websiteCountries: UmamiMetricItem[]
  /** Download button click events */
  downloadEvents: UmamiMetricItem[]
  /** Top referrers for getprvw.com */
  prvwReferrers: UmamiMetricItem[]
  /** Top pages for getprvw.com */
  prvwPages: UmamiMetricItem[]
}

interface UmamiEnv {
  UMAMI_API_URL: string
  UMAMI_USERNAME: string
  UMAMI_PASSWORD: string
  UMAMI_WEBSITE_ID: string
  UMAMI_BLOG_WEBSITE_ID: string
  UMAMI_PRVW_WEBSITE_ID: string
}

/** Authenticates with Umami and returns a JWT token. Exported for the funnel source's own per-day calls. */
export async function authenticateUmami(apiUrl: string, username: string, password: string): Promise<string> {
  return authenticate(apiUrl, username, password)
}

/** Authenticates with Umami and returns a JWT token. */
async function authenticate(apiUrl: string, username: string, password: string): Promise<string> {
  const response = await fetch(`${apiUrl}/api/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  })
  if (!response.ok) {
    throw new Error(`Umami auth returned ${response.status}`)
  }
  const body = (await response.json()) as { token: string }
  return body.token
}

interface UmamiRawStats {
  pageviews: number
  visitors: number
  visits: number
  bounces: number
  totaltime: number
  comparison: {
    pageviews: number
    visitors: number
    visits: number
    bounces: number
    totaltime: number
  }
}

async function fetchStats(
  apiUrl: string,
  token: string,
  websiteId: string,
  startAt: number,
  endAt: number,
): Promise<UmamiSiteStats> {
  const url = `${apiUrl}/api/websites/${websiteId}/stats?startAt=${startAt}&endAt=${endAt}`
  const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } })
  if (!response.ok) {
    throw new Error(`Umami stats returned ${response.status}`)
  }
  const raw = (await response.json()) as UmamiRawStats
  return {
    pageviews: { value: raw.pageviews, prev: raw.comparison?.pageviews ?? 0 },
    visitors: { value: raw.visitors, prev: raw.comparison?.visitors ?? 0 },
    visits: { value: raw.visits, prev: raw.comparison?.visits ?? 0 },
    bounces: { value: raw.bounces, prev: raw.comparison?.bounces ?? 0 },
    totaltime: { value: raw.totaltime, prev: raw.comparison?.totaltime ?? 0 },
  }
}

async function fetchMetrics(
  apiUrl: string,
  token: string,
  websiteId: string,
  startAt: number,
  endAt: number,
  type: string,
): Promise<UmamiMetricItem[]> {
  const url = `${apiUrl}/api/websites/${websiteId}/metrics?startAt=${startAt}&endAt=${endAt}&type=${type}`
  const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } })
  if (!response.ok) {
    throw new Error(`Umami metrics (${type}) returned ${response.status}`)
  }
  return (await response.json()) as UmamiMetricItem[]
}

/** A `{ x: 'YYYY-MM-DD HH:MM:SS', y: count }` point from a daily Umami series. */
interface UmamiDailyPoint {
  x: string
  y: number
}

/** A `{ x: eventName, t: 'YYYY-MM-DD HH:MM:SS', y: count }` point from the daily event series. */
interface UmamiEventSeriesPoint {
  x: string
  t: string
  y: number
}

/** Maps an Umami series timestamp (`YYYY-MM-DD HH:MM:SS`, UTC) to its `YYYY-MM-DD` day. */
function dayOfUmamiTimestamp(ts: string): string {
  return ts.slice(0, 10)
}

/**
 * Per-day visitor counts (Umami "sessions" series, what Umami's own daily chart labels visitors) for
 * `websiteId`, as a `Map<YYYY-MM-DD, count>`. Uses `/pageviews?unit=day&timezone=UTC` so the day
 * buckets line up with the rest of the funnel (all UTC). The endpoint returns `pageviews` and
 * `sessions` arrays; we take `sessions`.
 */
export async function fetchUmamiDailySeries(
  apiUrl: string,
  token: string,
  websiteId: string,
  startAt: number,
  endAt: number,
): Promise<Map<string, number>> {
  const url = `${apiUrl}/api/websites/${websiteId}/pageviews?startAt=${startAt}&endAt=${endAt}&unit=day&timezone=UTC`
  const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } })
  if (!response.ok) {
    throw new Error(`Umami pageviews series returned ${response.status}`)
  }
  const body = (await response.json()) as { sessions?: UmamiDailyPoint[] }
  const byDay = new Map<string, number>()
  for (const point of body.sessions ?? []) {
    byDay.set(dayOfUmamiTimestamp(point.x), point.y)
  }
  return byDay
}

/**
 * Per-day counts of a named custom event (for example `download`) for `websiteId`, as a
 * `Map<YYYY-MM-DD, count>`. Uses `/events/series?unit=day&timezone=UTC`, which returns one point per
 * (event name, day); we keep only the rows whose `x` matches `eventName` and bucket by `t`'s day.
 */
export async function fetchUmamiDailyEventSeries(
  apiUrl: string,
  token: string,
  websiteId: string,
  startAt: number,
  endAt: number,
  eventName: string,
): Promise<Map<string, number>> {
  const url = `${apiUrl}/api/websites/${websiteId}/events/series?startAt=${startAt}&endAt=${endAt}&unit=day&timezone=UTC`
  const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } })
  if (!response.ok) {
    throw new Error(`Umami events series returned ${response.status}`)
  }
  const points = (await response.json()) as UmamiEventSeriesPoint[]
  const byDay = new Map<string, number>()
  for (const point of points) {
    if (point.x !== eventName) continue
    const day = dayOfUmamiTimestamp(point.t)
    byDay.set(day, (byDay.get(day) ?? 0) + point.y)
  }
  return byDay
}

export function parseUmamiStats(raw: unknown): UmamiSiteStats {
  const r = raw as Record<string, { value: number; prev: number }>
  return {
    pageviews: r.pageviews,
    visitors: r.visitors,
    visits: r.visits,
    bounces: r.bounces,
    totaltime: r.totaltime,
  }
}

export function parseUmamiMetrics(raw: unknown): UmamiMetricItem[] {
  const items = raw as Array<{ x: string; y: number }>
  return items.map((item) => ({ x: item.x, y: item.y }))
}

export async function fetchUmamiData(env: UmamiEnv, selection: DashboardSelection): Promise<SourceResult<UmamiData>> {
  const cacheKey = selectionCacheKey(selection)
  const cached = await cacheGet<UmamiData>('umami-v2', cacheKey)
  if (cached) return { ok: true, data: cached }

  try {
    const token = await authenticate(env.UMAMI_API_URL, env.UMAMI_USERNAME, env.UMAMI_PASSWORD)
    const { startAt, endAt } = toTimeWindow(selection)

    const [
      personalSite,
      website,
      prvw,
      websiteReferrers,
      websitePages,
      websiteCountries,
      downloadEvents,
      prvwReferrers,
      prvwPages,
    ] = await Promise.all([
      fetchStats(env.UMAMI_API_URL, token, env.UMAMI_BLOG_WEBSITE_ID, startAt, endAt),
      fetchStats(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt),
      fetchStats(env.UMAMI_API_URL, token, env.UMAMI_PRVW_WEBSITE_ID, startAt, endAt),
      fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt, 'referrer'),
      fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt, 'path'),
      fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt, 'country'),
      fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt, 'event'),
      fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_PRVW_WEBSITE_ID, startAt, endAt, 'referrer'),
      fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_PRVW_WEBSITE_ID, startAt, endAt, 'path'),
    ])

    const data: UmamiData = {
      personalSite,
      website,
      prvw,
      websiteReferrers,
      websitePages,
      websiteCountries,
      downloadEvents,
      prvwReferrers,
      prvwPages,
    }
    await cacheSet('umami-v2', cacheKey, data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Umami: ${e instanceof Error ? e.message : String(e)}` }
  }
}
