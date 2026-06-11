import type { TimeRange, SourceResult } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'
import { fetchWorkerEndpoint } from './worker-endpoint.js'

export interface DownloadRow {
  version: string
  arch: string
  country: string
  day: string
  downloads: number
}

/** One day of true daily-active data from the heartbeat: distinct installs (`dau`) and total beats. */
export interface HeartbeatDauRow {
  date: string
  dau: number
  beats: number
}

export interface CloudflareData {
  downloads: DownloadRow[]
  heartbeatDau: HeartbeatDauRow[]
}

interface CloudflareEnv {
  LICENSE_SERVER_ADMIN_TOKEN: string
}

/** Maps dashboard TimeRange to worker endpoint range param. */
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
  count: number
}

export function parseDownloadRows(raw: WorkerDownloadRow[]): DownloadRow[] {
  return raw.map((row) => ({
    version: row.version,
    arch: row.arch,
    country: row.country,
    day: row.date,
    downloads: row.count,
  }))
}

/** Passes through the worker's per-day DAU rows (already `{ date, dau, beats }`), sorted oldest-first. */
export function parseHeartbeatDauRows(raw: HeartbeatDauRow[]): HeartbeatDauRow[] {
  return [...raw]
    .map((row) => ({ date: row.date, dau: row.dau, beats: row.beats }))
    .sort((a, b) => a.date.localeCompare(b.date))
}

export async function fetchCloudflareData(env: CloudflareEnv, range: TimeRange): Promise<SourceResult<CloudflareData>> {
  const cached = await cacheGet<CloudflareData>('cloudflare', range)
  if (cached) return { ok: true, data: cached }

  try {
    const [downloadsRaw, heartbeatDauRaw] = await Promise.all([
      fetchWorkerEndpoint<WorkerDownloadRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/downloads?range=${downloadRangeMap[range]}`,
      ),
      fetchWorkerEndpoint<HeartbeatDauRow[]>(
        env.LICENSE_SERVER_ADMIN_TOKEN,
        `/admin/heartbeat-dau?range=${heartbeatDauRangeMap[range]}`,
      ),
    ])

    const data: CloudflareData = {
      downloads: parseDownloadRows(downloadsRaw),
      heartbeatDau: parseHeartbeatDauRows(heartbeatDauRaw),
    }
    await cacheSet('cloudflare', range, data)
    return { ok: true, data }
  } catch (e) {
    return { ok: false, error: `Cloudflare: ${e instanceof Error ? e.message : String(e)}` }
  }
}
