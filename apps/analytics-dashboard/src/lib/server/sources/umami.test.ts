import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  fetchUmamiDailyTraffic,
  fetchUmamiData,
  fetchUmamiEventCountsByCountry,
  parseUmamiStats,
  parseUmamiMetrics,
  umamiDaySpans,
  umamiMaxDaySpanDays,
} from './umami.js'
import { clearMemoryCache } from '../cache.js'

const mockEnv = {
  UMAMI_API_URL: 'https://umami.example.com',
  UMAMI_USERNAME: 'testuser',
  UMAMI_PASSWORD: 'testpass',
  UMAMI_WEBSITE_ID: '5ea041ae-b99d-4c31-b031-89c4a0005456',
  UMAMI_BLOG_WEBSITE_ID: '3ee5c901-70bf-4dc4-bd79-bca403db6aca',
  UMAMI_PRVW_WEBSITE_ID: 'a963eeab-7c38-49cb-a968-83a3c82b31d1',
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
      json: () => Promise.resolve({ token: 'test-jwt-token' }),
    })

    // 10 parallel requests: personalSite stats, website stats, prvw stats, referrers, pages, countries, events, prvwReferrers, prvwPages, personalSiteReferrers
    const rawStats = {
      pageviews: 1200,
      visitors: 450,
      visits: 600,
      bounces: 200,
      totaltime: 86400,
      comparison: { pageviews: 1000, visitors: 400, visits: 550, bounces: 180, totaltime: 72000 },
    }
    for (let i = 0; i < 10; i++) {
      fetchMock.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(i < 3 ? rawStats : sampleMetrics),
      })
    }

    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchUmamiData(mockEnv, { range: '7d', end: null })
    expect(result.ok).toBe(true)
    if (!result.ok) return

    expect(result.data.personalSite.pageviews.value).toBe(1200)
    expect(result.data.website.visitors.value).toBe(450)
    expect(result.data.prvw.pageviews.value).toBe(1200)
    expect(result.data.websitePages).toHaveLength(3)
    expect(result.data.prvwReferrers).toHaveLength(3)
    expect(result.data.personalSiteReferrers).toHaveLength(3)

    // Verify auth was called first
    expect(fetchMock.mock.calls[0][0]).toBe('https://umami.example.com/api/auth/login')
    expect(fetchMock.mock.calls[0][1]?.method).toBe('POST')

    // Verify stats calls used the token
    expect(fetchMock.mock.calls[1][1]?.headers).toEqual({ Authorization: 'Bearer test-jwt-token' })
  })

  it('returns error when auth fails', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({ ok: false, status: 401 }))

    const result = await fetchUmamiData(mockEnv, { range: '7d', end: null })
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('Umami')
    expect(result.error).toContain('401')
  })

  it('returns error when stats endpoint fails', async () => {
    const fetchMock = vi.fn()
    fetchMock.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve({ token: 'tok' }) })
    fetchMock.mockResolvedValueOnce({ ok: false, status: 500 })
    // The other parallel requests also need to resolve for Promise.all to work,
    // but the first rejection will be caught
    for (let i = 0; i < 8; i++) {
      fetchMock.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve(sampleStats) })
    }

    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchUmamiData(mockEnv, { range: '30d', end: null })
    expect(result.ok).toBe(false)
  })

  it('returns error on network failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValueOnce(new Error('Network error')))

    const result = await fetchUmamiData(mockEnv, { range: '7d', end: null })
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('Network error')
  })
})

describe('fetchUmamiEventCountsByCountry', () => {
  const event = (country: string | null) => ({ country, eventName: 'download' })
  const page = (data: unknown[], count: number, pageNumber: number) => ({
    ok: true,
    json: () => Promise.resolve({ data, count, page: pageNumber, pageSize: 2 }),
  })

  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('tallies every event by country across pages, with no-country events under (unknown)', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(page([event('us'), event('DE')], 5, 1))
      .mockResolvedValueOnce(page([event('US'), event(null)], 5, 2))
      .mockResolvedValueOnce(page([event('')], 5, 3))
    vi.stubGlobal('fetch', fetchMock)

    const counts = await fetchUmamiEventCountsByCountry(
      'https://umami.example.com',
      'tok',
      'site',
      { startAt: 1, endAt: 2 },
      'download',
      2,
    )
    expect(Object.fromEntries(counts)).toEqual({ US: 2, DE: 1, '(unknown)': 2 })
    expect(fetchMock).toHaveBeenCalledTimes(3)
    const firstUrl = String(fetchMock.mock.calls[0][0])
    expect(firstUrl).toContain('/api/websites/site/events?')
    expect(firstUrl).toContain('event=download')
    expect(firstUrl).toContain('page=1')
    expect(String(fetchMock.mock.calls[2][0])).toContain('page=3')
  })

  it('throws on an Umami error rather than return a partial tally', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(page([event('US'), event('DE')], 3, 1))
        .mockResolvedValueOnce({ ok: false, status: 500 }),
    )
    await expect(
      fetchUmamiEventCountsByCountry('https://u', 'tok', 'site', { startAt: 1, endAt: 2 }, 'download', 2),
    ).rejects.toThrow('500')
  })
})

describe('the daily series over a long window', () => {
  const day = 86_400_000
  const startAt = Date.UTC(2025, 9, 7)

  it('splits a window into back-to-back spans short enough for `unit=day`', () => {
    expect(umamiDaySpans({ startAt, endAt: startAt + 181 * day - 1 })).toEqual([
      { startAt, endAt: startAt + 181 * day - 1 },
    ])
    expect(umamiDaySpans({ startAt, endAt: startAt + 400 * day - 1 })).toEqual([
      { startAt, endAt: startAt + umamiMaxDaySpanDays * day - 1 },
      { startAt: startAt + umamiMaxDaySpanDays * day, endAt: startAt + 400 * day - 1 },
    ])
  })

  it('asks once per span and merges the days, so Umami never falls back to monthly buckets', async () => {
    const fetchMock = vi.fn((url: string) => {
      const from = Number(new URL(url).searchParams.get('startAt'))
      const iso = new Date(from).toISOString().slice(0, 10)
      const point = (y: number) => [{ x: `${iso} 00:00:00`, y }]
      return Promise.resolve({ ok: true, json: () => Promise.resolve({ sessions: point(1), pageviews: point(2) }) })
    })
    vi.stubGlobal('fetch', fetchMock)
    const traffic = await fetchUmamiDailyTraffic('https://umami.example.com', 'tok', 'site', {
      startAt,
      endAt: startAt + 400 * day - 1,
      country: 'DE',
    })
    vi.unstubAllGlobals()
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect([...traffic.visitors.keys()]).toEqual(['2025-10-07', '2026-04-25'])
    expect(traffic.pageviews.get('2026-04-25')).toBe(2)
  })
})
