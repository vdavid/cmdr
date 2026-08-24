import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import {
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  handleSyntheticHeartbeatSweep,
} from './index'
import { syntheticHeartbeatGraceDays } from './scheduled'
import { ERROR_REPORT_PREFIX, EVICTION_MIN_AGE_DAYS, TOTAL_BYTES_KEY } from './telemetry/error-report-eviction'
import { INTAKE_PAUSED_KEY } from './telemetry/error-report-intake'
import { createBaseEnv, createMockD1 } from './cron-test-helpers'

/** Fixtures age relative to now, so eviction eligibility never depends on the calendar. */
function daysAgo(days: number): Date {
  return new Date(Date.now() - days * 24 * 60 * 60 * 1000)
}

// Mock Resend: intercept email sends
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- mock stands in for Resend's send; a precise signature adds no test value
const mockSend = vi.fn<any>(() => Promise.resolve({ id: 'test-email-id' }))
vi.mock('resend', () => ({
  Resend: class {
    emails = { send: mockSend }
  },
}))

function lastEmailCall(): { subject: string; to: string; from: string; html: string } {
  return mockSend.mock.lastCall?.[0] as { subject: string; to: string; from: string; html: string }
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
  it('deletes only heartbeats, bounded by the grace-period cutoff', async () => {
    const { db, calls } = createMockD1()
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleSyntheticHeartbeatSweep(env as never)

    expect(calls).toHaveLength(1)
    expect(calls[0].sql).toContain('DELETE FROM heartbeat')
    // The cutoff has to be BOUND, never inlined: an unbounded delete would take the brand-new
    // installs that have not saved a setting yet. What the predicate does with it is proved
    // against a real SQLite in `synthetic-heartbeats.test.ts`.
    expect(calls[0].bindings).toHaveLength(1)
    const cutoffMs = Date.parse(`${String(calls[0].bindings[0])}Z`)
    const expectedMs = Date.now() - syntheticHeartbeatGraceDays * 86_400_000
    expect(Math.abs(cutoffMs - expectedMs)).toBeLessThan(86_400_000)
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

describe('handleDbSizeCheck', () => {
  it('sends alert when DB size exceeds threshold', async () => {
    const sizeBytes = 150 * 1024 * 1024 // 150 MB
    const responses = new Map<string, unknown>([
      ['pragma_page_count', { total_size: sizeBytes }],
      ['COUNT(*)', { cnt: 42 }],
    ])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleDbSizeCheck(env as never)

    expect(mockSend).toHaveBeenCalledOnce()
    const emailCall = lastEmailCall()
    expect(emailCall.subject).toBe('Cmdr: telemetry DB is 150 MB')
  })

  it('does not send alert when DB size is under threshold', async () => {
    const sizeBytes = 50 * 1024 * 1024 // 50 MB
    const responses = new Map<string, unknown>([['pragma_page_count', { total_size: sizeBytes }]])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleDbSizeCheck(env as never)

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('does not send alert when pragma query returns null', async () => {
    const { db } = createMockD1()
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleDbSizeCheck(env as never)

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('skips when CRASH_NOTIFICATION_EMAIL is not set', async () => {
    const { db } = createMockD1()
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
