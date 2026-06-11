import { describe, expect, it, vi } from 'vitest'
import { app } from './index'

function createMockKv(): KVNamespace {
  return {
    get: vi.fn(() => null),
    put: vi.fn(),
  } as unknown as KVNamespace
}

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
  return { writeDataPoint: vi.fn() }
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

interface MockR2Object {
  key: string
  customMetadata?: Record<string, string>
  uploaded?: string
  size?: number
}

/** A one-page R2 bucket whose `list` returns the given objects (no pagination). */
function createMockR2(objects: MockR2Object[] = []): R2Bucket {
  return {
    list: vi.fn(() =>
      Promise.resolve({
        objects: objects.map((o) => ({
          key: o.key,
          size: o.size ?? 1000,
          uploaded: new Date(o.uploaded ?? '2026-01-01T00:00:00.000Z'),
          customMetadata: o.customMetadata ?? {},
        })),
        truncated: false,
        cursor: undefined,
      }),
    ),
  } as unknown as R2Bucket
}

const baseBindings = {
  LICENSE_CODES: createMockKv(),
  DEVICE_COUNTS: createMockAnalyticsEngine(),
  TELEMETRY_DB: createMockD1(),
  ERROR_REPORTS_BUCKET: createMockR2(),
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

describe('GET /admin/heartbeat-dau', () => {
  it('returns 401 without auth', async () => {
    const res = await app.request('/admin/heartbeat-dau', {}, baseBindings)
    expect(res.status).toBe(401)
  })

  it('returns 400 for invalid range', async () => {
    const res = await app.request('/admin/heartbeat-dau?range=24h', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(400)
  })

  it('returns empty array when no data', async () => {
    const res = await app.request('/admin/heartbeat-dau?range=7d', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(200)
    const body = await res.json()
    expect(body).toEqual([])
  })

  it('returns per-day dau and beats', async () => {
    const mockData = [
      { date: '2025-03-20', dau: 8, beats: 42 },
      { date: '2025-03-21', dau: 10, beats: 57 },
    ]
    const bindings = {
      ...baseBindings,
      TELEMETRY_DB: createMockD1({ heartbeat: mockData }),
    }

    const res = await app.request('/admin/heartbeat-dau?range=30d', { headers: authHeaders }, bindings)
    expect(res.status).toBe(200)
    const body = await res.json()
    expect(body).toEqual(mockData)
  })

  it('counts distinct anal_id for dau and all rows for beats', async () => {
    const captured: string[] = []
    const db = {
      prepare: vi.fn((sql: string) => {
        captured.push(sql)
        return {
          bind: vi.fn().mockReturnThis(),
          all: vi.fn(() => Promise.resolve({ results: [] })),
          run: vi.fn(() => Promise.resolve({ success: true })),
        }
      }),
    } as unknown as D1Database

    await app.request('/admin/heartbeat-dau?range=7d', { headers: authHeaders }, { ...baseBindings, TELEMETRY_DB: db })

    const sql = captured[0]
    expect(sql).toContain('COUNT(DISTINCT anal_id)')
    expect(sql).toContain('COUNT(*)')
    expect(sql).toContain('date(created_at)')
    expect(sql).toContain('FROM heartbeat')
  })

  it('applies the range filter to the where clause', async () => {
    const captured: string[] = []
    const db = {
      prepare: vi.fn((sql: string) => {
        captured.push(sql)
        return {
          bind: vi.fn().mockReturnThis(),
          all: vi.fn(() => Promise.resolve({ results: [] })),
          run: vi.fn(() => Promise.resolve({ success: true })),
        }
      }),
    } as unknown as D1Database
    const bindings = { ...baseBindings, TELEMETRY_DB: db }

    await app.request('/admin/heartbeat-dau?range=7d', { headers: authHeaders }, bindings)
    expect(captured[0]).toContain("datetime('now', '-7 days')")

    captured.length = 0
    await app.request('/admin/heartbeat-dau?range=all', { headers: authHeaders }, bindings)
    expect(captured[0]).not.toContain('WHERE')
  })

  it('accepts all valid ranges', async () => {
    for (const range of ['7d', '30d', '90d', 'all']) {
      const res = await app.request(`/admin/heartbeat-dau?range=${range}`, { headers: authHeaders }, baseBindings)
      expect(res.status).toBe(200)
    }
  })
})

describe('GET /admin/feedback', () => {
  it('returns 401 without auth', async () => {
    const res = await app.request('/admin/feedback', {}, baseBindings)
    expect(res.status).toBe(401)
  })

  it('returns 400 for invalid range', async () => {
    const res = await app.request('/admin/feedback?range=24h', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(400)
  })

  it('returns empty array when no data', async () => {
    const res = await app.request('/admin/feedback?range=7d', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual([])
  })

  it('returns feedback rows with camelCase keys and the reply-to email', async () => {
    const mockData = [
      {
        id: 2,
        createdAt: '2026-06-10 09:00:00',
        feedback: 'Love the speed!',
        email: 'tester@example.com',
        appVersion: '0.22.0',
        osVersion: 'macOS 15.5',
        buildMode: 'release',
      },
    ]
    const bindings = { ...baseBindings, TELEMETRY_DB: createMockD1({ feedback: mockData }) }
    const res = await app.request('/admin/feedback?range=30d', { headers: authHeaders }, bindings)
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual(mockData)
  })

  it('accepts all valid ranges', async () => {
    for (const range of ['7d', '30d', '90d', 'all']) {
      const res = await app.request(`/admin/feedback?range=${range}`, { headers: authHeaders }, baseBindings)
      expect(res.status).toBe(200)
    }
  })
})

describe('GET /admin/error-reports', () => {
  it('returns 401 without auth', async () => {
    const res = await app.request('/admin/error-reports', {}, baseBindings)
    expect(res.status).toBe(401)
  })

  it('returns 400 for invalid range', async () => {
    const res = await app.request('/admin/error-reports?range=24h', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(400)
  })

  it('returns empty array when the bucket is empty', async () => {
    const res = await app.request('/admin/error-reports?range=7d', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual([])
  })

  it('maps each bundle from its key date and custom metadata', async () => {
    const bindings = {
      ...baseBindings,
      ERROR_REPORTS_BUCKET: createMockR2([
        {
          key: 'error-reports/prod/2026-06-10/ERR-ABCDE-uuid.zip',
          customMetadata: {
            id: 'ERR-ABCDE',
            kind: 'auto',
            appVersion: '0.22.0',
            osVersion: 'macOS 15.5',
            arch: 'aarch64',
            generatedAt: '2026-06-10T08:00:00.000Z',
          },
        },
      ]),
    }
    const res = await app.request('/admin/error-reports?range=all', { headers: authHeaders }, bindings)
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual([
      {
        id: 'ERR-ABCDE',
        kind: 'auto',
        appVersion: '0.22.0',
        osVersion: 'macOS 15.5',
        arch: 'aarch64',
        date: '2026-06-10',
        generatedAt: '2026-06-10T08:00:00.000Z',
      },
    ])
  })

  it('excludes bundles older than the range window', async () => {
    const bindings = {
      ...baseBindings,
      ERROR_REPORTS_BUCKET: createMockR2([
        { key: 'error-reports/prod/2020-01-01/ERR-OLD00-uuid.zip', customMetadata: { id: 'ERR-OLD00', kind: 'user' } },
      ]),
    }
    const res = await app.request('/admin/error-reports?range=7d', { headers: authHeaders }, bindings)
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual([])
  })

  it('lists the prod prefix only', async () => {
    const listSpy = vi.fn().mockResolvedValue({ objects: [], truncated: false, cursor: undefined })
    const r2 = { list: listSpy } as unknown as R2Bucket
    await app.request(
      '/admin/error-reports?range=7d',
      { headers: authHeaders },
      { ...baseBindings, ERROR_REPORTS_BUCKET: r2 },
    )
    expect(listSpy).toHaveBeenCalledWith(
      expect.objectContaining({ prefix: 'error-reports/prod/', include: ['customMetadata'] }),
    )
  })

  it('accepts all valid ranges', async () => {
    for (const range of ['7d', '30d', '90d', 'all']) {
      const res = await app.request(`/admin/error-reports?range=${range}`, { headers: authHeaders }, baseBindings)
      expect(res.status).toBe(200)
    }
  })
})
