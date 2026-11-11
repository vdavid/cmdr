import type { DashboardSelection, SourceResult } from '../types.js'
import { selectionCacheKey } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerEndpoint } from './worker-endpoint.js'
import type { FeatureUsageData } from '../../feature-usage.js'

interface FeatureUsageEnv {
  LICENSE_SERVER_ADMIN_TOKEN: string
  /** Optional override for the api-server base URL (local QA). Defaults to production. */
  WORKER_BASE_URL?: string
}

/**
 * Per-feature usage over the selection's range, from `/admin/feature-usage`. The api-server owns the
 * feature → event mapping and does all the counting; this passes the answer through.
 */
export async function fetchFeatureUsageData(
  env: FeatureUsageEnv,
  selection: DashboardSelection,
): Promise<SourceResult<FeatureUsageData>> {
  const cached = await cacheGet<FeatureUsageData>('feature-usage', selectionCacheKey(selection))
  if (cached) return { ok: true, data: cached }

  // The endpoint takes 7d/30d/90d, exactly the dashboard's ranges.
  try {
    const data = await fetchWorkerEndpoint<FeatureUsageData>(
      env.LICENSE_SERVER_ADMIN_TOKEN,
      `/admin/feature-usage?range=${selection.range}`,
      env.WORKER_BASE_URL,
    )
    await cacheSet('feature-usage', selectionCacheKey(selection), data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Feature usage: ${e instanceof Error ? e.message : String(e)}` }
  }
}
