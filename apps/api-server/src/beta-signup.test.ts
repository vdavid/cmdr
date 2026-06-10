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

  it('subscribes to the configured list with status enabled and NO preconfirm (double opt-in)', async () => {
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
    // Subscriber status must be a valid Listmonk enum (enabled/disabled/blocklisted); "unconfirmed"
    // is the per-list subscription status and Postgres rejects it as a subscriber status.
    expect(sentBody.status).toBe('enabled')
    // Double opt-in: Listmonk sends its own confirmation, leaving the per-list subscription
    // unconfirmed. We must never preconfirm.
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

const discordWebhook = 'https://discord.example/beta-signups'

/** Find the Discord webhook POST among the recorded fetch calls, or undefined if none was made. */
function findDiscordCall(): { body: Record<string, unknown> } | undefined {
  const call = fetchMock.mock.calls.find(([url]) => String(url) === discordWebhook)
  if (!call) return undefined
  const init = call[1] as RequestInit
  return { body: JSON.parse(init.body as string) as Record<string, unknown> }
}

/**
 * Route fetch by URL + method, so a test can model the multi-call 409-recovery flow:
 * Listmonk subscribe, lookup, list-add, optin, then the Discord webhook.
 */
function routeFetch(handlers: { match: (url: string, init?: RequestInit) => boolean; response: () => Response }[]) {
  fetchMock.mockImplementation((url: string, init?: RequestInit) => {
    const handler = handlers.find((h) => h.match(url, init))
    if (!handler) throw new Error(`Unexpected fetch: ${init?.method ?? 'GET'} ${url}`)
    return Promise.resolve(handler.response())
  })
}

const subscribersUrl = 'https://mail.getcmdr.com/api/subscribers'

describe('POST /beta-signup Discord notification', () => {
  it('pings Discord on a fresh subscription (Listmonk 2xx)', async () => {
    const res = await postBetaSignup(
      { email: 'tester@example.com' },
      createBindings({ DISCORD_BETA_SIGNUP_WEBHOOK_URL: discordWebhook }),
    )
    expect(res.status).toBe(204)

    const discord = findDiscordCall()
    expect(discord).toBeDefined()
    const embed = (discord?.body.embeds as { title: string; description: string; fields: unknown[] }[])[0]
    expect(embed.title).toBe('New beta-tester signup')
    expect(embed.description).toContain('Listmonk sent them the confirmation email')
  })

  it('does NOT ping Discord on a Listmonk 5xx failure', async () => {
    fetchMock.mockResolvedValueOnce(new Response('upstream down', { status: 500 }))
    const res = await postBetaSignup(
      { email: 'tester@example.com' },
      createBindings({ DISCORD_BETA_SIGNUP_WEBHOOK_URL: discordWebhook }),
    )
    expect(res.status).toBe(502)
    expect(findDiscordCall()).toBeUndefined()
  })

  it('does NOT ping Discord when the subscriber is already on the beta list (409, quiet re-signup)', async () => {
    routeFetch([
      {
        match: (url, init) => url === subscribersUrl && init?.method === 'POST',
        response: () => new Response(JSON.stringify({ message: 'already exists' }), { status: 409 }),
      },
      {
        // Lookup: subscriber is already subscribed to the beta list (id 7).
        match: (url, init) => url.startsWith(subscribersUrl) && (init?.method ?? 'GET') === 'GET',
        response: () =>
          new Response(
            JSON.stringify({
              data: { results: [{ id: 42, lists: [{ id: 7, subscription_status: 'confirmed' }] }] },
            }),
            { status: 200 },
          ),
      },
    ])

    const res = await postBetaSignup(
      { email: 'tester@example.com' },
      createBindings({ DISCORD_BETA_SIGNUP_WEBHOOK_URL: discordWebhook }),
    )
    expect(res.status).toBe(204)
    expect(findDiscordCall()).toBeUndefined()
  })

  it('adds an existing subscriber to the beta list on 409 and pings Discord', async () => {
    const seen: { url: string; method: string; body?: string }[] = []
    fetchMock.mockImplementation((url: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      seen.push({ url, method, body: init?.body as string | undefined })
      if (url === subscribersUrl && method === 'POST') {
        return Promise.resolve(new Response(JSON.stringify({ message: 'already exists' }), { status: 409 }))
      }
      if (url.startsWith(subscribersUrl) && method === 'GET') {
        // Existing subscriber on the newsletter (list 3) but NOT the beta list (7).
        return Promise.resolve(
          new Response(
            JSON.stringify({
              data: { results: [{ id: 42, lists: [{ id: 3, subscription_status: 'confirmed' }] }] },
            }),
            { status: 200 },
          ),
        )
      }
      if (url === `${subscribersUrl}/lists` && method === 'PUT') {
        return Promise.resolve(new Response(JSON.stringify({ data: true }), { status: 200 }))
      }
      if (url === `${subscribersUrl}/42/optin` && method === 'POST') {
        return Promise.resolve(new Response(JSON.stringify({ data: true }), { status: 200 }))
      }
      return Promise.resolve(new Response(url)) // Discord webhook + any fallthrough
    })

    const res = await postBetaSignup(
      { email: 'tester@example.com' },
      createBindings({ DISCORD_BETA_SIGNUP_WEBHOOK_URL: discordWebhook }),
    )
    expect(res.status).toBe(204)

    // The list-add targets the beta list with action=add and status=unconfirmed.
    const listAdd = seen.find((s) => s.url === `${subscribersUrl}/lists` && s.method === 'PUT')
    expect(listAdd).toBeDefined()
    const addBody = JSON.parse(listAdd?.body ?? '{}') as Record<string, unknown>
    expect(addBody.ids).toEqual([42])
    expect(addBody.action).toBe('add')
    expect(addBody.target_list_ids).toEqual([7])
    expect(addBody.status).toBe('unconfirmed')

    // The opt-in confirmation email is explicitly triggered (the list-add endpoint doesn't send it).
    expect(seen.some((s) => s.url === `${subscribersUrl}/42/optin` && s.method === 'POST')).toBe(true)

    const discord = findDiscordCall()
    expect(discord).toBeDefined()
    const embed = (discord?.body.embeds as { description: string }[])[0]
    expect(embed.description).toContain('Existing subscriber, added to the beta list')
  })

  it('still returns 204 when the Discord ping fails', async () => {
    fetchMock.mockImplementation((url: string, init?: RequestInit) => {
      if (url === subscribersUrl && (init?.method ?? 'GET') === 'POST') {
        return Promise.resolve(new Response(JSON.stringify({ data: { id: 1 } }), { status: 200 }))
      }
      // Discord webhook fails.
      return Promise.resolve(new Response('discord down', { status: 500 }))
    })

    const res = await postBetaSignup(
      { email: 'tester@example.com' },
      createBindings({ DISCORD_BETA_SIGNUP_WEBHOOK_URL: discordWebhook }),
    )
    expect(res.status).toBe(204)
  })

  it('the outbound Discord payload carries no install id (privacy invariant)', async () => {
    await postBetaSignup(
      { email: 'tester@example.com', analId: 'anal_should-be-ignored', diagId: 'diag_should-be-ignored' },
      createBindings({ DISCORD_BETA_SIGNUP_WEBHOOK_URL: discordWebhook }),
    )

    const discord = findDiscordCall()
    expect(discord).toBeDefined()
    const serialized = JSON.stringify(discord?.body)
    expect(serialized).not.toContain('anal_')
    expect(serialized).not.toContain('diag_')
  })

  it('falls back to DISCORD_WEBHOOK_URL when no dedicated beta-signup webhook is set', async () => {
    await postBetaSignup(
      { email: 'tester@example.com' },
      createBindings({ DISCORD_WEBHOOK_URL: 'https://discord.example/error-reports' }),
    )

    const fallbackCall = fetchMock.mock.calls.find(([url]) => String(url) === 'https://discord.example/error-reports')
    expect(fallbackCall).toBeDefined()
  })

  it('does not ping when no Discord webhook is configured', async () => {
    const res = await postBetaSignup({ email: 'tester@example.com' }, createBindings())
    expect(res.status).toBe(204)
    // Only the Listmonk subscribe call happened, nothing Discord-bound.
    expect(fetchMock).toHaveBeenCalledOnce()
  })
})
