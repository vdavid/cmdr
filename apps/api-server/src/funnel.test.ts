import { describe, expect, it, vi } from 'vitest'
import { app } from './index'
import { assembleFunnel, buildDateList } from './funnel'

function createMockKv(): KVNamespace {
  return { get: vi.fn(() => null), put: vi.fn() } as unknown as KVNamespace
}

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
  return { writeDataPoint: vi.fn() }
}

/** A D1 mock that returns rows by matching a substring of the SQL to a key in `queryResults`. */
function createMockD1(queryResults: Record<string, unknown[]> = {}): D1Database {
  return {
    prepare: vi.fn((sql: string) => ({
      bind: vi.fn().mockReturnThis(),
      all: vi.fn(() => {
        for (const [key, results] of Object.entries(queryResults)) {
          if (sql.includes(key)) return Promise.resolve({ results })
        }
        return Promise.resolve({ results: [] })
      }),
      run: vi.fn(() => Promise.resolve({ success: true })),
    })),
  } as unknown as D1Database
}

function createMockR2(): R2Bucket {
  return {
    list: vi.fn(() => Promise.resolve({ objects: [], truncated: false, cursor: undefined })),
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
  // No Listmonk config -> signups column is null (unknown), not 0.
}

const authHeaders = { Authorization: 'Bearer test-admin-token-secret' }

describe('GET /admin/funnel (route)', () => {
  it('returns 401 without auth', async () => {
    const res = await app.request('/admin/funnel', {}, baseBindings)
    expect(res.status).toBe(401)
  })

  it('returns 400 for non-numeric days', async () => {
    const res = await app.request('/admin/funnel?days=abc', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(400)
  })

  it('returns 400 for out-of-range days', async () => {
    expect((await app.request('/admin/funnel?days=0', { headers: authHeaders }, baseBindings)).status).toBe(400)
    expect((await app.request('/admin/funnel?days=91', { headers: authHeaders }, baseBindings)).status).toBe(400)
  })

  it('defaults to 30 days and returns one row per day, oldest first', async () => {
    const res = await app.request('/admin/funnel', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(200)
    const body: { date: string }[] = await res.json()
    expect(body).toHaveLength(30)
    // Oldest first, strictly ascending, last is today (UTC).
    for (let i = 1; i < body.length; i++) expect(body[i].date > body[i - 1].date).toBe(true)
    expect(body[body.length - 1].date).toBe(new Date().toISOString().slice(0, 10))
  })

  it('honors ?days=N', async () => {
    const res = await app.request('/admin/funnel?days=7', { headers: authHeaders }, baseBindings)
    const body: unknown[] = await res.json()
    expect(body).toHaveLength(7)
  })

  it('reports signups as null when Listmonk is unconfigured', async () => {
    const res = await app.request('/admin/funnel?days=3', { headers: authHeaders }, baseBindings)
    const body: { newsletterSignups: number | null }[] = await res.json()
    expect(body.every((d) => d.newsletterSignups === null)).toBe(true)
  })
})

describe('buildDateList', () => {
  it('returns N consecutive UTC days ending today, oldest first', () => {
    const now = new Date('2026-06-12T15:30:00Z')
    expect(buildDateList(3, now)).toEqual(['2026-06-10', '2026-06-11', '2026-06-12'])
  })

  it('handles a single day', () => {
    expect(buildDateList(1, new Date('2026-06-12T00:00:01Z'))).toEqual(['2026-06-12'])
  })

  it('crosses a month boundary correctly', () => {
    const now = new Date('2026-07-01T10:00:00Z')
    expect(buildDateList(2, now)).toEqual(['2026-06-30', '2026-07-01'])
  })
})

describe('assembleFunnel', () => {
  const now = new Date('2026-06-12T12:00:00Z')

  it('zero-fills count metrics for days with no rows, but leaves signups null when unconfigured', () => {
    const dates = ['2026-06-11', '2026-06-12']
    const rows = assembleFunnel(dates, [], [], [], [], [], [], null, now)
    expect(rows[0]).toMatchObject({
      date: '2026-06-11',
      downloads: 0,
      downloadsBySource: { website: 0, homebrew: 0, other: 0 },
      downloadsByRef: {},
      downloadsByReferer: {},
      newInstalls: 0,
      dau: 0,
      newsletterSignups: null,
    })
  })

  it('sums downloads across sources and keeps the per-source breakdown', () => {
    const dates = ['2026-06-12']
    const rows = assembleFunnel(
      dates,
      [
        { date: '2026-06-12', source: 'website', count: 5 },
        { date: '2026-06-12', source: 'homebrew', count: 2 },
        { date: '2026-06-12', source: 'other', count: 1 },
      ],
      [],
      [],
      [],
      [],
      [],
      null,
      now,
    )
    expect(rows[0].downloads).toBe(8)
    expect(rows[0].downloadsBySource).toEqual({ website: 5, homebrew: 2, other: 1 })
  })

  it('buckets downloads by ref per day, mapping NULL ref to "(none)"', () => {
    const dates = ['2026-06-11', '2026-06-12']
    const rows = assembleFunnel(
      dates,
      [],
      [
        { date: '2026-06-11', ref: 'hn', count: 4 },
        { date: '2026-06-11', ref: '(none)', count: 2 },
        { date: '2026-06-12', ref: 'reddit', count: 1 },
      ],
      [],
      [],
      [],
      [],
      null,
      now,
    )
    expect(rows[0].downloadsByRef).toEqual({ hn: 4, '(none)': 2 })
    expect(rows[1].downloadsByRef).toEqual({ reddit: 1 })
  })

  it('buckets downloads by referer host per day, mapping NULL referer to "(none)"', () => {
    const dates = ['2026-06-11', '2026-06-12']
    const rows = assembleFunnel(
      dates,
      [],
      [],
      [
        { date: '2026-06-11', referer: 'alternativeto.net', count: 5 },
        { date: '2026-06-11', referer: '(none)', count: 3 },
        { date: '2026-06-12', referer: 'github.com', count: 2 },
      ],
      [],
      [],
      [],
      null,
      now,
    )
    expect(rows[0].downloadsByReferer).toEqual({ 'alternativeto.net': 5, '(none)': 3 })
    expect(rows[1].downloadsByReferer).toEqual({ 'github.com': 2 })
  })

  it('buckets signups per day from the Listmonk map', () => {
    const dates = ['2026-06-11', '2026-06-12']
    const signups = new Map([['2026-06-11', 3]])
    const rows = assembleFunnel(dates, [], [], [], [], [], [], signups, now)
    expect(rows[0].newsletterSignups).toBe(3)
    expect(rows[1].newsletterSignups).toBe(0) // configured but no signups that day -> 0, not null
  })

  it('reports D7 as null for cohorts younger than 8 days', () => {
    // 2026-06-12 is "today"; 2026-06-06 is 6 days old (< 8) -> null.
    const dates = ['2026-06-06']
    const rows = assembleFunnel(dates, [], [], [], [{ date: '2026-06-06', newInstalls: 10 }], [], [], null, now)
    expect(rows[0].d7Retention).toBeNull()
    expect(rows[0].d7Retained).toBeNull()
  })

  it('computes D7 as retained/installs for cohorts at least 8 days old', () => {
    // 2026-06-04 is 8 days before 2026-06-12 -> knowable.
    const dates = ['2026-06-04']
    const rows = assembleFunnel(
      dates,
      [],
      [],
      [],
      [{ date: '2026-06-04', newInstalls: 4 }],
      [],
      [{ cohortDate: '2026-06-04', retained: 1 }],
      null,
      now,
    )
    expect(rows[0].d7Retained).toBe(1)
    expect(rows[0].d7Retention).toBeCloseTo(0.25)
  })

  it('reports D7 retention 0 (not null) for an old cohort with installs but no retained beats', () => {
    const dates = ['2026-06-04']
    const rows = assembleFunnel(dates, [], [], [], [{ date: '2026-06-04', newInstalls: 4 }], [], [], null, now)
    expect(rows[0].d7Retained).toBe(0)
    expect(rows[0].d7Retention).toBe(0)
  })

  it('reports D7 as null for an old day with NO new installs (no cohort to retain)', () => {
    const dates = ['2026-06-04']
    const rows = assembleFunnel(dates, [], [], [], [], [], [], null, now)
    expect(rows[0].newInstalls).toBe(0)
    expect(rows[0].d7Retention).toBeNull()
    expect(rows[0].d7Retained).toBeNull()
  })
})
