import type { TimeRange, SourceResult } from '../types.js'
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
  results: Array<[string, number]>
  error?: string
}

function toHogQLInterval(range: TimeRange): string {
  const map: Record<TimeRange, string> = {
    '24h': '1 day',
    '7d': '7 day',
    '30d': '30 day',
  }
  return map[range]
}

export function parseHogQLResponse(raw: HogQLResponse): PostHogDailyRow[] {
  if (!raw.results) return []
  return raw.results.map(([day, views]) => ({ day, views }))
}

export async function fetchPostHogData(env: PostHogEnv, range: TimeRange): Promise<SourceResult<PostHogData>> {
  const cached = await cacheGet<PostHogData>('posthog', range)
  if (cached) return { ok: true, data: cached }

  try {
    const interval = toHogQLInterval(range)
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
      throw new Error(`PostHog returned ${response.status}`)
    }

    const raw = (await response.json()) as HogQLResponse
    if (raw.error) {
      throw new Error(`PostHog query error: ${raw.error}`)
    }

    const dailyPageviews = parseHogQLResponse(raw)
    const totalPageviews = dailyPageviews.reduce((sum, row) => sum + row.views, 0)

    const data: PostHogData = { totalPageviews, dailyPageviews }
    await cacheSet('posthog', range, data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `PostHog: ${e instanceof Error ? e.message : String(e)}` }
  }
}
