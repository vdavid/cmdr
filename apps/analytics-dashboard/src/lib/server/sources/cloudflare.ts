import type { TimeRange, SourceResult } from '../types.js'
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

/** One day of true daily-active data from the heartbeat: distinct installs (`dau`) and total beats. */
export interface HeartbeatDauRow {
  date: string
  dau: number
  beats: number
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

/** Maps dashboard TimeRange to worker endpoint range param. Downloads and update-activity share this. */
const downloadRangeMap: Record<TimeRange, string> = {
  '24h': '24h',
  '7d': '7d',
  '30d': '30d',
}

/** The heartbeat-DAU endpoint takes 7d/30d/90d/all; the dashboard's shortest range (24h) maps up to 7d. */
const heartbeatDauRangeMap: Record<TimeRange, string> = {
  '24h': '7d',
  '7d': '7d',
  '30d': '30d',
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

/** Passes through the worker's per-day DAU rows (already `{ date, dau, beats }`), sorted oldest-first. */
export function parseHeartbeatDauRows(raw: HeartbeatDauRow[]): HeartbeatDauRow[] {
  return [...raw]
    .map((row) => ({ date: row.date, dau: row.dau, beats: row.beats }))
    .sort((a, b) => a.date.localeCompare(b.date))
}

export function parseUpdateActivityRows(raw: WorkerUpdateActivityRow[]): UpdateActivityRow[] {
  return raw.map((row) => ({ day: row.date, version: row.version, updaters: row.count }))
}

export async function fetchCloudflareData(env: CloudflareEnv, range: TimeRange): Promise<SourceResult<CloudflareData>> {
  const cached = await cacheGet<CloudflareData>('cloudflare', range)
  if (cached) return { ok: true, data: cached }

  try {
    const [downloadsRaw, heartbeatDauRaw, updateActivityRaw] = await Promise.all([
      fetchWorkerEndpoint<WorkerDownloadRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/downloads?range=${downloadRangeMap[range]}`,
        env.WORKER_BASE_URL,
      ),
      fetchWorkerEndpoint<HeartbeatDauRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/heartbeat-dau?range=${heartbeatDauRangeMap[range]}`,
        env.WORKER_BASE_URL,
      ),
      fetchWorkerEndpoint<WorkerUpdateActivityRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/update-activity?range=${downloadRangeMap[range]}`,
        env.WORKER_BASE_URL,
      ),
    ])

    const data: CloudflareData = {
      downloads: parseDownloadRows(downloadsRaw),
      heartbeatDau: parseHeartbeatDauRows(heartbeatDauRaw),
      updateActivity: parseUpdateActivityRows(updateActivityRaw),
    }
    await cacheSet('cloudflare', range, data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Cloudflare: ${e instanceof Error ? e.message : String(e)}` }
  }
}
