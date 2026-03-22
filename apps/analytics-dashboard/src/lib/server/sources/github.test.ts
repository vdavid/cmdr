import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchGitHubData, parseRelease, parseGitHubReleases } from './github.js'
import { clearMemoryCache } from '../cache.js'

const sampleRawRelease = {
    tag_name: 'v1.2.0',
    published_at: '2026-03-10T12:00:00Z',
    assets: [
        { name: 'Cmdr-1.2.0-aarch64.dmg', download_count: 120 },
        { name: 'Cmdr-1.2.0-x86_64.dmg', download_count: 45 },
    ],
}

const sampleRawReleases = [
    sampleRawRelease,
    {
        tag_name: 'v1.1.0',
        published_at: '2026-02-20T12:00:00Z',
        assets: [{ name: 'Cmdr-1.1.0-aarch64.dmg', download_count: 200 }],
    },
]

describe('parseRelease', () => {
    it('parses a release with assets', () => {
        const result = parseRelease(sampleRawRelease)
        expect(result.tagName).toBe('v1.2.0')
        expect(result.publishedAt).toBe('2026-03-10T12:00:00Z')
        expect(result.assets).toHaveLength(2)
        expect(result.assets[0]).toEqual({ name: 'Cmdr-1.2.0-aarch64.dmg', downloadCount: 120 })
        expect(result.totalDownloads).toBe(165)
    })

    it('handles release with no assets', () => {
        const result = parseRelease({ tag_name: 'v0.1.0', published_at: '2025-01-01T00:00:00Z', assets: [] })
        expect(result.assets).toEqual([])
        expect(result.totalDownloads).toBe(0)
    })
})

describe('parseGitHubReleases', () => {
    it('parses multiple releases and sums total', () => {
        const result = parseGitHubReleases(sampleRawReleases)
        expect(result.releases).toHaveLength(2)
        expect(result.totalDownloads).toBe(365) // 120 + 45 + 200
    })

    it('handles empty releases', () => {
        const result = parseGitHubReleases([])
        expect(result.releases).toEqual([])
        expect(result.totalDownloads).toBe(0)
    })
})

describe('fetchGitHubData', () => {
    beforeEach(() => {
        vi.restoreAllMocks()
        clearMemoryCache()
    })

    it('returns parsed data on success', async () => {
        vi.stubGlobal(
            'fetch',
            vi.fn().mockResolvedValue({
                ok: true,
                json: async () => sampleRawReleases,
            })
        )

        const result = await fetchGitHubData({})
        expect(result.ok).toBe(true)
        if (!result.ok) return

        expect(result.data.releases).toHaveLength(2)
        expect(result.data.totalDownloads).toBe(365)
    })

    it('sends auth header when token is provided', async () => {
        const fetchMock = vi.fn().mockResolvedValue({
            ok: true,
            json: async () => [],
        })
        vi.stubGlobal('fetch', fetchMock)

        await fetchGitHubData({ GITHUB_TOKEN: 'ghp_test123' })

        const headers = fetchMock.mock.calls[0][1]?.headers as Record<string, string>
        expect(headers.Authorization).toBe('Bearer ghp_test123')
    })

    it('works without auth token', async () => {
        const fetchMock = vi.fn().mockResolvedValue({
            ok: true,
            json: async () => [],
        })
        vi.stubGlobal('fetch', fetchMock)

        await fetchGitHubData({})

        const headers = fetchMock.mock.calls[0][1]?.headers as Record<string, string>
        expect(headers.Authorization).toBeUndefined()
    })

    it('returns error when API fails', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 403 }))

        const result = await fetchGitHubData({})
        expect(result.ok).toBe(false)
        if (result.ok) return
        expect(result.error).toContain('GitHub')
        expect(result.error).toContain('403')
    })

    it('returns error on network failure', async () => {
        vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('fetch failed')))

        const result = await fetchGitHubData({})
        expect(result.ok).toBe(false)
        if (result.ok) return
        expect(result.error).toContain('fetch failed')
    })
})
