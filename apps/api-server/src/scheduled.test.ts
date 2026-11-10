import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import {
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  handlePersonalCommentSweep,
  handleSyntheticHeartbeatSweep,
} from './index'
import { syntheticHeartbeatGraceDays } from './scheduled'
import { ERROR_REPORT_PREFIX, EVICTION_MIN_AGE_DAYS, TOTAL_BYTES_KEY } from './telemetry/error-report-eviction'
import { INTAKE_PAUSED_KEY } from './telemetry/error-report-intake'
import { createBaseEnv, createMockD1 } from './cron-test-helpers'
import { createSqliteD1 } from './sqlite-d1-test-helper'
import type { EmailMessage } from './email/send'

/** Fixtures age relative to now, so eviction eligibility never depends on the calendar. */
function daysAgo(days: number): Date {
  return new Date(Date.now() - days * 24 * 60 * 60 * 1000)
}

// Intercept email sends at our one door, so no email leaves the test.
const mockSend = vi.hoisted(() => vi.fn<typeof import('./email/send').sendEmail>(() => Promise.resolve()))
vi.mock('./email/send', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./email/send')>()),
  sendEmail: mockSend,
}))

function lastEmailCall(): EmailMessage {
  const message = mockSend.mock.lastCall?.[1]
  if (!message) throw new Error('No email was sent')
  return message
}

beforeEach(() => {
  mockSend.mockClear()
})

describe('handleDailyAggregation', () => {
  it('aggregates update checks and prunes old data', async () => {
    // Return null for the "already aggregated" check (no existing row)
    const { db, calls } = createMockD1()
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleDailyAggregation(env as never)

    // Should have run: check existing, aggregate insert, prune delete
    const sqlStatements = calls.map((c) => c.sql)
    expect(sqlStatements.some((s) => s.includes('SELECT 1 FROM daily_active_users'))).toBe(true)
    expect(sqlStatements.some((s) => s.includes('INSERT OR IGNORE INTO daily_active_users'))).toBe(true)
    expect(sqlStatements.some((s) => s.includes('DELETE FROM update_checks'))).toBe(true)
  })

  it('skips aggregation when already aggregated (idempotency)', async () => {
    // Return a row for the "already aggregated" check
    const responses = new Map<string, unknown>([['SELECT 1 FROM daily_active_users', { '1': 1 }]])
    const { db, calls } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleDailyAggregation(env as never)

    // Should have checked but not inserted or pruned
    const sqlStatements = calls.map((c) => c.sql)
    expect(sqlStatements.some((s) => s.includes('SELECT 1 FROM daily_active_users'))).toBe(true)
    expect(sqlStatements.some((s) => s.includes('INSERT OR IGNORE INTO daily_active_users'))).toBe(false)
    expect(sqlStatements.some((s) => s.includes('DELETE FROM update_checks'))).toBe(false)
  })
})

describe('handleSyntheticHeartbeatSweep', () => {
  it("deletes the synthetic installs' events, then their heartbeats, both bounded by the grace-period cutoff", async () => {
    const { db, calls } = createMockD1()
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleSyntheticHeartbeatSweep(env as never)

    expect(calls).toHaveLength(3)
    // Events and install summaries first: the classifier reads `heartbeat`, so deleting the beats
    // first would leave them with nothing to identify them by, for good.
    expect(calls[0].sql).toContain('DELETE FROM analytics_event')
    expect(calls[1].sql).toContain('DELETE FROM install_summary')
    expect(calls[2].sql).toContain('DELETE FROM heartbeat')
    // The cutoff has to be BOUND, never inlined: an unbounded delete would take the brand-new
    // installs that have not saved a setting yet. What the predicate does with it is proved
    // against a real SQLite in `synthetic-heartbeats.test.ts`.
    for (const call of calls) {
      expect(call.bindings).toHaveLength(1)
      const cutoffMs = Date.parse(`${String(call.bindings[0])}Z`)
      const expectedMs = Date.now() - syntheticHeartbeatGraceDays * 86_400_000
      expect(Math.abs(cutoffMs - expectedMs)).toBeLessThan(86_400_000)
    }
  })
})

