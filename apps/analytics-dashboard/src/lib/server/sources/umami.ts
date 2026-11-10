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
    throw new Error(`Umami auth returned ${String(response.status)}`)
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
  /** Only present when Umami has a previous window to compare against, so treat it as optional. */
  comparison?: {
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
  const url = `${apiUrl}/api/websites/${websiteId}/stats?startAt=${String(startAt)}&endAt=${String(endAt)}`
  const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } })
  if (!response.ok) {
    throw new Error(`Umami stats returned ${String(response.status)}`)
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
  const url = `${apiUrl}/api/websites/${websiteId}/metrics?startAt=${String(startAt)}&endAt=${String(endAt)}&type=${type}`
  const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } })
  if (!response.ok) {
    throw new Error(`Umami metrics (${type}) returned ${String(response.status)}`)
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

/** `&country=XX` for a country-scoped Umami query, or nothing. Umami filters every endpoint this way. */
function countryFilter(country: string | null): string {
  return country ? `&country=${encodeURIComponent(country)}` : ''
}

/**
 * The longest span one `unit=day` series call may cover. Past roughly seven months Umami ignores
 * `unit=day` and answers monthly buckets with the same shape, which would read as a handful of huge days
 * (verified on anal.veszelovszki.com, Umami v3, curl: 210 days came back daily, 230 monthly, 2026-11-10).
 * At 200, the usual 181-day funnel stays one call and the deepest (400 days) takes two.
 */
export const umamiMaxDaySpanDays = 200

/**
 * Split `[startAt, endAt]` into back-to-back spans of at most `umamiMaxDaySpanDays` days, each starting
 * where the last stopped, so a day never lands in two spans (`startAt` is a UTC midnight). Pure for testing.
 */
export function umamiDaySpans(window: { startAt: number; endAt: number }): { startAt: number; endAt: number }[] {
  const step = umamiMaxDaySpanDays * 86_400_000
  const spans: { startAt: number; endAt: number }[] = []
  for (let start = window.startAt; start <= window.endAt; start += step) {
    spans.push({ startAt: start, endAt: Math.min(start + step - 1, window.endAt) })
  }
  return spans
}

/** Merge per-span day maps (their days never overlap). */
function mergeDays(maps: Map<string, number>[]): Map<string, number> {
  return new Map(maps.flatMap((m) => [...m]))
}

/** Per-day getcmdr.com traffic: unique visitors and pageviews, each a `Map<YYYY-MM-DD, count>`. */
export interface UmamiDailyTraffic {
  visitors: Map<string, number>
  pageviews: Map<string, number>
}

/**
 * Per-day visitors (Umami's "sessions" series, what its own daily chart labels visitors) and pageviews for
 * `websiteId`, optionally scoped to one country. Uses `/pageviews?unit=day&timezone=UTC` so the day buckets
 * line up with the rest of the funnel (all UTC); one call per `umamiDaySpans` span returns both arrays.
 */
export async function fetchUmamiDailyTraffic(
  apiUrl: string,
  token: string,
  websiteId: string,
  window: { startAt: number; endAt: number; country: string | null },
): Promise<UmamiDailyTraffic> {
  const spans = await Promise.all(
    umamiDaySpans(window).map((span) => fetchUmamiDailyTrafficSpan(apiUrl, token, websiteId, { ...window, ...span })),
  )
  return { visitors: mergeDays(spans.map((s) => s.visitors)), pageviews: mergeDays(spans.map((s) => s.pageviews)) }
}

async function fetchUmamiDailyTrafficSpan(
  apiUrl: string,
  token: string,
  websiteId: string,
  window: { startAt: number; endAt: number; country: string | null },
): Promise<UmamiDailyTraffic> {
  const url =
    `${apiUrl}/api/websites/${websiteId}/pageviews?startAt=${String(window.startAt)}&endAt=${String(window.endAt)}` +
    `&unit=day&timezone=UTC${countryFilter(window.country)}`
  const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } })
  if (!response.ok) {
    throw new Error(`Umami pageviews series returned ${String(response.status)}`)
  }
  const body = (await response.json()) as { sessions?: UmamiDailyPoint[]; pageviews?: UmamiDailyPoint[] }
  const byDay = (points: UmamiDailyPoint[] | undefined) =>
    new Map((points ?? []).map((point) => [dayOfUmamiTimestamp(point.x), point.y]))
  return { visitors: byDay(body.sessions), pageviews: byDay(body.pageviews) }
}

