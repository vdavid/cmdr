/**
 * D1 and env fakes shared by the cron-job tests (`scheduled.test.ts` and
 * `crash-notification-email.test.ts`). The Resend mock stays per-file: `vi.mock` is hoisted
 * into the file it appears in, so it can't be shared from here.
 */
import { vi } from 'vitest'

/** Create a mock D1Database with configurable query responses. */
export function createMockD1(responses: Map<string, unknown> = new Map()) {
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
          run: vi.fn(() => Promise.resolve({ success: true, meta: { changes: 0 } })),
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
        return Promise.resolve({ success: true, meta: { changes: 0 } })
      }),
    })),
  } as unknown as D1Database

  return { db, calls }
}

export function createBaseEnv(overrides: Record<string, unknown> = {}) {
  return {
    TELEMETRY_DB: createMockD1().db,
    CRASH_NOTIFICATION_EMAIL: 'test@example.com',
    RESEND_API_KEY: 'test-resend-key',
    ...overrides,
  }
}