describe('handleRetentionSweep', () => {
  /** The SQL the sweep ran, joined, so a test can assert on statement shape without ordering noise. */
  async function runSweep(): Promise<{ sql: string[]; bindings: unknown[][] }> {
    const { db, calls } = createMockD1()
    const env = createBaseEnv({ TELEMETRY_DB: db })
    await handleRetentionSweep(env as never)
    return { sql: calls.map((c) => c.sql), bindings: calls.map((c) => c.bindings) }
  }

  it('captures per-day unique downloaders before the hashes that produce them are cleared', async () => {
    const { sql } = await runSweep()

    const rollupIndex = sql.findIndex((s) => s.includes('INSERT OR IGNORE INTO downloads_daily_unique'))
    const clearIndex = sql.findIndex((s) => s.includes('UPDATE downloads') && s.includes('hashed_ip = NULL'))

    expect(rollupIndex).toBeGreaterThanOrEqual(0)
    expect(clearIndex).toBeGreaterThanOrEqual(0)
    // Order is the whole point: clearing first would silently zero every historical unique count.
    expect(rollupIndex).toBeLessThan(clearIndex)
  })

  it('clears the identifying download columns, keeping the countable ones', async () => {
    const { sql } = await runSweep()

    const clear = sql.find((s) => s.includes('UPDATE downloads'))
    expect(clear).toContain('hashed_ip = NULL')
    expect(clear).toContain('user_agent = NULL')
    // The row itself survives: version, arch, country, source, ref, and the UA family stay countable.
    expect(clear).not.toContain('DELETE')
    expect(clear).not.toContain('ua_family')
  })

  it('drops the reply-to email and diagnostics id from crash reports, keeping the technical row', async () => {
    const { sql } = await runSweep()

    const clear = sql.find((s) => s.includes('UPDATE crash_reports'))
    expect(clear).toContain('email = NULL')
    expect(clear).toContain('diag_id = NULL')
    expect(clear).not.toContain('backtrace')
    expect(clear).not.toContain('top_function')
  })

  it('deletes heartbeats past their retention window (the one table with a stable install id)', async () => {
    const { sql } = await runSweep()

    expect(sql.some((s) => s.includes('DELETE FROM heartbeat'))).toBe(true)
  })

  it("drops an install's summary once its last beat is gone, after the heartbeat delete", async () => {
    const { sql } = await runSweep()

    const heartbeatIndex = sql.findIndex((s) => s.includes('DELETE FROM heartbeat'))
    const summaryIndex = sql.findIndex((s) => s.includes('DELETE FROM install_summary'))
    expect(summaryIndex).toBeGreaterThan(heartbeatIndex)
    // What the predicate keeps and drops is proved against a real SQLite in `synthetic-heartbeats.test.ts`.
  })

  it('deletes relayed feature events on the same two-year window, by OUR receive time', async () => {
    const { sql, bindings } = await runSweep()

    const index = sql.findIndex((s) => s.includes('DELETE FROM analytics_event'))
    expect(index).toBeGreaterThanOrEqual(0)
    // Never `occurred_at`: that's the client's clock, and a wrong one would keep a row forever.
    expect(sql[index]).toContain('received_at < ?1')
    const heartbeatIndex = sql.findIndex((s) => s.includes('DELETE FROM heartbeat'))
    expect(bindings[index]).toEqual(bindings[heartbeatIndex])
  })

  it('drops the reply-to email from old feedback', async () => {
    const { sql } = await runSweep()

    const clear = sql.find((s) => s.includes('UPDATE feedback'))
    expect(clear).toContain('email = NULL')
    expect(clear).not.toContain('feedback = NULL')
  })

  it('bounds every statement by a cutoff rather than sweeping the whole table', async () => {
    const { sql, bindings } = await runSweep()

    const sweepIndexes = sql
      .map((statement, index) => ({ statement, index }))
      .filter(
        ({ statement }) =>
          statement.includes('UPDATE downloads') ||
          statement.includes('UPDATE crash_reports') ||
          statement.includes('UPDATE feedback') ||
          statement.includes('DELETE FROM heartbeat'),
      )

    expect(sweepIndexes.length).toBe(4)
    for (const { statement, index } of sweepIndexes) {
      expect(statement).toContain('created_at < ?1')
      expect(bindings[index]).toHaveLength(1)
    }
  })

  it('snaps every cutoff to midnight, so a day is never half-swept', async () => {
    const { bindings } = await runSweep()

    for (const bound of bindings.flat()) {
      expect(bound).toMatch(/^\d{4}-\d{2}-\d{2} 00:00:00$/)
    }
  })
})

