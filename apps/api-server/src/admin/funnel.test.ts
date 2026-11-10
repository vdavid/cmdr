import { describe, expect, it, vi } from 'vitest'
import { app } from '../index'
import { assembleFunnel, buildDateList, normalizeCountry, parseDays, type FunnelRows } from './funnel'
import { classifyUaFamily, resolveUaFamily } from '../user-agent'

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
  SMTP2GO_API_KEY: 'test-smtp2go-key',
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
    expect((await app.request('/admin/funnel?days=401', { headers: authHeaders }, baseBindings)).status).toBe(400)
  })

  it('accepts up to 400 days', async () => {
    const res = await app.request('/admin/funnel?days=400', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(200)
    expect(await res.json<unknown[]>()).toHaveLength(400)
  })

  it('returns 400 for a country that is not an ISO-2 code', async () => {
    for (const country of ['SWE', 'S', '12', '(unknown)']) {
      const res = await app.request(`/admin/funnel?country=${country}`, { headers: authHeaders }, baseBindings)
      expect(res.status).toBe(400)
    }
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

  it('reports signups as null under a country filter, without asking Listmonk', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    try {
      const bindings = {
        ...baseBindings,
        LISTMONK_API_URL: 'https://listmonk.example',
        LISTMONK_API_USER: 'agent',
        LISTMONK_API_TOKEN: 'token',
      }
      const res = await app.request('/admin/funnel?days=3&country=se', { headers: authHeaders }, bindings)
      expect(res.status).toBe(200)
      const body: { newsletterSignups: number | null }[] = await res.json()
      expect(body.every((d) => d.newsletterSignups === null)).toBe(true)
      expect(fetchMock).not.toHaveBeenCalled()
    } finally {
      vi.unstubAllGlobals()
    }
  })
})

describe('GET /admin/funnel/countries (route)', () => {
  it('returns 401 without auth', async () => {
    const res = await app.request('/admin/funnel/countries', {}, baseBindings)
    expect(res.status).toBe(401)
  })

  it('returns 400 for out-of-range days', async () => {
    const res = await app.request('/admin/funnel/countries?days=401', { headers: authHeaders }, baseBindings)
    expect(res.status).toBe(400)
  })

  it('returns one entry per country, unknowns folded together', async () => {
    const bindings = {
      ...baseBindings,
      TELEMETRY_DB: createMockD1({
        'FROM downloads': [
          { country: 'SE', uaFamily: 'human', userAgent: null, count: 3 },
          { country: 'unknown', uaFamily: 'human', userAgent: null, count: 1 },
          { country: 'XX', uaFamily: 'bot', userAgent: null, count: 2 },
        ],
      }),
    }
    const res = await app.request('/admin/funnel/countries?days=7', { headers: authHeaders }, bindings)
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual([
      { country: 'SE', serverDownloads: 3, humanInstalls: 3, newInstalls: 0, d7Cohort: 0, d7Retained: 0 },
      { country: '(unknown)', serverDownloads: 3, humanInstalls: 1, newInstalls: 0, d7Cohort: 0, d7Retained: 0 },
    ])
  })
})

describe('parseDays', () => {
  it('defaults to 30 and accepts 1..400', () => {
    expect(parseDays(undefined)).toBe(30)
    expect(parseDays('1')).toBe(1)
    expect(parseDays('400')).toBe(400)
  })

  it('rejects anything that is not a whole number in range', () => {
    for (const raw of ['0', '401', '7abc', '-3', '1.5', '']) expect(parseDays(raw)).toBeNull()
  })
})

