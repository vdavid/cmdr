import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchPostHogData, parseHogQLResponse } from './posthog.js'
import { clearMemoryCache } from '../cache.js'

const mockEnv = {
    POSTHOG_API_KEY: 'phx_test123',
    POSTHOG_PROJECT_ID: '136072',
    POSTHOG_API_URL: 'https://eu.posthog.com',
}

const sampleHogQLResponse = {
    columns: ['day', 'views'],
    results: [
        ['2026-03-15', 200],
        ['2026-03-16', 180],
        ['2026-03-17', 220],
        ['2026-03-18', 250],
        ['2026-03-19', 190],
        ['2026-03-20', 230],
        ['2026-03-21', 230],
    ] as Array<[string, number]>,
}

describe('parseHogQLResponse', () => {
    it('parses a valid HogQL response', () => {
        const result = parseHogQLResponse(sampleHogQLResponse)
        expect(result).toHaveLength(7)
        expect(result[0]).toEqual({ day: '2026-03-15', views: 200 })
        expect(result[6]).toEqual({ day: '2026-03-21', views: 230 })
    })

    it('returns empty array for missing results', () => {
        expect(parseHogQLResponse({ columns: [], results: undefined } as unknown as typeof sampleHogQLResponse)).toEqual([])
    })
})

describe('fetchPostHogData', () => {
    beforeEach(() => {
        vi.restoreAllMocks()
        clearMemoryCache()
    })

    it('returns parsed data on success', async () => {
        vi.stubGlobal(
            'fetch',
            vi.fn().mockResolvedValue({
                ok: true,
                json: async () => sampleHogQLResponse,
            })
        )

        const result = await fetchPostHogData(mockEnv, '7d')
        expect(result.ok).toBe(true)
        if (!result.ok) return

        expect(result.data.totalPageviews).toBe(1500)
        expect(result.data.dailyPageviews).toHaveLength(7)
    })

    it('sends correct request to PostHog EU query endpoint', async () => {
        const fetchMock = vi.fn().mockResolvedValue({
            ok: true,
            json: async () => sampleHogQLResponse,
        })
        vi.stubGlobal('fetch', fetchMock)

        await fetchPostHogData(mockEnv, '30d')

        const url = fetchMock.mock.calls[0][0] as string
        expect(url).toBe('https://eu.posthog.com/api/projects/136072/query/')

        const options = fetchMock.mock.calls[0][1]
        expect(options?.method).toBe('POST')
        expect(options?.headers).toEqual({
            Authorization: 'Bearer phx_test123',
            'Content-Type': 'application/json',
        })

        const body = JSON.parse(options?.body as string)
        expect(body.query.kind).toBe('HogQLQuery')
        expect(body.query.query).toContain('30 day')
    })

    it('uses correct interval for 24h', async () => {
        const fetchMock = vi.fn().mockResolvedValue({
            ok: true,
            json: async () => sampleHogQLResponse,
        })
        vi.stubGlobal('fetch', fetchMock)

        await fetchPostHogData(mockEnv, '24h')

        const body = JSON.parse(fetchMock.mock.calls[0][1]?.body as string)
        expect(body.query.query).toContain('1 day')
    })

    it('returns error when API fails', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 401 }))

        const result = await fetchPostHogData(mockEnv, '7d')
        expect(result.ok).toBe(false)
        if (result.ok) return
        expect(result.error).toContain('PostHog')
        expect(result.error).toContain('401')
    })

    it('returns error on network failure', async () => {
        vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Connection refused')))

        const result = await fetchPostHogData(mockEnv, '7d')
        expect(result.ok).toBe(false)
        if (result.ok) return
        expect(result.error).toContain('Connection refused')
    })
})
