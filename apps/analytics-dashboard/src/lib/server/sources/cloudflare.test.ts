import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  fetchCloudflareData,
  sinceDay,
  parseDownloadRows,
  parseHeartbeatDauRows,
  parseUpdateActivityRows,
} from './cloudflare.js'
import { clearMemoryCache } from '../cache.js'

const mockEnv = {
  LICENSE_SERVER_ADMIN_TOKEN: 'test-admin-token',
}

const sampleDownloadsResponse = [
  {
    date: '2025-03-20',
    version: '1.2.0',
    arch: 'aarch64',
    country: 'US',
    source: 'website',
    count: 150,
    uniqueCount: 120,
  },
  {
    date: '2025-03-20',
    version: '1.2.0',
    arch: 'x86_64',
    country: 'DE',
    source: 'homebrew',
    count: 80,
    uniqueCount: 75,
  },
  { date: '2025-03-21', version: '1.1.0', arch: 'aarch64', country: 'GB', source: 'other', count: 45, uniqueCount: 40 },
]

const sampleHeartbeatDauResponse = [
  { date: '2025-03-20', dau: 8, appHours: 42 },
  { date: '2025-03-21', dau: 10, appHours: 57.5 },
]

const sampleUpdateActivityResponse = [
  { date: '2025-03-20', version: '1.1.0', count: 30 },
  { date: '2025-03-20', version: '1.2.0', count: 12 },
  { date: '2025-03-21', version: '1.2.0', count: 35 },
]

describe('parseDownloadRows', () => {
  it('maps worker response to DownloadRow format', () => {
    const rows = parseDownloadRows(sampleDownloadsResponse)
    expect(rows).toHaveLength(3)
    expect(rows[0]).toEqual({
      version: '1.2.0',
      arch: 'aarch64',
      country: 'US',
      source: 'website',
      day: '2025-03-20',
      downloads: 150,
      uniqueDownloads: 120,
    })
    expect(rows[2]).toEqual({
      version: '1.1.0',
      arch: 'aarch64',
      country: 'GB',
      source: 'other',
      day: '2025-03-21',
      downloads: 45,
      uniqueDownloads: 40,
    })
  })

  it('handles empty data', () => {
    expect(parseDownloadRows([])).toEqual([])
  })
})

describe('parseUpdateActivityRows', () => {
  it('maps worker response to UpdateActivityRow format', () => {
    const rows = parseUpdateActivityRows(sampleUpdateActivityResponse)
    expect(rows).toHaveLength(3)
    expect(rows[0]).toEqual({ day: '2025-03-20', version: '1.1.0', updaters: 30 })
  })

  it('handles empty data', () => {
    expect(parseUpdateActivityRows([])).toEqual([])
  })
})

