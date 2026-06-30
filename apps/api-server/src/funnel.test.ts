import { describe, expect, it, vi } from 'vitest'
import { app } from './index'
import { assembleFunnel, buildDateList, classifyUaFamily } from './funnel'

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
    const rows = assembleFunnel(dates, [], [], [], [], [], [], [], null, now)
    expect(rows[0]).toMatchObject({
      date: '2026-06-11',
      downloads: 0,
      downloadsBySource: { website: 0, homebrew: 0, other: 0 },
      downloadsByRef: {},
      downloadsByReferer: {},
      downloadsByUaFamily: { human: 0, bot: 0, unknown: 0 },
      humanInstalls: 0,
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
      [],
      null,
      now,
    )
    expect(rows[0].downloadsByReferer).toEqual({ 'alternativeto.net': 5, '(none)': 3 })
    expect(rows[1].downloadsByReferer).toEqual({ 'github.com': 2 })
  })

  it('buckets downloads by UA family per day and derives humanInstalls = human + unknown', () => {
    const dates = ['2026-06-11', '2026-06-12']
    const rows = assembleFunnel(
      dates,
      [],
      [],
      [],
      [
        // 2026-06-11: a Mac browser (human), a Windows bot (excluded), and a NULL UA (unknown, kept).
        { date: '2026-06-11', userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', count: 4 },
        { date: '2026-06-11', userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)', count: 6 },
        { date: '2026-06-11', userAgent: null, count: 2 },
        // 2026-06-12: a Homebrew install (human) and a Linux bot (excluded).
        { date: '2026-06-12', userAgent: 'Homebrew/4.1.0 (Macintosh; arm64)', count: 3 },
        { date: '2026-06-12', userAgent: 'Mozilla/5.0 (X11; Linux x86_64)', count: 5 },
      ],
      [],
      [],
      [],
      null,
      now,
    )
    expect(rows[0].downloadsByUaFamily).toEqual({ human: 4, bot: 6, unknown: 2 })
    expect(rows[0].humanInstalls).toBe(6) // 4 human + 2 unknown; the 6 Windows bots are excluded
    expect(rows[1].downloadsByUaFamily).toEqual({ human: 3, bot: 5, unknown: 0 })
    expect(rows[1].humanInstalls).toBe(3)
  })

  it('buckets signups per day from the Listmonk map', () => {
    const dates = ['2026-06-11', '2026-06-12']
    const signups = new Map([['2026-06-11', 3]])
    const rows = assembleFunnel(dates, [], [], [], [], [], [], [], signups, now)
    expect(rows[0].newsletterSignups).toBe(3)
    expect(rows[1].newsletterSignups).toBe(0) // configured but no signups that day -> 0, not null
  })

  it('reports D7 as null for cohorts younger than 8 days', () => {
    // 2026-06-12 is "today"; 2026-06-06 is 6 days old (< 8) -> null.
    const dates = ['2026-06-06']
    const rows = assembleFunnel(dates, [], [], [], [], [{ date: '2026-06-06', newInstalls: 10 }], [], [], null, now)
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
    const rows = assembleFunnel(dates, [], [], [], [], [{ date: '2026-06-04', newInstalls: 4 }], [], [], null, now)
    expect(rows[0].d7Retained).toBe(0)
    expect(rows[0].d7Retention).toBe(0)
  })

  it('reports D7 as null for an old day with NO new installs (no cohort to retain)', () => {
    const dates = ['2026-06-04']
    const rows = assembleFunnel(dates, [], [], [], [], [], [], [], null, now)
    expect(rows[0].newInstalls).toBe(0)
    expect(rows[0].d7Retention).toBeNull()
    expect(rows[0].d7Retained).toBeNull()
  })
})

describe('classifyUaFamily', () => {
  it('classifies a Mac browser UA as human (Macintosh / Mac OS markers)', () => {
    expect(classifyUaFamily('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15')).toBe('human')
    expect(classifyUaFamily('Mozilla/5.0 (Mac OS X) Safari')).toBe('human')
  })

  it('classifies Homebrew and curl/wget installs as human', () => {
    expect(classifyUaFamily('Homebrew/4.1.0 (Macintosh; arm64)')).toBe('human')
    expect(classifyUaFamily('curl/8.4.0')).toBe('human')
    expect(classifyUaFamily('Wget/1.21.4')).toBe('human')
  })

  it('classifies non-macOS UAs (Windows / Android / Linux / X11) as bot, the provable exclusion', () => {
    expect(classifyUaFamily('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')).toBe('bot')
    expect(classifyUaFamily('Mozilla/5.0 (Linux; Android 13; Pixel 7)')).toBe('bot')
    expect(classifyUaFamily('Mozilla/5.0 (X11; Linux x86_64)')).toBe('bot')
    expect(classifyUaFamily('Mozilla/5.0 (X11; Ubuntu; Linux i686)')).toBe('bot')
  })

  it('classifies a NULL, empty, or unrecognized UA as unknown (never excluded)', () => {
    expect(classifyUaFamily(null)).toBe('unknown')
    expect(classifyUaFamily(undefined)).toBe('unknown')
    expect(classifyUaFamily('')).toBe('unknown')
    expect(classifyUaFamily('SomeRandomFetcher/2.0')).toBe('unknown')
  })

  it('lets a Mac marker win over a non-mac one, so a Mac-claiming UA is never excluded', () => {
    // Contrived, but documents the precedence: human markers are checked first.
    expect(classifyUaFamily('Macintosh; also mentions Windows')).toBe('human')
  })

  it('is case-insensitive', () => {
    expect(classifyUaFamily('MOZILLA/5.0 (MACINTOSH)')).toBe('human')
    expect(classifyUaFamily('mozilla/5.0 (windows nt 10.0)')).toBe('bot')
  })
})
