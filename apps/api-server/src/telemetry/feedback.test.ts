import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { app } from '../index'

/** In-memory D1 stub that records every prepare/bind so tests can assert the written row. */
function createRecordingD1(): { db: D1Database; calls: { sql: string; args: unknown[] }[] } {
  const calls: { sql: string; args: unknown[] }[] = []
  const db = {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => {
        calls.push({ sql, args })
        return { run: () => Promise.resolve({ success: true, meta: { last_row_id: 1 } }) }
      },
    }),
  } as unknown as D1Database
  return { db, calls }
}

/** D1 stub whose writes always reject, for the soft-failure path. */
function createFailingD1(): D1Database {
  return {
    prepare: () => ({
      bind: () => ({ run: () => Promise.reject(new Error('D1 down')) }),
    }),
  } as unknown as D1Database
}

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    LICENSE_CODES: {} as KVNamespace,
    DEVICE_COUNTS: { writeDataPoint: vi.fn() },
    TELEMETRY_DB: createRecordingD1().db,
    ERROR_REPORTS_BUCKET: {} as R2Bucket,
    ERROR_REPORT_META: {} as KVNamespace,
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ...overrides,
  }
}

const validBody = {
  feedback: 'The dual-pane copy flow is great, but I wish F6 renamed in place.',
  appVersion: '0.14.0',
  osVersion: 'macOS 26.0',
}

function postFeedback(body: unknown, bindings: ReturnType<typeof createBindings>) {
  return app.request(
    '/feedback',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    },
    bindings,
  )
}

let fetchMock: ReturnType<typeof vi.fn>

