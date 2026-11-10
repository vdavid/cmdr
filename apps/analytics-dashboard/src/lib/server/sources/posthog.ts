import type { DashboardSelection, SourceResult } from '../types.js'
import { rangeDays, selectionCacheKey, type TimeRange } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'

export interface PostHogDailyRow {
  day: string
  views: number
}

export interface PostHogData {
  totalPageviews: number
  dailyPageviews: PostHogDailyRow[]
}

export interface PostHogEnv {
  POSTHOG_API_KEY: string
  POSTHOG_PROJECT_ID: string
  POSTHOG_API_URL: string
}

interface HogQLResponse {
  columns: string[]
  /** Absent when PostHog answers with an `error` instead of rows, so guard before reading it. */
  results?: Array<[string, number]>
  error?: string
}

function toHogQLInterval(range: TimeRange): string {
  return `${String(rangeDays[range])} day`
}

export function parseHogQLResponse(raw: HogQLResponse): PostHogDailyRow[] {
  if (!raw.results) return []
  return raw.results.map(([day, views]) => ({ day, views }))
}

export async function fetchPostHogData(
  env: PostHogEnv,
  selection: DashboardSelection,
): Promise<SourceResult<PostHogData>> {
  const cached = await cacheGet<PostHogData>('posthog', selectionCacheKey(selection))
  if (cached) return { ok: true, data: cached }

  try {
    const interval = toHogQLInterval(selection.range)
    const url = `${env.POSTHOG_API_URL}/api/projects/${env.POSTHOG_PROJECT_ID}/query/`

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${env.POSTHOG_API_KEY}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        query: {
          kind: 'HogQLQuery',
          query: `SELECT toDate(timestamp) as day, count() as views FROM events WHERE event = '$pageview' AND timestamp > now() - interval ${interval} GROUP BY day ORDER BY day`,
        },
      }),
    })

    if (!response.ok) {
      throw new Error(`PostHog returned ${String(response.status)}`)
    }

    const raw = (await response.json()) as HogQLResponse
    if (raw.error) {
      throw new Error(`PostHog query error: ${raw.error}`)
    }

    const dailyPageviews = parseHogQLResponse(raw)
    const totalPageviews = dailyPageviews.reduce((sum, row) => sum + row.views, 0)

    const data: PostHogData = { totalPageviews, dailyPageviews }
    await cacheSet('posthog', selectionCacheKey(selection), data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `PostHog: ${e instanceof Error ? e.message : String(e)}` }
  }
}