/**
 * Per-day counts of a named custom event (for example `download`) for `websiteId`, optionally scoped to one
 * country, as a `Map<YYYY-MM-DD, count>`. Uses `/events/series?unit=day&timezone=UTC`, which returns one
 * point per (event name, day); we keep only the rows whose `x` matches `eventName` and bucket by `t`'s day.
 * One call per `umamiDaySpans` span.
 */
export async function fetchUmamiDailyEventSeries(
  apiUrl: string,
  token: string,
  websiteId: string,
  window: { startAt: number; endAt: number; country: string | null },
  eventName: string,
): Promise<Map<string, number>> {
  const spans = await Promise.all(
    umamiDaySpans(window).map((span) =>
      fetchUmamiDailyEventSeriesSpan(apiUrl, token, websiteId, { ...window, ...span }, eventName),
    ),
  )
  return mergeDays(spans)
}

async function fetchUmamiDailyEventSeriesSpan(
  apiUrl: string,
  token: string,
  websiteId: string,
  window: { startAt: number; endAt: number; country: string | null },
  eventName: string,
): Promise<Map<string, number>> {
  const url =
    `${apiUrl}/api/websites/${websiteId}/events/series?startAt=${String(window.startAt)}&endAt=${String(window.endAt)}` +
    `&unit=day&timezone=UTC${countryFilter(window.country)}`
  const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } })
  if (!response.ok) {
    throw new Error(`Umami events series returned ${String(response.status)}`)
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

/**
 * Unique visitors per country (`metrics?type=country`) over the window, as `Map<ISO-2, visitors>`.
 * Umami returns the codes uppercase already; they're normalized anyway so they join the worker's.
 */
export async function fetchUmamiVisitorsByCountry(
  apiUrl: string,
  token: string,
  websiteId: string,
  startAt: number,
  endAt: number,
): Promise<Map<string, number>> {
  const items = await fetchMetrics(apiUrl, token, websiteId, startAt, endAt, 'country')
  return new Map(items.map((item) => [item.x.toUpperCase(), item.y]))
}

/** The page size for `fetchUmamiEventCountsByCountry`; 90 days of clicks fit in one or two pages. */
const eventPageSize = 1000

/** Past this many pages the listing is runaway, not traffic: fail loudly rather than drain the subrequest budget. */
const maxEventPages = 20

/** The country label for an event Umami geolocated nowhere, matching the worker's no-country bucket. */
const noCountry = '(unknown)'

/**
 * How many `eventName` events each country fired over the window, as `Map<ISO-2 | "(unknown)", count>`, from
 * one paginated `/events?event=NAME` listing (each row carries the session's country). No aggregate endpoint
 * groups custom events by country: `metrics?type=country&event=` and `/breakdown` both count pageviews only
 * and answer `[]` for a custom event (verified on anal.veszelovszki.com, Umami v3, curl, 2026-11-10). The
 * listing's per-country tallies matched `metrics?type=event&country=XX` there (US 35 of 287).
 */
export async function fetchUmamiEventCountsByCountry(
  apiUrl: string,
  token: string,
  websiteId: string,
  window: { startAt: number; endAt: number },
  eventName: string,
  pageSize = eventPageSize,
): Promise<Map<string, number>> {
  const counts = new Map<string, number>()
  let seen = 0
  for (let page = 1; page <= maxEventPages; page++) {
    const url =
      `${apiUrl}/api/websites/${websiteId}/events?startAt=${String(window.startAt)}&endAt=${String(window.endAt)}` +
      `&event=${encodeURIComponent(eventName)}&page=${String(page)}&pageSize=${String(pageSize)}`
    const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } })
    if (!response.ok) {
      throw new Error(`Umami events listing returned ${String(response.status)}`)
    }
    const body = (await response.json()) as { data: { country: string | null }[]; count: number }
    for (const row of body.data) {
      const country = row.country ? row.country.toUpperCase() : noCountry
      counts.set(country, (counts.get(country) ?? 0) + 1)
    }
    seen += body.data.length
    if (body.data.length === 0 || seen >= body.count) return counts
  }
  throw new Error(`Umami events listing ran past ${String(maxEventPages)} pages`)
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