beforeEach(() => {
  fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })))
  globalThis.fetch = fetchMock as unknown as typeof fetch
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('POST /feedback', () => {
  it('returns 204 and writes the feedback row to D1', async () => {
    const { db, calls } = createRecordingD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postFeedback(validBody, bindings)

    expect(res.status).toBe(204)
    expect(calls).toHaveLength(1)
    expect(calls[0].sql).toContain('INSERT INTO feedback')
    expect(calls[0].args).toEqual([validBody.feedback, null, validBody.appVersion, validBody.osVersion, null, null])
  })

  it('stores the optional reply-to email and buildMode', async () => {
    const { db, calls } = createRecordingD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postFeedback({ ...validBody, email: 'tester@example.com', buildMode: 'debug' }, bindings)

    expect(res.status).toBe(204)
    expect(calls[0].args).toEqual([
      validBody.feedback,
      'tester@example.com',
      validBody.appVersion,
      validBody.osVersion,
      'debug',
      null,
    ])
  })

  it('stores the error report id of the logs the sender chose to include', async () => {
    const { db, calls } = createRecordingD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postFeedback({ ...validBody, errorReportId: 'ERR-AB23X' }, bindings)

    expect(res.status).toBe(204)
    expect(calls[0].sql).toContain('error_report_id')
    expect(calls[0].args.at(-1)).toBe('ERR-AB23X')
  })

  it('returns 400 for an error report id that is not an ERR-XXXXX', async () => {
    const bindings = createBindings()
    for (const errorReportId of ['ERR-ab23x', 'CRASH-AB23X', 'ERR-AB23X; DROP TABLE feedback', 42]) {
      const res = await postFeedback({ ...validBody, errorReportId }, bindings)
      expect(res.status).toBe(400)
    }
  })

  it('trims surrounding whitespace from the feedback text before storing', async () => {
    const { db, calls } = createRecordingD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postFeedback({ ...validBody, feedback: '  hello there  \n' }, bindings)

    expect(res.status).toBe(204)
    expect(calls[0].args[0]).toBe('hello there')
  })

  it('returns 400 when feedback is missing', async () => {
    const bindings = createBindings()
    const { feedback: _f, ...rest } = validBody

    const res = await postFeedback(rest, bindings)

    expect(res.status).toBe(400)
  })

  it('returns 400 when feedback is empty or whitespace-only', async () => {
    const bindings = createBindings()

    expect((await postFeedback({ ...validBody, feedback: '' }, bindings)).status).toBe(400)
    expect((await postFeedback({ ...validBody, feedback: '   \n\t' }, bindings)).status).toBe(400)
  })

  it('returns 400 when feedback exceeds the 100,000-code-point cap', async () => {
    const bindings = createBindings()
    const over = 'a'.repeat(100_001)

    const res = await postFeedback({ ...validBody, feedback: over }, bindings)

    expect(res.status).toBe(400)
  })

  it('accepts feedback at exactly the 100,000-code-point cap', async () => {
    const { db } = createRecordingD1()
    const bindings = createBindings({ TELEMETRY_DB: db })
    const atCap = 'a'.repeat(100_000)

    const res = await postFeedback({ ...validBody, feedback: atCap }, bindings)

    expect(res.status).toBe(204)
  })

  it('counts code points, not UTF-16 units (emoji do not double-count)', async () => {
    const { db } = createRecordingD1()
    const bindings = createBindings({ TELEMETRY_DB: db })
    // 60,000 emoji = 120,000 UTF-16 units but only 60,000 code points → under the cap.
    const emoji = '🎉'.repeat(60_000)

    const res = await postFeedback({ ...validBody, feedback: emoji }, bindings)

    expect(res.status).toBe(204)
  })

  it('returns 400 for a malformed email', async () => {
    const bindings = createBindings()

    const res = await postFeedback({ ...validBody, email: 'not-an-email' }, bindings)

    expect(res.status).toBe(400)
  })

  it('tolerates email and buildMode being explicit null (Rust Option::None)', async () => {
    const { db } = createRecordingD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postFeedback({ ...validBody, email: null, buildMode: null, errorReportId: null }, bindings)

    expect(res.status).toBe(204)
  })

  it('returns 400 when appVersion or osVersion is missing', async () => {
    const bindings = createBindings()
    const { appVersion: _a, ...noAppVersion } = validBody
    const { osVersion: _o, ...noOsVersion } = validBody

    expect((await postFeedback(noAppVersion, bindings)).status).toBe(400)
    expect((await postFeedback(noOsVersion, bindings)).status).toBe(400)
  })

  it('returns 400 for an invalid buildMode', async () => {
    const bindings = createBindings()

    const res = await postFeedback({ ...validBody, buildMode: 'nightly' }, bindings)

    expect(res.status).toBe(400)
  })

  it('returns 400 for malformed JSON', async () => {
    const bindings = createBindings()

    const res = await app.request(
      '/feedback',
      { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: 'not-json{{' },
      bindings,
    )

    expect(res.status).toBe(400)
  })

  it('returns 413 when the declared content-length exceeds the body cap', async () => {
    const bindings = createBindings()

    const res = await app.request(
      '/feedback',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'content-length': String(1024 * 1024) },
        body: JSON.stringify(validBody),
      },
      bindings,
    )

    expect(res.status).toBe(413)
  })

  it('returns 429 when the rate limiter denies the request', async () => {
    const limiter = { limit: vi.fn(() => Promise.resolve({ success: false })) }
    const bindings = createBindings({ FEEDBACK_LIMITER: limiter })

    const res = await postFeedback(validBody, bindings)

    expect(res.status).toBe(429)
    expect(limiter.limit).toHaveBeenCalledOnce()
  })

  it('returns 502 when the D1 write rejects, so the app can offer a retry', async () => {
    const bindings = createBindings({ TELEMETRY_DB: createFailingD1() })

    const res = await postFeedback(validBody, bindings)

    expect(res.status).toBe(502)
  })

  it("talks to nobody at intake: the card and the email are the cron's job", async () => {
    const { db } = createRecordingD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postFeedback({ ...validBody, email: 'reply-me@example.com' }, bindings)

    expect(res.status).toBe(204)
    expect(fetchMock).not.toHaveBeenCalled()
  })
})
