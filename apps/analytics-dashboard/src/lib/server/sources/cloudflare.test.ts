import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchCloudflareData, parseDownloadRows, parseUpdateCheckRows } from './cloudflare.js'
import { clearMemoryCache } from '../cache.js'

const mockEnv = {
    CLOUDFLARE_API_TOKEN: 'test-cf-token',
    CLOUDFLARE_ACCOUNT_ID: '6a4433bf11c3cf86feda057f76f47991',
}

const sampleDownloadsResponse = {
    data: [
        { version: '1.2.0', arch: 'aarch64', country: 'US', downloads: 150 },
        { version: '1.2.0', arch: 'x86_64', country: 'DE', downloads: 80 },
        { version: '1.1.0', arch: 'aarch64', country: 'GB', downloads: 45 },
    ],
    meta: [
        { name: 'version', type: 'String' },
        { name: 'arch', type: 'String' },
        { name: 'country', type: 'String' },
        { name: 'downloads', type: 'UInt64' },
    ],
    rows: 3,
}

const sampleUpdateChecksResponse = {
    data: [
        { version: '1.2.0', checks: 500 },
        { version: '1.1.0', checks: 200 },
    ],
    meta: [
        { name: 'version', type: 'String' },
        { name: 'checks', type: 'UInt64' },
    ],
    rows: 2,
}

describe('parseDownloadRows', () => {
    it('parses download data with named columns', () => {
        const rows = parseDownloadRows(sampleDownloadsResponse)
        expect(rows).toHaveLength(3)
        expect(rows[0]).toEqual({ version: '1.2.0', arch: 'aarch64', country: 'US', downloads: 150 })
        expect(rows[2]).toEqual({ version: '1.1.0', arch: 'aarch64', country: 'GB', downloads: 45 })
    })

    it('handles blob-style column names', () => {
        const blobResponse = {
            data: [{ blob1: '1.2.0', blob2: 'aarch64', blob3: 'US', count: 100 }],
            meta: [],
            rows: 1,
        }
        const rows = parseDownloadRows(blobResponse)
        expect(rows[0]).toEqual({ version: '1.2.0', arch: 'aarch64', country: 'US', downloads: 100 })
    })

    it('handles empty data', () => {
        expect(parseDownloadRows({ data: [], meta: [], rows: 0 })).toEqual([])
    })
})

describe('parseUpdateCheckRows', () => {
    it('parses update check data', () => {
        const rows = parseUpdateCheckRows(sampleUpdateChecksResponse)
        expect(rows).toHaveLength(2)
        expect(rows[0]).toEqual({ version: '1.2.0', checks: 500 })
    })
})

describe('fetchCloudflareData', () => {
    beforeEach(() => {
        vi.restoreAllMocks()
        clearMemoryCache()
    })

    it('returns parsed data on success', async () => {
        const fetchMock = vi.fn()
        fetchMock.mockResolvedValueOnce({ ok: true, json: async () => sampleDownloadsResponse })
        fetchMock.mockResolvedValueOnce({ ok: true, json: async () => sampleUpdateChecksResponse })
        vi.stubGlobal('fetch', fetchMock)

        const result = await fetchCloudflareData(mockEnv, '7d')
        expect(result.ok).toBe(true)
        if (!result.ok) return

        expect(result.data.downloads).toHaveLength(3)
        expect(result.data.updateChecks).toHaveLength(2)

        // Verify SQL was sent as POST body
        expect(fetchMock.mock.calls[0][1]?.method).toBe('POST')
        expect(fetchMock.mock.calls[0][1]?.headers).toEqual({
            Authorization: 'Bearer test-cf-token',
        })
        // Verify the SQL references the correct dataset
        const sqlBody = fetchMock.mock.calls[0][1]?.body as string
        expect(sqlBody).toContain('cmdr_downloads')
    })

    it('includes correct interval for different time ranges', async () => {
        const fetchMock = vi.fn()
        fetchMock.mockResolvedValue({ ok: true, json: async () => ({ data: [], meta: [], rows: 0 }) })
        vi.stubGlobal('fetch', fetchMock)

        await fetchCloudflareData(mockEnv, '24h')
        const sql24h = fetchMock.mock.calls[0][1]?.body as string
        expect(sql24h).toContain("'1' DAY")

        fetchMock.mockClear()
        await fetchCloudflareData(mockEnv, '30d')
        const sql30d = fetchMock.mock.calls[0][1]?.body as string
        expect(sql30d).toContain("'30' DAY")
    })

    it('returns error when API fails', async () => {
        vi.stubGlobal(
            'fetch',
            vi.fn().mockResolvedValue({ ok: false, status: 403, text: async () => 'Forbidden' })
        )

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
