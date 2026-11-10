import type { DashboardSelection, SourceResult, TimeRange } from '../types.js'
import { rangeDays, selectionCacheKey } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerEndpoint } from './worker-endpoint.js'

export interface DownloadRow {
  version: string
  arch: string
  country: string
  /** 'website' | 'homebrew' | 'other'. See the `/download` handler in the api-server. */
  source: string
  day: string
  /** Raw download requests (one row per hit). */
  downloads: number
  /** Distinct same-day downloaders (deduped by daily-salted hashed IP). Always <= `downloads`. */
  uniqueDownloads: number
}

/** One day of true daily-active data from the heartbeat: distinct installs and how long the app ran. */
export interface HeartbeatDauRow {
  date: string
  dau: number
  /**
   * App hours that day, summed over installs: each beat's reported uptime, and one hour for a beat
   * from a build too old to report it (those beat hourly). The engagement signal, independent of
   * how often the app beats. Fractional.
   */
  appHours: number
}

/** One day of update-check activity: distinct update-enabled installs that ran while on `version`. */
export interface UpdateActivityRow {
  day: string
  version: string
  updaters: number
}

export interface CloudflareData {
  downloads: DownloadRow[]
  heartbeatDau: HeartbeatDauRow[]
  updateActivity: UpdateActivityRow[]
}

interface CloudflareEnv {
  LICENSE_SERVER_ADMIN_TOKEN: string
  /** Optional override for the api-server base URL (local QA). Defaults to production. */
  WORKER_BASE_URL?: string
}

/**
 * The downloads/update-activity range param. Those two endpoints take 24h/7d/30d/all (no 90d), so 90d asks
 * for `all` and `sinceDay` trims the rows to the window.
 */
const downloadRangeMap: Record<TimeRange, string> = {
  '7d': '7d',
  '30d': '30d',
  '90d': 'all',
}

/** The first UTC day a range covers (`days` days ending today), for trimming an `all` answer. */
export function sinceDay(range: TimeRange, now: Date): string {
  const today = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())
  return new Date(today - (rangeDays[range] - 1) * 86_400_000).toISOString().slice(0, 10)
}

interface WorkerDownloadRow {
  date: string
  version: string
  arch: string
  country: string
  source: string
  count: number
  uniqueCount: number
}

interface WorkerUpdateActivityRow {
  date: string
  version: string
  count: number
}

export function parseDownloadRows(raw: WorkerDownloadRow[]): DownloadRow[] {
  return raw.map((row) => ({
    version: row.version,
    arch: row.arch,
    country: row.country,
    source: row.source,
    day: row.date,
    downloads: row.count,
    uniqueDownloads: row.uniqueCount,
  }))
}

/** Passes through the worker's per-day DAU rows (already `{ date, dau, appHours }`), sorted oldest-first. */
export function parseHeartbeatDauRows(raw: HeartbeatDauRow[]): HeartbeatDauRow[] {
  return [...raw]
    .map((row) => ({ date: row.date, dau: row.dau, appHours: row.appHours }))
    .sort((a, b) => a.date.localeCompare(b.date))
}

export function parseUpdateActivityRows(raw: WorkerUpdateActivityRow[]): UpdateActivityRow[] {
  return raw.map((row) => ({ day: row.date, version: row.version, updaters: row.count }))
}

export async function fetchCloudflareData(
  env: CloudflareEnv,
  selection: DashboardSelection,
): Promise<SourceResult<CloudflareData>> {
  const cached = await cacheGet<CloudflareData>('cloudflare', selectionCacheKey(selection))
  if (cached) return { ok: true, data: cached }

  const range = selection.range
  // Only the `all` answer needs trimming; the worker windows the others itself.
  const since = downloadRangeMap[range] === 'all' ? sinceDay(range, new Date()) : ''
  try {
    const [downloadsRaw, heartbeatDauRaw, updateActivityRaw] = await Promise.all([
      fetchWorkerEndpoint<WorkerDownloadRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/downloads?range=${downloadRangeMap[range]}`,
        env.WORKER_BASE_URL,
      ),
      fetchWorkerEndpoint<HeartbeatDauRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/heartbeat-dau?range=${range}`,
        env.WORKER_BASE_URL,
      ),
      fetchWorkerEndpoint<WorkerUpdateActivityRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/update-activity?range=${downloadRangeMap[range]}`,
        env.WORKER_BASE_URL,
      ),
    ])

    const data: CloudflareData = {
      downloads: parseDownloadRows(downloadsRaw).filter((r) => r.day >= since),
      heartbeatDau: parseHeartbeatDauRows(heartbeatDauRaw),
      updateActivity: parseUpdateActivityRows(updateActivityRaw).filter((r) => r.day >= since),
    }
    await cacheSet('cloudflare', selectionCacheKey(selection), data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Cloudflare: ${e instanceof Error ? e.message : String(e)}` }
  }
}