describe('handlePersonalCommentSweep', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  const configured = { GITHUB_ISSUES_TOKEN: 'ghp_x', GITHUB_ISSUES_REPO: 'getcmdr/cmdr-reports' }

  it('does nothing when the reports repo is not configured', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    await handlePersonalCommentSweep(createBaseEnv({}) as never)

    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('deletes every comment that is due and leaves the rest alone', async () => {
    const yesterday = new Date(Date.now() - 86_400_000).toISOString().slice(0, 10)
    const nextYear = new Date(Date.now() + 365 * 86_400_000).toISOString().slice(0, 10)
    const fetchMock = vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
      if (init?.method === 'DELETE') return Promise.resolve(new Response(null, { status: 204 }))
      return Promise.resolve(
        new Response(
          JSON.stringify([
            { id: 1, body: `<!-- cmdr:personal-data expires=${yesterday} -->\nold`, issue_url: 'https://x/issues/11' },
            { id: 2, body: `<!-- cmdr:personal-data expires=${nextYear} -->\nfresh`, issue_url: 'https://x/issues/12' },
          ]),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
      )
    })
    vi.stubGlobal('fetch', fetchMock)

    await handlePersonalCommentSweep(createBaseEnv(configured) as never)

    const deletes = fetchMock.mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === 'DELETE')
    expect(deletes).toHaveLength(1)
    expect(deletes[0]?.[0]).toContain('/issues/comments/1')
  })

  it('throws so the cron alarm fires when GitHub will not answer', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 503 })))

    await expect(handlePersonalCommentSweep(createBaseEnv(configured) as never)).rejects.toThrow('503')
  })
})

