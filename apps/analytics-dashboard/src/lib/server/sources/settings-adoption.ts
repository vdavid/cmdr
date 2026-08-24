import type { DashboardSelection, SourceResult } from '../types.js'
import { selectionCacheKey, selectionToWorkerRange } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerEndpoint } from './worker-endpoint.js'
import {
  resolveAdoption,
  type ConfigShapeInstallRow,
  type ConfigShapeValueRow,
  type SettingsAdoption,
} from '../settings-defaults.js'

interface ConfigShapeResponse {
  installs: ConfigShapeInstallRow[]
  values: ConfigShapeValueRow[]
}

interface SettingsAdoptionEnv {
  LICENSE_SERVER_ADMIN_TOKEN: string
  /** Optional override for the api-server base URL (local QA). Defaults to production. */
  WORKER_BASE_URL?: string
}

/**
 * The config-shape endpoint takes 7d/30d/90d/all. The shortest coarse range maps up to 7d: a single
 * day of heartbeats sees only whoever happened to run the app, which makes a thin denominator look
 * like a real one.
 */
const configShapeRangeMap: Record<'24h' | '7d' | '30d', string> = {
  '24h': '7d',
  '7d': '7d',
  '30d': '30d',
}

/**
 * Settings adoption, resolved server-side.
 *
 * The resolution runs here rather than in a component for two reasons: the per-version defaults
 * manifest is a server-side artifact that has no business in the browser bundle, and the browser
 * only ever needs the aggregate anyway.
 */
export async function fetchSettingsAdoptionData(
  env: SettingsAdoptionEnv,
  selection: DashboardSelection,
): Promise<SourceResult<SettingsAdoption>> {
  const cached = await cacheGet<SettingsAdoption>('settings-adoption', selectionCacheKey(selection))
  if (cached) return { ok: true, data: cached }

  const range = configShapeRangeMap[selectionToWorkerRange(selection)]
  try {
    const raw = await fetchWorkerEndpoint<ConfigShapeResponse>(
      env.LICENSE_SERVER_ADMIN_TOKEN,
      `/admin/config-shape?range=${range}`,
      env.WORKER_BASE_URL,
    )
    const data = resolveAdoption(raw.installs, raw.values)
    await cacheSet('settings-adoption', selectionCacheKey(selection), data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Settings adoption: ${e instanceof Error ? e.message : String(e)}` }
  }
}
