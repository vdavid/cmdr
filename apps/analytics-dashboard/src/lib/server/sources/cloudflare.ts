import type { TimeRange, SourceResult } from '../types.js'
import { toSqlInterval } from '../types.js'
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
    CLOUDFLARE_API_TOKEN: string
    CLOUDFLARE_ACCOUNT_ID: string
}

interface AnalyticsEngineResponse {
    data: Array<Record<string, string | number>>
    meta: Array<{ name: string; type: string }>
    rows: number
}

async function querySql(env: CloudflareEnv, sql: string): Promise<AnalyticsEngineResponse> {
    const url = `https://api.cloudflare.com/client/v4/accounts/${env.CLOUDFLARE_ACCOUNT_ID}/analytics_engine/sql`
    const response = await fetch(url, {
        method: 'POST',
        headers: { Authorization: `Bearer ${env.CLOUDFLARE_API_TOKEN}` },
        body: sql,
    })
    if (!response.ok) {
        const text = await response.text()
        throw new Error(`CF Analytics Engine returned ${response.status}: ${text}`)
    }
    return (await response.json()) as AnalyticsEngineResponse
}

export function parseDownloadRows(raw: AnalyticsEngineResponse): DownloadRow[] {
    return raw.data.map((row) => ({
        version: String(row.version ?? row.blob1 ?? ''),
        arch: String(row.arch ?? row.blob2 ?? ''),
        country: String(row.country ?? row.blob3 ?? ''),
        day: String(row.day ?? ''),
        downloads: Number(row.downloads ?? row.count ?? 0),
    }))
}

export function parseUpdateCheckRows(raw: AnalyticsEngineResponse): UpdateCheckRow[] {
    return raw.data.map((row) => ({
        version: String(row.version ?? row.blob1 ?? ''),
        checks: Number(row.checks ?? row.count ?? 0),
    }))
}

export async function fetchCloudflareData(env: CloudflareEnv, range: TimeRange): Promise<SourceResult<CloudflareData>> {
    const cached = await cacheGet<CloudflareData>('cloudflare', range)
    if (cached) return { ok: true, data: cached }

    try {
        const interval = toSqlInterval(range)

        const [downloadsResult, updateChecksResult] = await Promise.all([
            querySql(
                env,
                `SELECT blob1 AS version, blob2 AS arch, blob3 AS country,
                        toDate(timestamp) AS day, SUM(_sample_interval) AS downloads
                 FROM cmdr_downloads
                 WHERE timestamp > NOW() - INTERVAL ${interval}
                 GROUP BY version, arch, country, day
                 ORDER BY day ASC, downloads DESC`
            ),
            querySql(
                env,
                `SELECT blob1 AS version, COUNT(DISTINCT index1) AS checks
                 FROM cmdr_update_checks
                 WHERE timestamp > NOW() - INTERVAL ${interval}
                 GROUP BY version
                 ORDER BY checks DESC`
            ),
        ])

        const data: CloudflareData = {
            downloads: parseDownloadRows(downloadsResult),
            updateChecks: parseUpdateCheckRows(updateChecksResult),
        }
        await cacheSet('cloudflare', range, data)
        return { ok: true, data }
    } catch (e) {
        return { ok: false, error: `Cloudflare: ${e instanceof Error ? e.message : String(e)}` }
    }
}
