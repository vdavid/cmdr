import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'
import { app } from '../index'

/** Mock the Workers rate-limit binding. Defaults to allowing every request. */
function createMockRateLimiter(success = true): { limiter: RateLimit; limitMock: Mock } {
  const limitMock = vi.fn(() => Promise.resolve({ success }))
  return { limiter: { limit: limitMock }, limitMock }
}

const betaUuid = 'beta-list-uuid'
const newsletterUuid = 'newsletter-list-uuid'
const publicSubscriptionUrl = 'https://mail.getcmdr.com/api/public/subscription'
const discordWebhook = 'https://discord.example/signups'

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    SIGNUP_LIMITER: createMockRateLimiter().limiter,
    LISTMONK_API_URL: 'https://mail.getcmdr.com',
    LISTMONK_BETA_LIST_ID: 7,
    LISTMONK_BETA_LIST_UUID: betaUuid,
    LISTMONK_NEWSLETTER_LIST_ID: 3,
    LISTMONK_NEWSLETTER_LIST_UUID: newsletterUuid,
    ...overrides,
  }
}

/** Both signup routes share every behavior below; only the list and the Discord title differ. */
const routes = [
  { path: '/beta-signup', listUuid: betaUuid, uuidVar: 'LISTMONK_BETA_LIST_UUID', title: 'New beta-tester signup' },
  {
    path: '/newsletter-signup',
    listUuid: newsletterUuid,
    uuidVar: 'LISTMONK_NEWSLETTER_LIST_UUID',
    title: 'New newsletter signup',
  },
] as const

function post(
  path: string,
  body: unknown,
  bindings: Record<string, unknown>,
  extraHeaders: Record<string, string> = {},
) {
  return app.request(
    path,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...extraHeaders },
      body: typeof body === 'string' ? body : JSON.stringify(body),
    },
    bindings,
  )
}

/** Listmonk's public endpoint answers `{data: {has_optin}}`: true when it sent a confirmation mail. */
function listmonkOk(hasOptin: boolean): Response {
  return new Response(JSON.stringify({ data: { has_optin: hasOptin } }), { status: 200 })
}

/** The Listmonk fetch is the one network boundary (plus Discord); stub it per test. */
let fetchMock: Mock