describe('parseHeartbeatDauRows', () => {
  it('passes through per-day dau and app hours unchanged', () => {
    const rows = parseHeartbeatDauRows(sampleHeartbeatDauResponse)
    expect(rows).toEqual([
      { date: '2025-03-20', dau: 8, appHours: 42 },
      { date: '2025-03-21', dau: 10, appHours: 57.5 },
    ])
  })

  it('sorts by date oldest-first', () => {
    const rows = parseHeartbeatDauRows([
      { date: '2025-03-21', dau: 10, appHours: 57.5 },
      { date: '2025-03-19', dau: 4, appHours: 20 },
      { date: '2025-03-20', dau: 8, appHours: 42 },
    ])
    expect(rows.map((r) => r.date)).toEqual(['2025-03-19', '2025-03-20', '2025-03-21'])
  })

  it('keeps dau distinct from app hours (the engagement signal)', () => {
    const rows = parseHeartbeatDauRows([{ date: '2025-03-20', dau: 8, appHours: 42.25 }])
    expect(rows[0].dau).toBe(8)
    expect(rows[0].appHours).toBe(42.25)
  })

  it('handles empty data', () => {
    expect(parseHeartbeatDauRows([])).toEqual([])
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
      if (url.includes('/admin/update-activity')) {
        return Promise.resolve({ ok: true, json: () => Promise.resolve(sampleUpdateActivityResponse) })
      }
      if (url.includes('/admin/downloads')) {
        return Promise.resolve({ ok: true, json: () => Promise.resolve(sampleDownloadsResponse) })
      }
      if (url.includes('/admin/heartbeat-dau')) {
        return Promise.resolve({ ok: true, json: () => Promise.resolve(sampleHeartbeatDauResponse) })
      }
      return Promise.resolve({ ok: false, status: 404, text: () => Promise.resolve('Not found') })
    })
    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchCloudflareData(mockEnv, { range: '7d', end: null })
    expect(result.ok).toBe(true)
    if (!result.ok) return

    expect(result.data.downloads).toHaveLength(3)
    expect(result.data.heartbeatDau).toEqual(sampleHeartbeatDauResponse)
    expect(result.data.updateActivity).toHaveLength(3)

    // Verify auth header is sent
    expect(fetchMock.mock.calls[0][1]?.headers).toEqual({
      Authorization: 'Bearer test-admin-token',
    })
  })

  it('uses correct range parameters', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: () => Promise.resolve([]) })
    vi.stubGlobal('fetch', fetchMock)

    await fetchCloudflareData(mockEnv, { range: '7d', end: null })
    const downloadUrl = fetchMock.mock.calls[0][0] as string
    const heartbeatUrl = fetchMock.mock.calls[1][0] as string
    expect(downloadUrl).toContain('range=7d')
    expect(heartbeatUrl).toContain('range=7d')

    fetchMock.mockClear()
    await fetchCloudflareData(mockEnv, { range: '30d', end: null })
    const downloadUrl30 = fetchMock.mock.calls[0][0] as string
    const heartbeatUrl30 = fetchMock.mock.calls[1][0] as string
    expect(downloadUrl30).toContain('range=30d')
    expect(heartbeatUrl30).toContain('range=30d')
  })

  it('asks the downloads endpoints for `all` on 90d (they take no 90d) and trims to the window', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-12T12:00:00Z'))
    const download = (date: string) => ({
      date,
      version: '1',
      arch: 'arm',
      country: 'SE',
      source: 'website',
      count: 1,
      uniqueCount: 1,
    })
    const fetchMock = vi.fn((url: string) =>
      Promise.resolve({
        ok: true,
        json: () =>
          Promise.resolve(
            url.includes('/admin/downloads')
              ? [download('2026-03-14'), download('2026-03-15')]
              : url.includes('/admin/update-activity')
                ? [{ date: '2026-03-14', version: '1', count: 1 }]
                : [],
          ),
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchCloudflareData(mockEnv, { range: '90d', end: null })
    vi.useRealTimers()
    const urls = fetchMock.mock.calls.map((c) => c[0])
    expect(urls.find((u) => u.includes('/admin/downloads'))).toContain('range=all')
    expect(urls.find((u) => u.includes('/admin/heartbeat-dau'))).toContain('range=90d')
    expect(result.ok && result.data.downloads.map((d) => d.day)).toEqual(['2026-03-15'])
    expect(result.ok && result.data.updateActivity).toEqual([])
  })

  it('sinceDay is the first of the range’s days ending today', () => {
    expect(sinceDay('7d', new Date('2026-06-12T23:00:00Z'))).toBe('2026-06-06')
    expect(sinceDay('90d', new Date('2026-06-12T00:00:00Z'))).toBe('2026-03-15')
  })

  it('returns error when API fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({ ok: false, status: 403, text: () => Promise.resolve('Forbidden') }),
    )

    const result = await fetchCloudflareData(mockEnv, { range: '7d', end: null })
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('Cloudflare')
    expect(result.error).toContain('403')
  })

  it('returns error on network failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('DNS resolution failed')))

    const result = await fetchCloudflareData(mockEnv, { range: '7d', end: null })
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('DNS resolution failed')
  })
})
