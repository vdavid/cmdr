import { describe, expect, it, vi } from 'vitest'
import { app } from './index'

function createMockKv(): KVNamespace {
  return {
    get: vi.fn(() => null),
    put: vi.fn(),
  } as unknown as KVNamespace
}

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
  return { writeDataPoint: vi.fn() } as unknown as AnalyticsEngineDataset
}

function createMockD1(queryResults: Record<string, unknown[]> = {}): D1Database {
  return {
    prepare: vi.fn((sql: string) => ({
      bind: vi.fn().mockReturnThis(),
      all: vi.fn(() => {
        // Match the query to return appropriate results
        for (const [key, results] of Object.entries(queryResults)) {
          if (sql.includes(key)) {
            return Promise.resolve({ results })
          }
        }
        return Promise.resolve({ results: [] })
      }),
      run: vi.fn(() => Promise.resolve({ success: true })),
    })),
  } as unknown as D1Database
}

const baseBindings = {
  LICENSE_CODES: createMockKv(),
  DEVICE_COUNTS: createMockAnalyticsEngine(),
  TELEMETRY_DB: createMockD1(),
  ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
  RESEND_API_KEY: 'test-resend-key',
  PRODUCT_NAME: 'Cmdr',
  SUPPORT_EMAIL: 'test@example.com',
  ADMIN_API_TOKEN: 'test-admin-token-secret',
}

const authHeaders = { Authorization: 'Bearer test-admin-token-secret' }

describe('GET /admin/downloads', () => {
  it('returns 401 without auth', async () => {
    const res = await app.request('/admin/downloads', {}, baseBindings)
    expect(res.status).toBe(401)
  })

  it('returns 400 for invalid range', async () => {
    const res = await app.request('/admin/downloads?range=99d', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(400)
  })

  it('returns empty array when no data', async () => {
    const res = await app.request('/admin/downloads?range=7d', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(200)
    const body = await res.json()
    expect(body).toEqual([])
  })

  it('returns grouped download data', async () => {
    const mockData = [{ date: '2025-03-20', version: '0.9.0', arch: 'aarch64', country: 'US', count: 5 }]
    const bindings = {
      ...baseBindings,
      TELEMETRY_DB: createMockD1({ downloads: mockData }),
    }

    const res = await app.request('/admin/downloads?range=7d', { headers: authHeaders }, bindings)
    expect(res.status).toBe(200)
    const body = await res.json()
    expect(body).toEqual(mockData)
  })

  it('accepts all valid ranges', async () => {
    for (const range of ['24h', '7d', '30d', 'all']) {
      const res = await app.request(`/admin/downloads?range=${range}`, { headers: authHeaders }, baseBindings)
      expect(res.status).toBe(200)
    }
  })
})

describe('GET /admin/active-users', () => {
  it('returns 401 without auth', async () => {
    const res = await app.request('/admin/active-users', {}, baseBindings)
    expect(res.status).toBe(401)
  })

  it('returns 400 for invalid range', async () => {
    const res = await app.request('/admin/active-users?range=24h', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(400)
  })

  it('returns empty array when no data', async () => {
    const res = await app.request('/admin/active-users?range=7d', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(200)
    const body = await res.json()
    expect(body).toEqual([])
  })

  it('returns active user data with camelCase keys', async () => {
    const mockData = [{ date: '2025-03-20', version: '0.9.0', arch: 'aarch64', uniqueUsers: 42 }]
    const bindings = {
      ...baseBindings,
      TELEMETRY_DB: createMockD1({ daily_active_users: mockData }),
    }

    const res = await app.request('/admin/active-users?range=7d', { headers: authHeaders }, bindings)
    expect(res.status).toBe(200)
    const body = await res.json()
    expect(body).toEqual(mockData)
  })

  it('accepts all valid ranges', async () => {
    for (const range of ['7d', '30d', '90d', 'all']) {
      const res = await app.request(`/admin/active-users?range=${range}`, { headers: authHeaders }, baseBindings)
      expect(res.status).toBe(200)
    }
  })
})

describe('GET /admin/crashes', () => {
  it('returns 401 without auth', async () => {
    const res = await app.request('/admin/crashes', {}, baseBindings)
    expect(res.status).toBe(401)
  })

  it('returns 400 for invalid range', async () => {
    const res = await app.request('/admin/crashes?range=24h', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(400)
  })

  it('returns empty array when no data', async () => {
    const res = await app.request('/admin/crashes?range=7d', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(200)
    const body = await res.json()
    expect(body).toEqual([])
  })

  it('returns crash data grouped by site', async () => {
    const mockData = [
      { date: '2025-03-20', topFunction: 'cmdr::main', signal: 'SIGSEGV', count: 3, versions: '0.8.0,0.9.0' },
    ]
    const bindings = {
      ...baseBindings,
      TELEMETRY_DB: createMockD1({ crash_reports: mockData }),
    }

    const res = await app.request('/admin/crashes?range=7d', { headers: authHeaders }, bindings)
    expect(res.status).toBe(200)
    const body = await res.json()
    expect(body).toEqual(mockData)
  })

  it('accepts all valid ranges', async () => {
    for (const range of ['7d', '30d', '90d', 'all']) {
      const res = await app.request(`/admin/crashes?range=${range}`, { headers: authHeaders }, baseBindings)
      expect(res.status).toBe(200)
    }
  })
})
