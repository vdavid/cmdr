import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchFunnelCountriesData } from './funnel-countries.js'
import { clearMemoryCache } from '../cache.js'

const env = {
  LICENSE_SERVER_ADMIN_TOKEN: 'admin',
  WORKER_BASE_URL: 'https://worker.example.com',
  UMAMI_API_URL: 'https://umami.example.com',
  UMAMI_USERNAME: 'u',
  UMAMI_PASSWORD: 'p',
  UMAMI_WEBSITE_ID: 'site',
}

const json = (body: unknown, headers: Record<string, string> = {}) => ({
  ok: true,
  headers: new Headers(headers),
  json: () => Promise.resolve(body),
})

/** Answers each upstream by URL, so the test doesn't depend on call order. `window` is the worker's `X-Window`. */
function stubUpstreams(workerCountry: string, window?: string) {
  const fetchMock = vi.fn((url: string) => {
    if (url.endsWith('/api/auth/login')) return Promise.resolve(json({ token: 'tok' }))
    if (url.includes('/metrics?') && url.includes('type=country'))
      return Promise.resolve(
        json([
          { x: 'US', y: 40 },
          { x: 'SE', y: 3 },
        ]),
      )
    if (url.includes('/events?'))
      return Promise.resolve(json({ data: [{ country: 'US' }, { country: 'SE' }, { country: 'US' }], count: 3 }))
    if (url.includes('/admin/funnel/countries'))
      return Promise.resolve(
        json(
          [
            {
              country: workerCountry,
              serverDownloads: 2,
              humanInstalls: 1,
              newInstalls: 1,
              d7Cohort: 1,
              d7Retained: 0,
            },
          ],
          window ? { 'X-Window': window } : {},
        ),
      )
    return Promise.reject(new Error(`unexpected ${url}`))
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

describe('fetchFunnelCountriesData', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    clearMemoryCache()
  })

  it('asks the worker and Umami for the requested window', async () => {
    const fetchMock = stubUpstreams('US')
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(Date.UTC(2026, 10, 10, 12))
    const result = await fetchFunnelCountriesData(env, 7)
    vi.useRealTimers()
    expect(result.ok).toBe(true)
    const urls = fetchMock.mock.calls.map(([url]) => url)
    expect(urls.some((u) => u.includes('/admin/funnel/countries?days=7'))).toBe(true)
    const umamiStart = Date.UTC(2026, 10, 4)
    expect(
      urls.filter((u) => u.includes('/api/websites/')).every((u) => u.includes(`startAt=${String(umamiStart)}`)),
    ).toBe(true)
  })

  it('caches each window on its own, so switching range changes the numbers', async () => {
    stubUpstreams('US')
    const week = await fetchFunnelCountriesData(env, 7)
    stubUpstreams('SE')
    const quarter = await fetchFunnelCountriesData(env, 90)
    expect(week.ok && quarter.ok).toBe(true)
    if (!week.ok || !quarter.ok) return
    const installs = (rows: typeof week.data.rows) => rows.find((r) => r.country === 'SE')?.newInstalls
    expect(installs(week.data.rows)).toBe(0)
    expect(installs(quarter.data.rows)).toBe(1)
  })

  it('asks for a past window ending `end`, Umami included, and keeps the worker side when it confirms the window', async () => {
    const fetchMock = stubUpstreams('US', '2026-10-04..2026-10-10')
    const result = await fetchFunnelCountriesData(env, 7, '2026-10-10')
    if (!result.ok) throw new Error(result.error)
    expect(result.data.workerAvailable).toBe(true)
    const urls = fetchMock.mock.calls.map(([url]) => url)
    expect(urls.some((u) => u.includes('/admin/funnel/countries?days=7&end=2026-10-10'))).toBe(true)
    const umami = urls.filter((u) => u.includes('/api/websites/'))
    expect(umami.every((u) => u.includes(`startAt=${String(Date.UTC(2026, 9, 4))}`))).toBe(true)
    expect(umami.every((u) => u.includes(`endAt=${String(Date.UTC(2026, 9, 11) - 1)}`))).toBe(true)
  })

  it("drops the worker side for a past window when the worker doesn't confirm it (an older worker ignores `end`)", async () => {
    stubUpstreams('US')
    const result = await fetchFunnelCountriesData(env, 7, '2026-10-10')
    if (!result.ok) throw new Error(result.error)
    expect(result.data.workerAvailable).toBe(false)
    expect(result.data.umamiAvailable).toBe(true)
  })

  it('fills clicks for every country from one events listing', async () => {
    const fetchMock = stubUpstreams('US')
    const result = await fetchFunnelCountriesData(env, 30)
    if (!result.ok) throw new Error(result.error)
    const clicks = Object.fromEntries(result.data.rows.map((r) => [r.country, r.clicks]))
    expect(clicks).toEqual({ US: 2, SE: 1 })
    expect(fetchMock.mock.calls.filter(([url]) => url.includes('type=event'))).toHaveLength(0)
  })
})
