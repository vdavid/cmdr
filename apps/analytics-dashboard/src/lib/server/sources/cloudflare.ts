import type { TimeRange, SourceResult } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'

export interface DownloadRow {
    version: string
    arch: string
    country: string
    day: string
    downloads: number
}

export interface UpdateCheckRow {
    version: string
    checks: number
}

export interface CloudflareData {
    downloads: DownloadRow[]
    updateChecks: UpdateCheckRow[]
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

const activeUserRangeMap: Record<TimeRange, string> = {
    '24h': '7d',
    '7d': '7d',
    '30d': '30d',
}

const workerBaseUrl = 'https://api.getcmdr.com'

async function fetchWorkerEndpoint<T>(env: CloudflareEnv, path: string): Promise<T> {
    const response = await fetch(`${workerBaseUrl}${path}`, {
        headers: { Authorization: `Bearer ${env.LICENSE_SERVER_ADMIN_TOKEN}` },
    })
    if (!response.ok) {
        const text = await response.text()
        throw new Error(`Worker ${path} returned ${String(response.status)}: ${text}`)
    }
    return (await response.json()) as T
}

interface WorkerDownloadRow {
    date: string
    version: string
    arch: string
    country: string
    count: number
}

interface WorkerActiveUserRow {
    date: string
    version: string
    arch: string
    uniqueUsers: number
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

/** Aggregates per-arch active user rows into per-version totals. */
export function parseUpdateCheckRows(raw: WorkerActiveUserRow[]): UpdateCheckRow[] {
    const byVersion = new Map<string, number>()
    for (const row of raw) {
        byVersion.set(row.version, (byVersion.get(row.version) ?? 0) + row.uniqueUsers)
    }
    return [...byVersion.entries()]
        .map(([version, checks]) => ({ version, checks }))
        .sort((a, b) => b.checks - a.checks)
}

export async function fetchCloudflareData(env: CloudflareEnv, range: TimeRange): Promise<SourceResult<CloudflareData>> {
    const cached = await cacheGet<CloudflareData>('cloudflare', range)
    if (cached) return { ok: true, data: cached }

    try {
        const [downloadsRaw, activeUsersRaw] = await Promise.all([
            fetchWorkerEndpoint<WorkerDownloadRow[]>(env, `/admin/downloads?range=${downloadRangeMap[range]}`),
            fetchWorkerEndpoint<WorkerActiveUserRow[]>(env, `/admin/active-users?range=${activeUserRangeMap[range]}`),
        ])

        const data: CloudflareData = {
            downloads: parseDownloadRows(downloadsRaw),
            updateChecks: parseUpdateCheckRows(activeUsersRaw),
        }
        await cacheSet('cloudflare', range, data)
        return { ok: true, data }
    } catch (e) {
        return { ok: false, error: `Cloudflare: ${e instanceof Error ? e.message : String(e)}` }
    }
}