describe('handleDbSizeCheck', () => {
  const MB = 1024 * 1024
  const today = new Date('2026-10-23T00:00:30Z')

  /** A real SQLite reporting `sizeMb` as D1's size, seeded with earlier daily samples. */
  function sizedDb(sizeMb: number, samples: Array<[day: string, sizeMb: number]> = []) {
    const sqlite = createSqliteD1({ sizeAfter: sizeMb * MB })
    for (const [day, mb] of samples) {
      sqlite.sqlite.prepare(`INSERT INTO db_size_samples (day, size_bytes) VALUES (?, ?)`).run(day, mb * MB)
    }
    return sqlite
  }

  it('alerts on the day the size crosses a step of the cap, with the row counts', async () => {
    const { d1 } = sizedDb(4300, [['2026-10-22', 4000]])

    await handleDbSizeCheck(createBaseEnv({ TELEMETRY_DB: d1 }) as never, today)

    expect(mockSend).toHaveBeenCalledOnce()
    const emailCall = lastEmailCall()
    expect(emailCall.subject).toBe("Cmdr: telemetry DB is 4300 MB, 42% of D1's 10240 MB cap")
    expect(emailCall.html).toContain('crash_reports')
  })

  // The regression: past the line, the old check mailed the same warning every single day.
  it('stays quiet the day after, while the size sits between steps', async () => {
    const { d1 } = sizedDb(4400, [['2026-10-22', 4300]])

    await handleDbSizeCheck(createBaseEnv({ TELEMETRY_DB: d1 }) as never, today)

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('stays quiet under the first step, and records the day either way', async () => {
    const { d1, sqlite } = sizedDb(103)

    await handleDbSizeCheck(createBaseEnv({ TELEMETRY_DB: d1 }) as never, today)

    expect(mockSend).not.toHaveBeenCalled()
    expect(sqlite.prepare(`SELECT day, size_bytes FROM db_size_samples`).all()).toEqual([
      { day: '2026-10-23', size_bytes: 103 * MB },
    ])
  })

  it('alerts once with no earlier sample when the size is already past a step', async () => {
    const { d1 } = sizedDb(6200)

    await handleDbSizeCheck(createBaseEnv({ TELEMETRY_DB: d1 }) as never, today)

    expect(mockSend).toHaveBeenCalledOnce()
  })

  it('says how fast it grows and when the cap arrives at that pace', async () => {
    const { d1 } = sizedDb(4300, [
      ['2026-09-23', 3100],
      ['2026-10-22', 4000],
    ])

    await handleDbSizeCheck(createBaseEnv({ TELEMETRY_DB: d1 }) as never, today)

    const { html } = lastEmailCall()
    expect(html).toContain('about 1200 MB a month')
    expect(html).toContain('about five months')
  })

  it('prunes samples older than 400 days', async () => {
    const { d1, sqlite } = sizedDb(103, [
      ['2025-09-01', 10],
      ['2026-10-22', 102],
    ])

    await handleDbSizeCheck(createBaseEnv({ TELEMETRY_DB: d1 }) as never, today)

    const days = sqlite.prepare(`SELECT day FROM db_size_samples ORDER BY day`).all()
    expect(days).toEqual([{ day: '2026-10-22' }, { day: '2026-10-23' }])
  })

  // D1 rejects SQLite's `pragma_page_count` / `pragma_page_size` table-valued functions with
  // `SQLITE_AUTH`, which threw the job every day for months while every mocked test passed. The
  // size has to come from `meta.size_after`, so no statement here may name a pragma.
  it('never queries a pragma for the size', async () => {
    const { db, calls } = createMockD1(new Map(), { sizeAfter: 150 * MB })
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleDbSizeCheck(env as never)

    expect(calls.length).toBeGreaterThan(0)
    for (const call of calls) expect(call.sql.toLowerCase()).not.toContain('pragma')
  })

  // The job runs daily and almost never alerts, so the quiet path skips the per-table counts.
  it('does not count rows when it stays quiet', async () => {
    const { db, calls } = createMockD1(new Map(), { sizeAfter: 50 * MB })
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleDbSizeCheck(env as never)

    expect(calls.some((call) => call.sql.includes('COUNT(*)'))).toBe(false)
  })

  it('skips when CRASH_NOTIFICATION_EMAIL is not set', async () => {
    const { db } = createMockD1(new Map(), { sizeAfter: 150 * MB })
    const env = createBaseEnv({ CRASH_NOTIFICATION_EMAIL: undefined, TELEMETRY_DB: db })

    await handleDbSizeCheck(env as never)

    expect(mockSend).not.toHaveBeenCalled()
  })
})

// -----------------------------------------------------------------------------
// Daily eviction sweep
// -----------------------------------------------------------------------------

interface StubR2Obj {
  key: string
  size: number
  uploaded: Date
}

function createR2Stub(objs: StubR2Obj[]): R2Bucket {
  const store = new Map<string, StubR2Obj>(objs.map((o) => [o.key, o]))
  return {
    list: ({ prefix, cursor }: { prefix?: string; cursor?: string } = {}) => {
      const all = [...store.values()]
        .filter((o) => !prefix || o.key.startsWith(prefix))
        .sort((a, b) => (a.key < b.key ? -1 : 1))
      const pageSize = 1000
      const startIdx = cursor ? parseInt(cursor, 10) : 0
      const slice = all.slice(startIdx, startIdx + pageSize)
      return Promise.resolve({
        objects: slice.map((o) => ({ key: o.key, size: o.size, uploaded: o.uploaded })),
        truncated: startIdx + pageSize < all.length,
        cursor: startIdx + pageSize < all.length ? String(startIdx + pageSize) : undefined,
      })
    },
    delete: (key: string) => {
      store.delete(key)
      return Promise.resolve()
    },
  } as unknown as R2Bucket
}

function createKvStub(initial: Record<string, string> = {}): KVNamespace {
  const store = new Map<string, string>(Object.entries(initial))
  return {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
    delete: (key: string) => {
      store.delete(key)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
}

describe('handleDailyEvictionSweep', () => {
  let originalFetch: typeof fetch
  beforeEach(() => {
    originalFetch = globalThis.fetch
    globalThis.fetch = () => Promise.resolve(new Response(null, { status: 204 }))
  })
  afterEach(() => {
    globalThis.fetch = originalFetch
  })

  it('recomputes total and does not evict when under high watermark', async () => {
    const GB = 1024 ** 3
    const bucket = createR2Stub([
      { key: `${ERROR_REPORT_PREFIX}2026-04-01/a.zip`, size: 2 * GB, uploaded: new Date('2026-04-01') },
    ])
    const kv = createKvStub({ [TOTAL_BYTES_KEY]: '999999999' }) // stale (too high)
    const env = { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv } as never

    await handleDailyEvictionSweep(env)

    // Corrected total
    expect(await kv.get(TOTAL_BYTES_KEY)).toBe(String(2 * GB))
  })

  it('evicts when recomputed total exceeds high watermark', async () => {
    const GB = 1024 ** 3
    // 10 × 1 GB = 10 GB > 8 GB threshold. Ages are relative to now and well past
    // EVICTION_MIN_AGE_DAYS, so every bundle is eligible whenever this suite runs.
    const objs: StubR2Obj[] = Array.from({ length: 10 }, (_, i) => {
      const uploaded = daysAgo(EVICTION_MIN_AGE_DAYS + 10 + i)
      return {
        key: `${ERROR_REPORT_PREFIX}${uploaded.toISOString().slice(0, 10)}/ERR-${String(i).padStart(5, '0')}-u.zip`,
        size: 1 * GB,
        uploaded,
      }
    })
    const bucket = createR2Stub(objs)
    const kv = createKvStub()
    const env = { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv } as never

    await handleDailyEvictionSweep(env)

    // Final recomputed total should be ≤ 6 GB
    const finalTotal = parseInt((await kv.get(TOTAL_BYTES_KEY)) ?? '0', 10)
    expect(finalTotal).toBeLessThanOrEqual(6 * GB)
  })

  it('pauses intake instead of evicting when the bucket is full of young bundles', async () => {
    const GB = 1024 ** 3
    const objs: StubR2Obj[] = Array.from({ length: 10 }, (_, i) => {
      const uploaded = daysAgo(1)
      return {
        key: `${ERROR_REPORT_PREFIX}prod/${uploaded.toISOString().slice(0, 10)}/ERR-${String(i).padStart(5, '0')}-u.zip`,
        size: 1 * GB,
        uploaded,
      }
    })
    const bucket = createR2Stub(objs)
    const kv = createKvStub()
    const env = { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv } as never

    await handleDailyEvictionSweep(env)

    expect(await kv.get(INTAKE_PAUSED_KEY)).not.toBeNull()
    const finalTotal = parseInt((await kv.get(TOTAL_BYTES_KEY)) ?? '0', 10)
    expect(finalTotal).toBe(10 * GB) // nothing deleted
  })

  it('resumes a paused intake once the bucket is back under the low watermark', async () => {
    const GB = 1024 ** 3
    const bucket = createR2Stub([
      { key: `${ERROR_REPORT_PREFIX}prod/2026-04-01/a.zip`, size: 1 * GB, uploaded: daysAgo(120) },
    ])
    const kv = createKvStub({ [INTAKE_PAUSED_KEY]: '1' })
    const env = { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv } as never

    await handleDailyEvictionSweep(env)

    expect(await kv.get(INTAKE_PAUSED_KEY)).toBeNull()
  })

  it('keeps intake paused while the bucket sits between the watermarks', async () => {
    const GB = 1024 ** 3
    // 7 GB: under the 8 GB high watermark (so no eviction) but over the 6 GB low one, which is
    // the level that paused intake in the first place. Reopening here would just refill it.
    const bucket = createR2Stub([
      { key: `${ERROR_REPORT_PREFIX}prod/2026-04-01/a.zip`, size: 7 * GB, uploaded: daysAgo(120) },
    ])
    const kv = createKvStub({ [INTAKE_PAUSED_KEY]: '1' })
    const env = { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv } as never

    await handleDailyEvictionSweep(env)

    expect(await kv.get(INTAKE_PAUSED_KEY)).not.toBeNull()
  })
})

describe('scheduled handler: eviction job isolation', () => {
  it('does not throw when individual jobs fail (each has its own try/catch)', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    // Importing the default export to exercise the dispatch
    const mod = (await import('./index')).default
    const env = {
      // All bindings missing: each handler should bail internally or throw,
      // and the scheduled wrapper's per-job try/catch absorbs each failure.
      TELEMETRY_DB: createMockD1().db,
    } as never
    const event = { scheduledTime: Date.UTC(2026, 3, 23, 0, 0, 0) } as ScheduledEvent
    await expect(mod.scheduled(event, env)).resolves.toBeUndefined()
    // At least one job logged its caught error
    expect(errSpy).toHaveBeenCalled()
    errSpy.mockRestore()
  })
})
