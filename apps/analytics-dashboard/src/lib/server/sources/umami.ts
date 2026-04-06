import type { TimeRange, SourceResult } from '../types.js'
import { toTimeWindow } from '../types.js'
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
  /** Top referrers for getcmdr.com */
  websiteReferrers: UmamiMetricItem[]
  /** Top pages for getcmdr.com */
  websitePages: UmamiMetricItem[]
  /** Top countries for getcmdr.com */
  websiteCountries: UmamiMetricItem[]
  /** Download button click events */
  downloadEvents: UmamiMetricItem[]
}

interface UmamiEnv {
  UMAMI_API_URL: string
  UMAMI_USERNAME: string
  UMAMI_PASSWORD: string
  UMAMI_WEBSITE_ID: string
  UMAMI_BLOG_WEBSITE_ID: string
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

export async function fetchUmamiData(env: UmamiEnv, range: TimeRange): Promise<SourceResult<UmamiData>> {
  const cached = await cacheGet<UmamiData>('umami-v2', range)
  if (cached) return { ok: true, data: cached }

  try {
    const token = await authenticate(env.UMAMI_API_URL, env.UMAMI_USERNAME, env.UMAMI_PASSWORD)
    const { startAt, endAt } = toTimeWindow(range)

    const [personalSite, website, websiteReferrers, websitePages, websiteCountries, downloadEvents] = await Promise.all(
      [
        fetchStats(env.UMAMI_API_URL, token, env.UMAMI_BLOG_WEBSITE_ID, startAt, endAt),
        fetchStats(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt),
        fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt, 'referrer'),
        fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt, 'path'),
        fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt, 'country'),
        fetchMetrics(env.UMAMI_API_URL, token, env.UMAMI_WEBSITE_ID, startAt, endAt, 'event'),
      ],
    )

    const data: UmamiData = { personalSite, website, websiteReferrers, websitePages, websiteCountries, downloadEvents }
    await cacheSet('umami-v2', range, data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Umami: ${e instanceof Error ? e.message : String(e)}` }
  }
}
