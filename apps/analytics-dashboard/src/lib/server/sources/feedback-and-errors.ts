import type { DashboardSelection, SourceResult } from '../types.js'
import { selectionCacheKey } from '../types.js'
import type { FeedbackRow, ErrorReportRow } from '../../feedback-and-errors.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerEndpoint } from './worker-endpoint.js'

export interface FeedbackAndErrorsData {
  feedback: FeedbackRow[]
  errorReports: ErrorReportRow[]
}

interface FeedbackAndErrorsEnv {
  LICENSE_SERVER_ADMIN_TOKEN: string
  /** Optional override for the api-server base URL (local QA). Defaults to production. */
  WORKER_BASE_URL?: string
}

/**
 * Feedback is read live on every load and only the error-report listing is cached. A message
 * marked answered has to show as answered on the reload right after, and a cached feedback list
 * would keep it under "Awaiting reply" for up to an hour. The feedback query is one small D1 read;
 * the error-report listing is an R2 scan, so that's the half worth caching.
 */
export async function fetchFeedbackAndErrorsData(
  env: FeedbackAndErrorsEnv,
  selection: DashboardSelection,
): Promise<SourceResult<FeedbackAndErrorsData>> {
  try {
    // `/admin/feedback` and `/admin/error-reports` take 7d/30d/90d/all, so every range passes through.
    const workerRange = selection.range
    const [feedback, errorReports] = await Promise.all([
      fetchWorkerEndpoint<FeedbackRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/feedback?range=${workerRange}`,
        env.WORKER_BASE_URL,
      ),
      fetchErrorReports(env, selection, workerRange),
    ])
    return { ok: true, data: { feedback, errorReports } }
  } catch (e) {
    return { ok: false, error: `Feedback & errors: ${e instanceof Error ? e.message : String(e)}` }
  }
}

async function fetchErrorReports(
  env: FeedbackAndErrorsEnv,
  selection: DashboardSelection,
  workerRange: string,
): Promise<ErrorReportRow[]> {
  const cached = await cacheGet<ErrorReportRow[]>('error-reports', selectionCacheKey(selection))
  if (cached) return cached

  const errorReports = await fetchWorkerEndpoint<ErrorReportRow[]>(
    env.LICENSE_SERVER_ADMIN_TOKEN,
    `/admin/error-reports?range=${workerRange}`,
    env.WORKER_BASE_URL,
  )
  await cacheSet('error-reports', selectionCacheKey(selection), errorReports)
  return errorReports
}

/**
 * Marks one feedback message answered, or back to awaiting a reply (`PUT /admin/feedback/:id/answered`).
 * Returns the stored `answeredAt` (null when unmarked). The worker's message comes back verbatim on a
 * rejection so the page shows what the server actually objected to.
 */
export async function setFeedbackAnswered(
  env: FeedbackAndErrorsEnv,
  input: { id: number; answered: boolean },
): Promise<SourceResult<string | null>> {
  const baseUrl = env.WORKER_BASE_URL || 'https://api.getcmdr.com'
  try {
    const response = await fetch(`${baseUrl}/admin/feedback/${String(input.id)}/answered`, {
      method: 'PUT',
      headers: {
        Authorization: `Bearer ${env.LICENSE_SERVER_ADMIN_TOKEN}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ answered: input.answered }),
    })
    const text = await response.text()
    if (!response.ok) throw new Error(errorMessage(text) ?? `returned ${String(response.status)}`)
    return { ok: true, data: (JSON.parse(text) as { answeredAt: string | null }).answeredAt }
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) }
  }
}

/** The worker's `{ error }` string, or null when the body isn't the shape we expect. */
function errorMessage(body: string): string | null {
  try {
    const parsed = JSON.parse(body) as { error?: unknown }
    return typeof parsed.error === 'string' ? parsed.error : null
  } catch {
    return null
  }
}
