import type { SourceResult } from '../types.js'
import { cacheGet, cacheSet } from '../cache.js'

export interface LicenseData {
    totalActivations: number
    activeDevices: number | null
}

interface LicenseEnv {
    LICENSE_SERVER_ADMIN_TOKEN: string
}

const licenseServerUrl = 'https://license.getcmdr.com'

export function parseLicenseStats(raw: unknown): LicenseData {
    const r = raw as { totalActivations: number; activeDevices: number | null }
    return {
        totalActivations: r.totalActivations,
        activeDevices: r.activeDevices,
    }
}

/**
 * Fetches activation and device stats from the license server.
 * Not time-range-dependent (returns current totals).
 * Cached under '30d' since the data changes slowly.
 */
export async function fetchLicenseData(env: LicenseEnv): Promise<SourceResult<LicenseData>> {
    const cached = await cacheGet<LicenseData>('license', '30d')
    if (cached) return { ok: true, data: cached }

    try {
        const response = await fetch(`${licenseServerUrl}/admin/stats`, {
            headers: { Authorization: `Bearer ${env.LICENSE_SERVER_ADMIN_TOKEN}` },
        })

        if (!response.ok) {
            throw new Error(`License server returned ${response.status}`)
        }

        const raw = await response.json()
        const data = parseLicenseStats(raw)
        await cacheSet('license', '30d', data)
        return { ok: true, data }
    } catch (e) {
        return { ok: false, error: `License server: ${e instanceof Error ? e.message : String(e)}` }
    }
}