beforeEach(() => {
  fetchMock = vi.fn((url: string) =>
    Promise.resolve(url === publicSubscriptionUrl ? listmonkOk(true) : new Response('ok')),
  )
  vi.stubGlobal('fetch', fetchMock)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

function listmonkCalls(): [string, RequestInit][] {
  return fetchMock.mock.calls.filter(([url]) => url === publicSubscriptionUrl) as [string, RequestInit][]
}

function findDiscordCall(url = discordWebhook): { body: Record<string, unknown> } | undefined {
  const call = fetchMock.mock.calls.find(([u]) => String(u) === url)
  if (!call) return undefined
  return { body: JSON.parse((call[1] as RequestInit).body as string) as Record<string, unknown> }
}

describe.each(routes)('POST $path', ({ path, listUuid, uuidVar, title }) => {
  it('returns an empty 204 for a valid email', async () => {
    const res = await post(path, { email: 'tester@example.com' }, createBindings())
    expect(res.status).toBe(204)
    expect(await res.text()).toBe('')
  })

  it("subscribes through Listmonk's public endpoint with ONLY the email and the list", async () => {
    await post(path, { email: 'tester@example.com', name: 'ignored', analId: 'anal_x' }, createBindings())

    const calls = listmonkCalls()
    expect(calls).toHaveLength(1)
    const [, init] = calls[0]
    expect(init.method).toBe('POST')
    // The public endpoint needs no credentials, so none are sent.
    expect((init.headers as Record<string, string>).Authorization).toBeUndefined()
    // Nothing but the email and the list: no install id, no name, and no `preconfirm` (the public
    // endpoint has no such field, so double opt-in can't be skipped from here).
    expect(JSON.parse(init.body as string)).toEqual({ email: 'tester@example.com', list_uuids: [listUuid] })
  })

  // Regression anchor (2026-10-05): Listmonk's admin `POST /api/subscribers` swallows a failed opt-in
  // send and answers 200, so for two months every signup "succeeded" while no confirmation mail went
  // out. The public endpoint answers 500 instead, and that has to reach the caller as a failure.
  it('returns a soft 502 when Listmonk could not send the confirmation email', async () => {
    fetchMock.mockImplementation((url: string) =>
      Promise.resolve(
        url === publicSubscriptionUrl
          ? new Response(JSON.stringify({ message: 'Error processing request. Please retry.' }), { status: 500 })
          : new Response('ok'),
      ),
    )
    const res = await post(
      path,
      { email: 'tester@example.com' },
      createBindings({ DISCORD_WEBHOOK_URL: discordWebhook }),
    )
    expect(res.status).toBe(502)
    expect(findDiscordCall()).toBeUndefined()
  })

  it('returns a soft 502 when the Listmonk fetch throws', async () => {
    fetchMock.mockRejectedValueOnce(new Error('network unreachable'))
    const res = await post(path, { email: 'tester@example.com' }, createBindings())
    expect(res.status).toBe(502)
  })

  it('returns 400 when Listmonk rejects the address', async () => {
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify({ message: 'Invalid email.' }), { status: 400 }))
    const res = await post(path, { email: 'tester@example.com' }, createBindings())
    expect(res.status).toBe(400)
  })

  it('returns 400 for a malformed or missing email without calling Listmonk', async () => {
    expect((await post(path, { email: 'not-an-email' }, createBindings())).status).toBe(400)
    expect((await post(path, {}, createBindings())).status).toBe(400)
    expect((await post(path, 'not json', createBindings())).status).toBe(400)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('returns 400 for a body over the size cap without calling Listmonk', async () => {
    const res = await post(path, { email: 'tester@example.com', padding: 'x'.repeat(2048) }, createBindings())
    expect(res.status).toBe(400)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('returns 429 over the rate limit, keyed by the caller IP, before calling Listmonk', async () => {
    const { limiter, limitMock } = createMockRateLimiter(false)
    const res = await post(path, { email: 'tester@example.com' }, createBindings({ SIGNUP_LIMITER: limiter }), {
      'cf-connecting-ip': '203.0.113.9',
    })
    expect(res.status).toBe(429)
    expect(limitMock).toHaveBeenCalledWith({ key: '203.0.113.9' })
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('returns 500 when its list is not configured', async () => {
    const res = await post(path, { email: 'tester@example.com' }, createBindings({ [uuidVar]: undefined }))
    expect(res.status).toBe(500)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('pings Discord, with no email in it, when Listmonk sent a confirmation', async () => {
    const res = await post(
      path,
      { email: 'tester@example.com' },
      createBindings({ DISCORD_WEBHOOK_URL: discordWebhook }),
    )
    expect(res.status).toBe(204)

    const discord = findDiscordCall()
    expect(discord).toBeDefined()
    const embed = (discord?.body.embeds as { title: string; description: string }[])[0]
    expect(embed.title).toBe(title)
    expect(embed.description).toContain('Listmonk sent them the confirmation email')
    expect(JSON.stringify(discord?.body)).not.toContain('tester@example.com')
  })

  it('stays quiet, with the same empty 204, when there was nothing to confirm (no enumeration)', async () => {
    fetchMock.mockResolvedValueOnce(listmonkOk(false))
    const res = await post(
      path,
      { email: 'tester@example.com' },
      createBindings({ DISCORD_WEBHOOK_URL: discordWebhook }),
    )
    expect(res.status).toBe(204)
    expect(await res.text()).toBe('')
    expect(findDiscordCall()).toBeUndefined()
  })

  it('prefers the dedicated signups webhook over the shared one', async () => {
    await post(
      path,
      { email: 'tester@example.com' },
      createBindings({
        DISCORD_BETA_SIGNUP_WEBHOOK_URL: discordWebhook,
        DISCORD_WEBHOOK_URL: 'https://discord.example/x',
      }),
    )
    expect(findDiscordCall()).toBeDefined()
    expect(findDiscordCall('https://discord.example/x')).toBeUndefined()
  })

  it('still returns 204 when the Discord ping fails', async () => {
    fetchMock.mockImplementation((url: string) =>
      Promise.resolve(url === publicSubscriptionUrl ? listmonkOk(true) : new Response('discord down', { status: 500 })),
    )
    const res = await post(
      path,
      { email: 'tester@example.com' },
      createBindings({ DISCORD_WEBHOOK_URL: discordWebhook }),
    )
    expect(res.status).toBe(204)
  })
})

describe('POST /beta-signup privacy invariant', () => {
  it('lets no install id reach Listmonk or Discord', async () => {
    await post(
      '/beta-signup',
      { email: 'tester@example.com', analId: 'anal_should-be-ignored', diagId: 'diag_should-be-ignored' },
      createBindings({ DISCORD_WEBHOOK_URL: discordWebhook }),
    )
    const outbound = JSON.stringify(fetchMock.mock.calls.map(([, init]) => (init as RequestInit).body))
    expect(outbound).not.toContain('anal_')
    expect(outbound).not.toContain('diag_')
  })
})

describe('/newsletter-signup CORS', () => {
  it('answers the preflight for getcmdr.com', async () => {
    const res = await app.request(
      '/newsletter-signup',
      { method: 'OPTIONS', headers: { Origin: 'https://getcmdr.com' } },
      createBindings(),
    )
    expect(res.status).toBe(204)
    expect(res.headers.get('Access-Control-Allow-Origin')).toBe('https://getcmdr.com')
    expect(res.headers.get('Access-Control-Allow-Headers')).toContain('Content-Type')
  })

  it('allows getcmdr.com to read the result, even a failure', async () => {
    fetchMock.mockResolvedValueOnce(new Response('{}', { status: 500 }))
    const res = await post('/newsletter-signup', { email: 'tester@example.com' }, createBindings(), {
      Origin: 'https://getcmdr.com',
    })
    expect(res.status).toBe(502)
    expect(res.headers.get('Access-Control-Allow-Origin')).toBe('https://getcmdr.com')
  })

  it('grants no other origin', async () => {
    const res = await post('/newsletter-signup', { email: 'tester@example.com' }, createBindings(), {
      Origin: 'https://evil.example',
    })
    expect(res.headers.get('Access-Control-Allow-Origin')).toBeNull()
  })
})
