import type { TimeRange, SourceResult } from '../types.js'
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
 * Maps the dashboard TimeRange to the worker range param. Both `/admin/feedback` and
 * `/admin/error-reports` take 7d/30d/90d/all, so the dashboard's 24h maps up to 7d (these are
 * low-volume streams; a 24h window would usually be empty). Same approach as heartbeat DAU.
 */
const rangeMap: Record<TimeRange, string> = {
  '24h': '7d',
  '7d': '7d',
  '30d': '30d',
}

export async function fetchFeedbackAndErrorsData(
  env: FeedbackAndErrorsEnv,
  range: TimeRange,
): Promise<SourceResult<FeedbackAndErrorsData>> {
  const cached = await cacheGet<FeedbackAndErrorsData>('feedback-and-errors', range)
  if (cached) return { ok: true, data: cached }

  try {
    const workerRange = rangeMap[range]
    const [feedback, errorReports] = await Promise.all([
      fetchWorkerEndpoint<FeedbackRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/feedback?range=${workerRange}`,
        env.WORKER_BASE_URL,
      ),
      fetchWorkerEndpoint<ErrorReportRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/error-reports?range=${workerRange}`,
        env.WORKER_BASE_URL,
      ),
    ])

    const data: FeedbackAndErrorsData = { feedback, errorReports }
    await cacheSet('feedback-and-errors', range, data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Feedback & errors: ${e instanceof Error ? e.message : String(e)}` }
  }
}
