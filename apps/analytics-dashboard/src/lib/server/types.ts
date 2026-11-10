import { defaultRange, rangeDays, timeRanges, type TimeRange } from '../time-range.js'

export { defaultRange, rangeDays, timeRanges, type TimeRange }

/**
 * The resolved time selection for a page load. An object (not a bare `TimeRange`) so a page and its sources
 * pass one value around; sources convert it to their own native windows via `toTimeWindow`.
 */
export interface DashboardSelection {
  range: TimeRange
}

/** Resolve the URL's `range` param into a `DashboardSelection`, defaulting to `defaultRange`. */
export function resolveSelection(rangeParam: string | null): DashboardSelection {
  const range = timeRanges.find((r) => r === rangeParam) ?? defaultRange
  return { range }
}

/** Converts a selection to a start/end timestamp pair (milliseconds, UTC): a rolling window ending now. */
export function toTimeWindow(selection: DashboardSelection): { startAt: number; endAt: number } {
  const now = Date.now()
  return { startAt: now - rangeDays[selection.range] * 86_400_000, endAt: now }
}

/** A stable cache key for a selection: the range name. */
export function selectionCacheKey(selection: DashboardSelection): string {
  return selection.range
}

/** Wraps a data source result. Either the data or an error message for the UI. */
export type SourceResult<T> = { ok: true; data: T } | { ok: false; error: string }

/** The env every data source reads, declared in `src/env.ts` and assembled by `env.ts`'s `dashboardEnv()`. */
export interface DashboardEnv {
  UMAMI_API_URL: string
  UMAMI_USERNAME: string
  UMAMI_PASSWORD: string
  UMAMI_WEBSITE_ID: string
  UMAMI_BLOG_WEBSITE_ID: string
  UMAMI_PRVW_WEBSITE_ID: string
  PADDLE_API_KEY_LIVE: string
  POSTHOG_API_KEY: string
  POSTHOG_PROJECT_ID: string
  POSTHOG_API_URL: string
  GITHUB_TOKEN?: string
  LICENSE_SERVER_ADMIN_TOKEN: string
  /** Optional local-QA override for the api-server base URL (defaults to production). */
  WORKER_BASE_URL?: string
}