describe('normalizeCountry', () => {
  it('keeps an ISO-2 code and folds everything else into "(unknown)"', () => {
    expect(normalizeCountry('SE')).toBe('SE')
    for (const raw of [null, 'unknown', 'XX', 'T1', 'se', 'SWE']) expect(normalizeCountry(raw)).toBe('(unknown)')
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

/** Empty row sets, with `rows` overriding the ones a test cares about. */
function funnelRows(rows: Partial<FunnelRows> = {}): FunnelRows {
  return {
    downloadsBySource: [],
    downloadsByRef: [],
    downloadsByReferer: [],
    downloadsByUa: [],
    installsByChannel: [],
    dau: [],
    retention: [],
    ...rows,
  }
}

/** `count` installs on `date` with no channel: what every pre-field install looks like. */
function installs(date: string, count: number): Pick<FunnelRows, 'installsByChannel'> {
  return { installsByChannel: [{ date, channel: null, count }] }
}

describe('assembleFunnel', () => {
  const now = new Date('2026-06-12T12:00:00Z')

  it('zero-fills count metrics for days with no rows, but leaves signups null when unconfigured', () => {
    const dates = ['2026-06-11', '2026-06-12']
    const rows = assembleFunnel(dates, funnelRows(), null, now)
    expect(rows[0]).toMatchObject({
      date: '2026-06-11',
      downloads: 0,
      downloadsBySource: { website: 0, homebrew: 0, other: 0 },
      downloadsByRef: {},
      downloadsByReferer: {},
      downloadsByUaFamily: { human: 0, bot: 0, unknown: 0 },
      humanInstalls: 0,
      newInstalls: 0,
      newInstallsByChannel: { homebrew: 0, direct: 0, unknown: 0 },
      dau: 0,
      retentionSoFar: null,
      newsletterSignups: null,
    })
  })

  it('sums downloads across sources and keeps the per-source breakdown', () => {
    const rows = assembleFunnel(
      ['2026-06-12'],
      funnelRows({
        downloadsBySource: [
          { date: '2026-06-12', source: 'website', count: 5 },
          { date: '2026-06-12', source: 'homebrew', count: 2 },
          { date: '2026-06-12', source: 'other', count: 1 },
        ],
      }),
      null,
      now,
    )
    expect(rows[0].downloads).toBe(8)
    expect(rows[0].downloadsBySource).toEqual({ website: 5, homebrew: 2, other: 1 })
  })

  it('buckets downloads by ref per day, mapping NULL ref to "(none)"', () => {
    const rows = assembleFunnel(
      ['2026-06-11', '2026-06-12'],
      funnelRows({
        downloadsByRef: [
          { date: '2026-06-11', ref: 'hn', count: 4 },
          { date: '2026-06-11', ref: '(none)', count: 2 },
          { date: '2026-06-12', ref: 'reddit', count: 1 },
        ],
      }),
      null,
      now,
    )
    expect(rows[0].downloadsByRef).toEqual({ hn: 4, '(none)': 2 })
    expect(rows[1].downloadsByRef).toEqual({ reddit: 1 })
  })

  it('buckets downloads by referer host per day, mapping NULL referer to "(none)"', () => {
    const rows = assembleFunnel(
      ['2026-06-11', '2026-06-12'],
      funnelRows({
        downloadsByReferer: [
          { date: '2026-06-11', referer: 'alternativeto.net', count: 5 },
          { date: '2026-06-11', referer: '(none)', count: 3 },
          { date: '2026-06-12', referer: 'github.com', count: 2 },
        ],
      }),
      null,
      now,
    )
    expect(rows[0].downloadsByReferer).toEqual({ 'alternativeto.net': 5, '(none)': 3 })
    expect(rows[1].downloadsByReferer).toEqual({ 'github.com': 2 })
  })

  it('buckets downloads by UA family per day and derives humanInstalls = human + unknown', () => {
    const rows = assembleFunnel(
      ['2026-06-11', '2026-06-12'],
      funnelRows({
        downloadsByUa: [
          // Legacy rows (no stored family), so the raw UA drives the classification.
          // 2026-06-11: a Mac browser (human), a Windows bot (excluded), and a NULL UA (unknown, kept).
          {
            date: '2026-06-11',
            uaFamily: null,
            userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)',
            count: 4,
          },
          { date: '2026-06-11', uaFamily: null, userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)', count: 6 },
          { date: '2026-06-11', uaFamily: null, userAgent: null, count: 2 },
          // 2026-06-12: a Homebrew install (human) and a Linux bot (excluded).
          { date: '2026-06-12', uaFamily: null, userAgent: 'Homebrew/4.1.0 (Macintosh; arm64)', count: 3 },
          { date: '2026-06-12', uaFamily: null, userAgent: 'Mozilla/5.0 (X11; Linux x86_64)', count: 5 },
        ],
      }),
      null,
      now,
    )
    expect(rows[0].downloadsByUaFamily).toEqual({ human: 4, bot: 6, unknown: 2 })
    expect(rows[0].humanInstalls).toBe(6) // 4 human + 2 unknown; the 6 Windows bots are excluded
    expect(rows[1].downloadsByUaFamily).toEqual({ human: 3, bot: 5, unknown: 0 })
    expect(rows[1].humanInstalls).toBe(3)
  })

  it('keeps the UA-family split intact after the retention sweep clears the raw User-Agents', () => {
    const rows = assembleFunnel(
      ['2026-06-11'],
      // Swept rows: the raw UA is gone, the family recorded at write time carries the day.
      funnelRows({
        downloadsByUa: [
          { date: '2026-06-11', uaFamily: 'human', userAgent: null, count: 4 },
          { date: '2026-06-11', uaFamily: 'bot', userAgent: null, count: 6 },
          { date: '2026-06-11', uaFamily: 'unknown', userAgent: null, count: 2 },
        ],
      }),
      null,
      now,
    )
    expect(rows[0].downloadsByUaFamily).toEqual({ human: 4, bot: 6, unknown: 2 })
    expect(rows[0].humanInstalls).toBe(6)
  })

  it('splits new installs by channel, summing to newInstalls, with NULL and unknown values under unknown', () => {
    const rows = assembleFunnel(
      ['2026-06-12'],
      funnelRows({
        installsByChannel: [
          { date: '2026-06-12', channel: 'homebrew', count: 3 },
          { date: '2026-06-12', channel: 'direct', count: 5 },
          { date: '2026-06-12', channel: null, count: 2 },
          { date: '2026-06-12', channel: 'mas', count: 1 },
        ],
      }),
      null,
      now,
    )
    expect(rows[0].newInstallsByChannel).toEqual({ homebrew: 3, direct: 5, unknown: 3 })
    expect(rows[0].newInstalls).toBe(11)
  })

  it('buckets signups per day from the Listmonk map', () => {
    const signups = new Map([['2026-06-11', 3]])
    const rows = assembleFunnel(['2026-06-11', '2026-06-12'], funnelRows(), signups, now)
    expect(rows[0].newsletterSignups).toBe(3)
    expect(rows[1].newsletterSignups).toBe(0) // configured but no signups that day -> 0, not null
  })

  it('reports D7 as null for cohorts younger than 8 days', () => {
    // 2026-06-12 is "today"; 2026-06-06 is 6 days old (< 8) -> null.
    const rows = assembleFunnel(['2026-06-06'], funnelRows(installs('2026-06-06', 10)), null, now)
    expect(rows[0].d7Retention).toBeNull()
    expect(rows[0].d7Retained).toBeNull()
  })

  it('computes D7 as retained/installs for cohorts at least 8 days old', () => {
    // 2026-06-04 is 8 days before 2026-06-12 -> knowable.
    const rows = assembleFunnel(
      ['2026-06-04'],
      funnelRows({
        ...installs('2026-06-04', 4),
        retention: [{ cohortDate: '2026-06-04', dayOffset: 7, retained: 1 }],
      }),
      null,
      now,
    )
    expect(rows[0].d7Retained).toBe(1)
    expect(rows[0].d7Retention).toBeCloseTo(0.25)
  })

  it('reports D7 retention 0 (not null) for an old cohort with installs but no retained beats', () => {
    const rows = assembleFunnel(['2026-06-04'], funnelRows(installs('2026-06-04', 4)), null, now)
    expect(rows[0].d7Retained).toBe(0)
    expect(rows[0].d7Retention).toBe(0)
  })

  it('reports D7 as null for an old day with NO new installs (no cohort to retain)', () => {
    const rows = assembleFunnel(['2026-06-04'], funnelRows(), null, now)
    expect(rows[0].newInstalls).toBe(0)
    expect(rows[0].d7Retention).toBeNull()
    expect(rows[0].d7Retained).toBeNull()
  })

  it('reports retention so far for cohorts 2..7 days old, at the latest fully passed day', () => {
    // Today is 2026-06-12. A cohort n+1 days old has fully passed day n.
    const dates = ['2026-06-04', '2026-06-05', '2026-06-09', '2026-06-10', '2026-06-11', '2026-06-12']
    const rows = assembleFunnel(
      dates,
      funnelRows({
        installsByChannel: dates.map((date) => ({ date, channel: null, count: 5 })),
        retention: [
          { cohortDate: '2026-06-05', dayOffset: 6, retained: 2 },
          { cohortDate: '2026-06-05', dayOffset: 5, retained: 4 }, // not the latest passed day: ignored
          { cohortDate: '2026-06-10', dayOffset: 1, retained: 3 },
        ],
      }),
      null,
      now,
    )
    expect(rows.map((r) => r.retentionSoFar)).toEqual([
      null, // 8 days old: D7 is knowable, no stand-in needed
      { day: 6, retained: 2 }, // 7 days old
      { day: 2, retained: 0 }, // 3 days old, nobody back on day 2
      { day: 1, retained: 3 }, // 2 days old
      null, // 1 day old: day 1 is still running
      null, // today
    ])
    expect(rows[0].d7Retained).toBe(0)
  })

  it('reports retention so far as null for a young day with no installs', () => {
    const rows = assembleFunnel(['2026-06-09'], funnelRows(), null, now)
    expect(rows[0].retentionSoFar).toBeNull()
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

describe('resolveUaFamily', () => {
  it('trusts the family stored at write time, so a cleared UA keeps its classification', () => {
    expect(resolveUaFamily({ uaFamily: 'human', userAgent: null })).toBe('human')
    expect(resolveUaFamily({ uaFamily: 'bot', userAgent: null })).toBe('bot')
  })

  it('classifies the raw UA for rows written before the column existed', () => {
    expect(resolveUaFamily({ uaFamily: null, userAgent: 'Mozilla/5.0 (Macintosh)' })).toBe('human')
    expect(resolveUaFamily({ uaFamily: null, userAgent: 'Mozilla/5.0 (Windows NT 10.0)' })).toBe('bot')
  })

  it('falls back to unknown for a pre-column row whose UA the sweep already cleared', () => {
    expect(resolveUaFamily({ uaFamily: null, userAgent: null })).toBe('unknown')
  })

  it('ignores an unrecognized stored value rather than trusting it blindly', () => {
    expect(resolveUaFamily({ uaFamily: 'nonsense', userAgent: 'Mozilla/5.0 (Macintosh)' })).toBe('human')
  })
})
