import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchUmamiData, parseUmamiStats, parseUmamiMetrics } from './umami.js'
import { clearMemoryCache } from '../cache.js'

const mockEnv = {
  UMAMI_API_URL: 'https://umami.example.com',
  UMAMI_USERNAME: 'testuser',
  UMAMI_PASSWORD: 'testpass',
  UMAMI_WEBSITE_ID: '5ea041ae-b99d-4c31-b031-89c4a0005456',
  UMAMI_BLOG_WEBSITE_ID: '3ee5c901-70bf-4dc4-bd79-bca403db6aca',
}

const sampleStats = {
  pageviews: { value: 1200, prev: 1000 },
  visitors: { value: 450, prev: 400 },
  visits: { value: 600, prev: 550 },
  bounces: { value: 200, prev: 180 },
  totaltime: { value: 86400, prev: 72000 },
}

const sampleMetrics = [
  { x: '/pricing', y: 150 },
  { x: '/features', y: 120 },
  { x: '/', y: 300 },
]

describe('parseUmamiStats', () => {
  it('parses a valid stats response', () => {
    const result = parseUmamiStats(sampleStats)
    expect(result.pageviews.value).toBe(1200)
    expect(result.pageviews.prev).toBe(1000)
    expect(result.visitors.value).toBe(450)
    expect(result.bounces.value).toBe(200)
  })
})

describe('parseUmamiMetrics', () => {
  it('parses a valid metrics response', () => {
    const result = parseUmamiMetrics(sampleMetrics)
    expect(result).toHaveLength(3)
    expect(result[0]).toEqual({ x: '/pricing', y: 150 })
    expect(result[2]).toEqual({ x: '/', y: 300 })
  })

  it('handles an empty array', () => {
    expect(parseUmamiMetrics([])).toEqual([])
  })
})

describe('fetchUmamiData', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    clearMemoryCache()
  })

  it('returns parsed data on success', async () => {
    const fetchMock = vi.fn()

    // Auth response
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ token: 'test-jwt-token' }),
    })

    // 6 parallel requests: blog stats, website stats, referrers, pages, countries, events
    for (let i = 0; i < 6; i++) {
      fetchMock.mockResolvedValueOnce({
        ok: true,
        json: async () => (i < 2 ? sampleStats : sampleMetrics),
      })
    }

    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchUmamiData(mockEnv, '7d')
    expect(result.ok).toBe(true)
    if (!result.ok) return

    expect(result.data.blog.pageviews.value).toBe(1200)
    expect(result.data.website.visitors.value).toBe(450)
    expect(result.data.websitePages).toHaveLength(3)

    // Verify auth was called first
    expect(fetchMock.mock.calls[0][0]).toBe('https://umami.example.com/api/auth/login')
    expect(fetchMock.mock.calls[0][1]?.method).toBe('POST')

    // Verify stats calls used the token
    expect(fetchMock.mock.calls[1][1]?.headers).toEqual({ Authorization: 'Bearer test-jwt-token' })
  })

  it('returns error when auth fails', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({ ok: false, status: 401 }))

    const result = await fetchUmamiData(mockEnv, '7d')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('Umami')
    expect(result.error).toContain('401')
  })

  it('returns error when stats endpoint fails', async () => {
    const fetchMock = vi.fn()
    fetchMock.mockResolvedValueOnce({ ok: true, json: async () => ({ token: 'tok' }) })
    fetchMock.mockResolvedValueOnce({ ok: false, status: 500 })
    // The other parallel requests also need to resolve for Promise.all to work,
    // but the first rejection will be caught
    for (let i = 0; i < 5; i++) {
      fetchMock.mockResolvedValueOnce({ ok: true, json: async () => sampleStats })
    }

    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchUmamiData(mockEnv, '30d')
    expect(result.ok).toBe(false)
  })

  it('returns error on network failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValueOnce(new Error('Network error')))

    const result = await fetchUmamiData(mockEnv, '24h')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('Network error')
  })
})
