import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'
import { app } from './index'

function createMockKv(): KVNamespace {
  return {
    get: vi.fn(() => null),
    put: vi.fn(),
  } as unknown as KVNamespace
}

function createMockD1(): D1Database {
  const run = vi.fn(() => Promise.resolve({ success: true }))
  const bindMock = vi.fn(() => ({ run }))
  const prepareMock = vi.fn(() => ({ bind: bindMock }))
  return { prepare: prepareMock } as unknown as D1Database
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
    TELEMETRY_DB: createMockD1(),
    BETA_SIGNUP_LIMITER: createMockRateLimiter().limiter,
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ADMIN_API_TOKEN: 'test-admin-token-secret',
    LISTMONK_API_URL: 'https://mail.getcmdr.com',
    LISTMONK_API_USER: 'agent',
    LISTMONK_API_TOKEN: 'listmonk-test-token',
    LISTMONK_BETA_LIST_ID: 7,
    ...overrides,
  }
}

function postBetaSignup(body: unknown, bindings: Record<string, unknown>, extraHeaders: Record<string, string> = {}) {
  return app.request(
    '/beta-signup',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...extraHeaders },
      body: JSON.stringify(body),
    },
    bindings,
  )
}

/** The Listmonk fetch is the one network boundary; stub it per test. */
let fetchMock: Mock

beforeEach(() => {
  fetchMock = vi.fn(() => Promise.resolve(new Response(JSON.stringify({ data: { id: 42 } }), { status: 200 })))
  vi.stubGlobal('fetch', fetchMock)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('POST /beta-signup', () => {
  it('returns 204 for a valid email', async () => {
    const res = await postBetaSignup({ email: 'tester@example.com' }, createBindings())
    expect(res.status).toBe(204)
  })

  it('subscribes to the configured list as unconfirmed with NO preconfirm (double opt-in)', async () => {
    await postBetaSignup({ email: 'tester@example.com' }, createBindings())

    expect(fetchMock).toHaveBeenCalledOnce()
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('https://mail.getcmdr.com/api/subscribers')
    expect(init.method).toBe('POST')

    const headers = init.headers as Record<string, string>
    expect(headers.Authorization).toBe('token agent:listmonk-test-token')

    const sentBody = JSON.parse(init.body as string) as Record<string, unknown>
    expect(sentBody.email).toBe('tester@example.com')
    expect(sentBody.lists).toEqual([7])
    expect(sentBody.status).toBe('unconfirmed')
    // Double opt-in: Listmonk sends its own confirmation. We must never preconfirm.
    expect(sentBody.preconfirm_subscriptions).toBeUndefined()
  })

  it('carries the email and NO install id of any kind (the privacy invariant)', async () => {
    // The whole point: the /beta-signup body and the outbound Listmonk body must never carry an
    // analytics or diagnostics install id. Email and analytics ids must never co-occur on our servers.
    const inboundBody = { email: 'tester@example.com' }
    await postBetaSignup(inboundBody, createBindings())

    const inboundSerialized = JSON.stringify(inboundBody)
    expect(inboundSerialized).not.toContain('anal_')
    expect(inboundSerialized).not.toContain('diag_')

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    const outboundSerialized = init.body as string
    expect(outboundSerialized).not.toContain('anal_')
    expect(outboundSerialized).not.toContain('diag_')
    // Defense in depth: no field named like an install id leaked into the outbound payload.
    const outbound = JSON.parse(outboundSerialized) as Record<string, unknown>
    expect(outbound.analId).toBeUndefined()
    expect(outbound.diagId).toBeUndefined()
    expect(outbound.installId).toBeUndefined()
  })

  it('returns 400 for a malformed email', async () => {
    const res = await postBetaSignup({ email: 'not-an-email' }, createBindings())
    expect(res.status).toBe(400)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('returns 400 for a missing email', async () => {
    const res = await postBetaSignup({}, createBindings())
    expect(res.status).toBe(400)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('returns a soft 502 when Listmonk errors', async () => {
    fetchMock.mockResolvedValueOnce(new Response('upstream down', { status: 500 }))
    const res = await postBetaSignup({ email: 'tester@example.com' }, createBindings())
    expect(res.status).toBe(502)
  })

  it('returns a soft 502 when the Listmonk fetch throws', async () => {
    fetchMock.mockRejectedValueOnce(new Error('network unreachable'))
    const res = await postBetaSignup({ email: 'tester@example.com' }, createBindings())
    expect(res.status).toBe(502)
  })

  it('does not reveal whether the address already existed (no enumeration)', async () => {
    // Listmonk returns 409 when the subscriber already exists. We treat that as success (204)
    // so the response is identical for new and existing addresses.
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ message: 'subscriber already exists' }), { status: 409 }),
    )
    const res = await postBetaSignup({ email: 'tester@example.com' }, createBindings())
    expect(res.status).toBe(204)

    // The body must be empty, identical to the brand-new-subscriber case (no oracle).
    const text = await res.text()
    expect(text).toBe('')
  })

  it('returns 429 when over the rate limit', async () => {
    const { limiter } = createMockRateLimiter(false)
    const res = await postBetaSignup({ email: 'tester@example.com' }, createBindings({ BETA_SIGNUP_LIMITER: limiter }))
    expect(res.status).toBe(429)
    // Rate-limited requests never reach Listmonk.
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('keys the rate limiter by the caller IP (the IP is used only for the limiter)', async () => {
    const { limiter, limitMock } = createMockRateLimiter(true)
    await postBetaSignup({ email: 'tester@example.com' }, createBindings({ BETA_SIGNUP_LIMITER: limiter }), {
      'cf-connecting-ip': '203.0.113.9',
    })
    expect(limitMock).toHaveBeenCalledWith({ key: '203.0.113.9' })
  })

  it('returns 500 when Listmonk is not configured (so the app surfaces a soft failure)', async () => {
    const res = await postBetaSignup(
      { email: 'tester@example.com' },
      createBindings({ LISTMONK_API_TOKEN: undefined, LISTMONK_BETA_LIST_ID: undefined }),
    )
    expect(res.status).toBe(500)
    expect(fetchMock).not.toHaveBeenCalled()
  })
})
