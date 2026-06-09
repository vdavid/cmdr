import { describe, expect, it, vi, type Mock } from 'vitest'
import { app } from './index'

function createMockKv(): KVNamespace {
  return {
    get: vi.fn(() => null),
    put: vi.fn(),
  } as unknown as KVNamespace
}

/** Mock D1Database that tracks prepare/bind/run calls. Returns mocks for assertions. */
function createMockD1(runImpl?: () => Promise<unknown>): {
  db: D1Database
  prepareMock: Mock
  bindMock: Mock
} {
  const run = vi.fn(runImpl ?? (() => Promise.resolve({ success: true })))
  const bindMock = vi.fn(() => ({ run }))
  const prepareMock = vi.fn(() => ({ bind: bindMock }))
  return { db: { prepare: prepareMock } as unknown as D1Database, prepareMock, bindMock }
}

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
  return { writeDataPoint: vi.fn() }
}

/** Mock the Workers rate-limit binding. Defaults to allowing every request. */
function createMockRateLimiter(success = true): { limiter: RateLimit; limitMock: Mock } {
  const limitMock = vi.fn(() => Promise.resolve({ success }))
  return { limiter: { limit: limitMock }, limitMock }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    LICENSE_CODES: createMockKv(),
    DEVICE_COUNTS: createMockAnalyticsEngine(),
    TELEMETRY_DB: createMockD1().db,
    HEARTBEAT_LIMITER: createMockRateLimiter().limiter,
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ADMIN_API_TOKEN: 'test-admin-token-secret',
    ...overrides,
  }
}

const validBeat = {
  analId: 'anal_0123456789abcdef0123456789abcdef0123',
  appVersion: '1.2.3',
  osVersion: '15.3.1',
  arch: 'aarch64',
}

function postHeartbeat(body: unknown, bindings: Record<string, unknown>) {
  return app.request(
    '/heartbeat',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    },
    bindings,
  )
}

describe('POST /heartbeat', () => {
  it('returns 204 for a valid beat', async () => {
    const bindings = createBindings()
    const res = await postHeartbeat(validBeat, bindings)
    expect(res.status).toBe(204)
  })

  it('inserts correct data into D1 (no IP column)', async () => {
    const { db, prepareMock, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await postHeartbeat(validBeat, bindings)

    expect(prepareMock).toHaveBeenCalledOnce()
    const sql = prepareMock.mock.calls[0][0] as string
    expect(sql).toContain('INSERT INTO heartbeat')
    expect(sql).not.toContain('hashed_ip')
    expect(sql).not.toContain('ip')

    const bindArgs = bindMock.mock.calls[0]
    // bindArgs: [anal_id, app_version, os_version, arch, build_mode, config_json]
    expect(bindArgs[0]).toBe('anal_0123456789abcdef0123456789abcdef0123')
    expect(bindArgs[1]).toBe('1.2.3')
    expect(bindArgs[2]).toBe('15.3.1')
    expect(bindArgs[3]).toBe('aarch64')
    expect(bindArgs[4]).toBeNull() // buildMode not supplied
    expect(bindArgs[5]).toBeNull() // config not supplied
  })

  it('round-trips the config blob verbatim as config_json', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const config = { theme: 'dark', viewMode: 'full', fdaGranted: true, tabCount: 3 }
    await postHeartbeat({ ...validBeat, buildMode: 'release', config }, bindings)

    const bindArgs = bindMock.mock.calls[0]
    expect(bindArgs[4]).toBe('release')
    expect(JSON.parse(bindArgs[5] as string)).toEqual(config)
  })

  it('returns 204 when optional fields are omitted', async () => {
    const bindings = createBindings()
    const res = await postHeartbeat(validBeat, bindings)
    expect(res.status).toBe(204)
  })

  it('accepts an explicit null buildMode (upgrade-window compat)', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postHeartbeat({ ...validBeat, buildMode: null, config: null }, bindings)
    expect(res.status).toBe(204)

    const bindArgs = bindMock.mock.calls[0]
    expect(bindArgs[4]).toBeNull()
    expect(bindArgs[5]).toBeNull()
  })

  it('returns 400 when analId is missing', async () => {
    const bindings = createBindings()
    const { analId, ...withoutId } = validBeat
    void analId

    const res = await postHeartbeat(withoutId, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Missing required field: analId')
  })

  it('returns 400 for a malformed analId', async () => {
    const bindings = createBindings()
    const res = await postHeartbeat({ ...validBeat, analId: 'anal_too-short' }, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid analId')
  })

  it('returns 400 for an analId without the anal_ prefix', async () => {
    const bindings = createBindings()
    const res = await postHeartbeat({ ...validBeat, analId: 'diag_0123456789abcdef0123456789abcdef0123' }, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid analId')
  })

  it('returns 400 for a malformed appVersion', async () => {
    const bindings = createBindings()
    const res = await postHeartbeat({ ...validBeat, appVersion: 'not-a-version' }, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid appVersion')
  })

  it('returns 400 when osVersion is missing', async () => {
    const bindings = createBindings()
    const { osVersion, ...rest } = validBeat
    void osVersion

    const res = await postHeartbeat(rest, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Missing required field: osVersion')
  })

  it('returns 400 for an invalid buildMode', async () => {
    const bindings = createBindings()
    const res = await postHeartbeat({ ...validBeat, buildMode: 'staging' }, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid buildMode')
  })

  it('returns 400 for an oversized body', async () => {
    const bindings = createBindings()
    const oversized = { ...validBeat, config: { padding: 'x'.repeat(40_000) } }

    const res = await postHeartbeat(oversized, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Heartbeat too large')
  })

  it('returns 400 for an oversized config blob', async () => {
    const bindings = createBindings()
    // Config blob exceeds its own 16 KB cap while the whole body stays under the 32 KB request cap.
    const config = { note: 'x'.repeat(20_000) }
    const res = await postHeartbeat({ ...validBeat, config }, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Config too large')
  })

  it('returns 400 for malformed JSON', async () => {
    const bindings = createBindings()
    const res = await app.request(
      '/heartbeat',
      { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: 'not json {{{' },
      bindings,
    )
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid JSON')
  })

  it('returns 204 even when the D1 write fails', async () => {
    const { db } = createMockD1(() => Promise.reject(new Error('D1 unavailable')))
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postHeartbeat(validBeat, bindings)
    expect(res.status).toBe(204)
  })

  it('returns 429 when over the rate limit', async () => {
    const { limiter } = createMockRateLimiter(false)
    const bindings = createBindings({ HEARTBEAT_LIMITER: limiter })

    const res = await postHeartbeat(validBeat, bindings)
    expect(res.status).toBe(429)
  })

  it('keys the rate limiter by the caller IP', async () => {
    const { limiter, limitMock } = createMockRateLimiter(true)
    const bindings = createBindings({ HEARTBEAT_LIMITER: limiter })

    await app.request(
      '/heartbeat',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'cf-connecting-ip': '203.0.113.7' },
        body: JSON.stringify(validBeat),
      },
      bindings,
    )

    expect(limitMock).toHaveBeenCalledWith({ key: '203.0.113.7' })
  })

  it('does not touch D1 when rate-limited', async () => {
    const { db, prepareMock } = createMockD1()
    const { limiter } = createMockRateLimiter(false)
    const bindings = createBindings({ TELEMETRY_DB: db, HEARTBEAT_LIMITER: limiter })

    await postHeartbeat(validBeat, bindings)
    expect(prepareMock).not.toHaveBeenCalled()
  })
})
