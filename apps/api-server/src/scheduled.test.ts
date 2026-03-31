import { describe, expect, it, vi, beforeEach } from 'vitest'
import { handleCrashNotifications, handleDailyAggregation, handleDbSizeCheck } from './index'

// Mock Resend — intercept email sends
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const mockSend = vi.fn<any>(() => Promise.resolve({ id: 'test-email-id' }))
vi.mock('resend', () => ({
  Resend: class {
    emails = { send: mockSend }
  },
}))

function lastEmailCall(): { subject: string; to: string; from: string; html: string } {
  return mockSend.mock.lastCall?.[0] as { subject: string; to: string; from: string; html: string }
}

/** Create a mock D1Database with configurable query responses. */
function createMockD1(responses: Map<string, unknown> = new Map()) {
  const calls: Array<{ sql: string; bindings: unknown[] }> = []

  const db = {
    prepare: vi.fn((sql: string) => ({
      bind: vi.fn((...args: unknown[]) => {
        calls.push({ sql, bindings: args })
        return {
          all: vi.fn(() => {
            for (const [pattern, response] of responses) {
              if (sql.includes(pattern)) return Promise.resolve(response)
            }
            return Promise.resolve({ results: [] })
          }),
          first: vi.fn(() => {
            for (const [pattern, response] of responses) {
              if (sql.includes(pattern)) return Promise.resolve(response)
            }
            return Promise.resolve(null)
          }),
          run: vi.fn(() => Promise.resolve({ success: true })),
        }
      }),
      all: vi.fn(() => {
        calls.push({ sql, bindings: [] })
        for (const [pattern, response] of responses) {
          if (sql.includes(pattern)) return Promise.resolve(response)
        }
        return Promise.resolve({ results: [] })
      }),
      first: vi.fn(() => {
        calls.push({ sql, bindings: [] })
        for (const [pattern, response] of responses) {
          if (sql.includes(pattern)) return Promise.resolve(response)
        }
        return Promise.resolve(null)
      }),
      run: vi.fn(() => {
        calls.push({ sql, bindings: [] })
        return Promise.resolve({ success: true })
      }),
    })),
  } as unknown as D1Database

  return { db, calls }
}

function createBaseEnv(overrides: Record<string, unknown> = {}) {
  return {
    TELEMETRY_DB: createMockD1().db,
    CRASH_NOTIFICATION_EMAIL: 'test@example.com',
    RESEND_API_KEY: 'test-resend-key',
    ...overrides,
  }
}

beforeEach(() => {
  mockSend.mockClear()
})

describe('handleCrashNotifications', () => {
  it('sends email for un-notified crash reports', async () => {
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'SIGSEGV',
              top_function: 'cmdr::sync::run',
              created_at: '2026-03-23T10:00:00Z',
            },
            {
              id: 2,
              app_version: '1.0.1',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'SIGSEGV',
              top_function: 'cmdr::sync::run',
              created_at: '2026-03-23T11:00:00Z',
            },
            {
              id: 3,
              app_version: '1.0.0',
              os_version: '14.5',
              arch: 'x86_64',
              signal: 'SIGABRT',
              top_function: 'cmdr_lib::indexer::build',
              created_at: '2026-03-23T12:00:00Z',
            },
          ],
        },
      ],
    ])
    const { db, calls } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    // Verify email was sent
    expect(mockSend).toHaveBeenCalledOnce()
    const emailCall = lastEmailCall()
    expect(emailCall.subject).toBe('Cmdr: 3 new crash reports')
    expect(emailCall.to).toBe('test@example.com')
    expect(emailCall.from).toBe('Cmdr Crash Alerts <noreply@getcmdr.com>')
    expect(emailCall.html).toContain('cmdr::sync::run')
    expect(emailCall.html).toContain('cmdr_lib::indexer::build')

    // Verify rows were marked as notified (UPDATE query was called)
    const updateCall = calls.find((c) => c.sql.includes('UPDATE crash_reports'))
    expect(updateCall).toBeDefined()
    // Bindings: [now, ...ids]
    const bindings = updateCall?.bindings ?? []
    expect(bindings.length).toBe(4) // now + 3 ids
    expect(bindings[1]).toBe(1)
    expect(bindings[2]).toBe(2)
    expect(bindings[3]).toBe(3)
  })

  it('sends singular subject for one crash report', async () => {
    const responses = new Map<string, unknown>([
      [
        'SELECT id',
        {
          results: [
            {
              id: 1,
              app_version: '1.0.0',
              os_version: '15.3',
              arch: 'arm64',
              signal: 'SIGSEGV',
              top_function: 'cmdr::sync::run',
              created_at: '2026-03-23T10:00:00Z',
            },
          ],
        },
      ],
    ])
    const { db } = createMockD1(responses)
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    const emailCall = lastEmailCall()
    expect(emailCall.subject).toBe('Cmdr: 1 new crash report')
  })

  it('does not send email when there are no un-notified crashes', async () => {
    const { db } = createMockD1()
    const env = createBaseEnv({ TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    expect(mockSend).not.toHaveBeenCalled()
  })

  it('skips when CRASH_NOTIFICATION_EMAIL is not set', async () => {
    const { db, calls } = createMockD1()
    const env = createBaseEnv({ CRASH_NOTIFICATION_EMAIL: undefined, TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    expect(mockSend).not.toHaveBeenCalled()
    // Should not even query D1
    expect(calls).toHaveLength(0)
  })

  it('skips when RESEND_API_KEY is not set', async () => {
    const { db, calls } = createMockD1()
    const env = createBaseEnv({ RESEND_API_KEY: undefined, TELEMETRY_DB: db })

    await handleCrashNotifications(env as never)

    expect(mockSend).not.toHaveBeenCalled()
    expect(calls).toHaveLength(0)
  })
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
