import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fetchLicenseData, parseLicenseStats } from './license.js'
import { clearMemoryCache } from '../cache.js'

const mockEnv = { LICENSE_SERVER_ADMIN_TOKEN: 'test-admin-token' }

const sampleResponse = { totalActivations: 42, activeDevices: null }

describe('parseLicenseStats', () => {
  it('parses a response with activeDevices as null', () => {
    const result = parseLicenseStats(sampleResponse)
    expect(result).toEqual({ totalActivations: 42, activeDevices: null })
  })

  it('parses a response with activeDevices as a number', () => {
    const result = parseLicenseStats({ totalActivations: 100, activeDevices: 75 })
    expect(result).toEqual({ totalActivations: 100, activeDevices: 75 })
  })
})

describe('fetchLicenseData', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    clearMemoryCache()
  })

  it('returns parsed data on success', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => sampleResponse,
      }),
    )

    const result = await fetchLicenseData(mockEnv)
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.data.totalActivations).toBe(42)
    expect(result.data.activeDevices).toBeNull()
  })

  it('sends correct auth header', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => sampleResponse,
    })
    vi.stubGlobal('fetch', fetchMock)

    await fetchLicenseData(mockEnv)

    expect(fetchMock.mock.calls[0][0]).toBe('https://api.getcmdr.com/admin/stats')
    expect(fetchMock.mock.calls[0][1]?.headers).toEqual({
      Authorization: 'Bearer test-admin-token',
    })
  })

  it('returns error when server returns 401', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 401 }))

    const result = await fetchLicenseData(mockEnv)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('License server')
    expect(result.error).toContain('401')
  })

  it('returns error when server returns 500', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 500 }))

    const result = await fetchLicenseData(mockEnv)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('500')
  })

  it('returns error on network failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('ECONNREFUSED')))

    const result = await fetchLicenseData(mockEnv)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('ECONNREFUSED')
  })
})
