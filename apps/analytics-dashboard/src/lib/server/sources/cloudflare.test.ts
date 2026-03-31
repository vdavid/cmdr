import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchCloudflareData, parseDownloadRows, parseUpdateCheckRows } from './cloudflare.js'
import { clearMemoryCache } from '../cache.js'

const mockEnv = {
    LICENSE_SERVER_ADMIN_TOKEN: 'test-admin-token',
}

const sampleDownloadsResponse = [
    { date: '2025-03-20', version: '1.2.0', arch: 'aarch64', country: 'US', count: 150 },
    { date: '2025-03-20', version: '1.2.0', arch: 'x86_64', country: 'DE', count: 80 },
    { date: '2025-03-21', version: '1.1.0', arch: 'aarch64', country: 'GB', count: 45 },
]

const sampleActiveUsersResponse = [
    { date: '2025-03-20', version: '1.2.0', arch: 'aarch64', uniqueUsers: 300 },
    { date: '2025-03-20', version: '1.2.0', arch: 'x86_64', uniqueUsers: 200 },
    { date: '2025-03-20', version: '1.1.0', arch: 'aarch64', uniqueUsers: 100 },
]

describe('parseDownloadRows', () => {
    it('maps worker response to DownloadRow format', () => {
        const rows = parseDownloadRows(sampleDownloadsResponse)
        expect(rows).toHaveLength(3)
        expect(rows[0]).toEqual({ version: '1.2.0', arch: 'aarch64', country: 'US', day: '2025-03-20', downloads: 150 })
        expect(rows[2]).toEqual({ version: '1.1.0', arch: 'aarch64', country: 'GB', day: '2025-03-21', downloads: 45 })
    })

    it('handles empty data', () => {
        expect(parseDownloadRows([])).toEqual([])
    })
})

describe('parseUpdateCheckRows', () => {
    it('aggregates active users across architectures by version', () => {
        const rows = parseUpdateCheckRows(sampleActiveUsersResponse)
        expect(rows).toHaveLength(2)
        // 1.2.0: 300 + 200 = 500
        expect(rows[0]).toEqual({ version: '1.2.0', checks: 500 })
        // 1.1.0: 100
        expect(rows[1]).toEqual({ version: '1.1.0', checks: 100 })
    })

    it('sorts by checks descending', () => {
        const rows = parseUpdateCheckRows([
            { date: '2025-03-20', version: '0.1.0', arch: 'aarch64', uniqueUsers: 10 },
            { date: '2025-03-20', version: '0.2.0', arch: 'aarch64', uniqueUsers: 50 },
        ])
        expect(rows[0].version).toBe('0.2.0')
        expect(rows[1].version).toBe('0.1.0')
    })

    it('handles empty data', () => {
        expect(parseUpdateCheckRows([])).toEqual([])
    })
})

describe('fetchCloudflareData', () => {
    beforeEach(() => {
        vi.restoreAllMocks()
        clearMemoryCache()
    })

    it('returns parsed data on success', async () => {
        const fetchMock = vi.fn()
        fetchMock.mockImplementation((url: string) => {
            if (String(url).includes('/admin/downloads')) {
                return Promise.resolve({ ok: true, json: async () => sampleDownloadsResponse })
            }
            if (String(url).includes('/admin/active-users')) {
                return Promise.resolve({ ok: true, json: async () => sampleActiveUsersResponse })
            }
            return Promise.resolve({ ok: false, status: 404, text: async () => 'Not found' })
        })
        vi.stubGlobal('fetch', fetchMock)

        const result = await fetchCloudflareData(mockEnv, '7d')
        expect(result.ok).toBe(true)
        if (!result.ok) return

        expect(result.data.downloads).toHaveLength(3)
        expect(result.data.updateChecks).toHaveLength(2)

        // Verify auth header is sent
        expect(fetchMock.mock.calls[0][1]?.headers).toEqual({
            Authorization: 'Bearer test-admin-token',
        })
    })

    it('uses correct range parameters', async () => {
        const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => [] })
        vi.stubGlobal('fetch', fetchMock)

        await fetchCloudflareData(mockEnv, '24h')
        const downloadUrl = fetchMock.mock.calls[0][0] as string
        const activeUserUrl = fetchMock.mock.calls[1][0] as string
        expect(downloadUrl).toContain('range=24h')
        expect(activeUserUrl).toContain('range=7d') // 24h maps to 7d for active users

        fetchMock.mockClear()
        await fetchCloudflareData(mockEnv, '30d')
        const downloadUrl30 = fetchMock.mock.calls[0][0] as string
        expect(downloadUrl30).toContain('range=30d')
    })

    it('returns error when API fails', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 403, text: async () => 'Forbidden' }))

        const result = await fetchCloudflareData(mockEnv, '7d')
        expect(result.ok).toBe(false)
        if (result.ok) return
        expect(result.error).toContain('Cloudflare')
        expect(result.error).toContain('403')
    })

    it('returns error on network failure', async () => {
        vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('DNS resolution failed')))

        const result = await fetchCloudflareData(mockEnv, '7d')
        expect(result.ok).toBe(false)
        if (result.ok) return
        expect(result.error).toContain('DNS resolution failed')
    })
})
