import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'
import { app } from '../index'
import { createSqliteD1 } from '../sqlite-d1-test-helper'

/** Mock the Workers rate-limit binding. Defaults to allowing every request. */
function createMockRateLimiter(success = true): { limiter: RateLimit; limitMock: Mock } {
  const limitMock = vi.fn(() => Promise.resolve({ success }))
  return { limiter: { limit: limitMock }, limitMock }
}

const betaUuid = 'beta-list-uuid'
const newsletterUuid = 'newsletter-list-uuid'
const subprocessorUuid = 'subprocessor-list-uuid'
const publicSubscriptionUrl = 'https://mail.getcmdr.com/api/public/subscription'

function createBindings(overrides: Record<string, unknown> = {}) {
  const db = createSqliteD1()
  return {
    SIGNUP_LIMITER: createMockRateLimiter().limiter,
    LISTMONK_API_URL: 'https://mail.getcmdr.com',
    LISTMONK_BETA_LIST_UUID: betaUuid,
    LISTMONK_NEWSLETTER_LIST_UUID: newsletterUuid,
    LISTMONK_SUBPROCESSOR_LIST_UUID: subprocessorUuid,
    TELEMETRY_DB: db.d1,
    /** Test-only handle on the engine behind `TELEMETRY_DB`, for reading the signup count back. */
    sqlite: db.sqlite,
    ...overrides,
  }
}

/** The signup events the morning brief would count, as written. */
function signupEvents(bindings: ReturnType<typeof createBindings>): { kind: string; message: string }[] {
  return bindings.sqlite.prepare(`SELECT kind, message FROM ops_events`).all() as { kind: string; message: string }[]
}

/** Every signup route shares every behavior below; only the list and the event kind differ. */
const routes = [
  { path: '/beta-signup', listUuid: betaUuid, uuidVar: 'LISTMONK_BETA_LIST_UUID', kind: 'beta-signup' },
  {
    path: '/newsletter-signup',
    listUuid: newsletterUuid,
    uuidVar: 'LISTMONK_NEWSLETTER_LIST_UUID',
    kind: 'newsletter-signup',
  },
  {
    path: '/subprocessor-updates-signup',
    listUuid: subprocessorUuid,
    uuidVar: 'LISTMONK_SUBPROCESSOR_LIST_UUID',
    kind: 'subprocessor-updates-signup',
  },
] as const

/** getcmdr.com's forms post cross-origin; the app's beta signup doesn't. */
const siteRoutes = ['/newsletter-signup', '/subprocessor-updates-signup'] as const

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

/** The Listmonk fetch is the one network boundary; stub it per test. */
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

describe.each(routes)('POST $path', ({ path, listUuid, uuidVar, kind }) => {
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
    const bindings = createBindings()
    const res = await post(path, { email: 'tester@example.com' }, bindings)
    expect(res.status).toBe(502)
    expect(signupEvents(bindings)).toEqual([])
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

  it('counts the signup for the morning brief, with no email in it, when Listmonk sent a confirmation', async () => {
    const bindings = createBindings()
    const res = await post(path, { email: 'tester@example.com' }, bindings)
    expect(res.status).toBe(204)

    const events = signupEvents(bindings)
    expect(events.map((event) => event.kind)).toEqual([kind])
    expect(JSON.stringify(events)).not.toContain('tester@example.com')
  })

  it('stays quiet, with the same empty 204, when there was nothing to confirm (no enumeration)', async () => {
    fetchMock.mockResolvedValueOnce(listmonkOk(false))
    const bindings = createBindings()
    const res = await post(path, { email: 'tester@example.com' }, bindings)
    expect(res.status).toBe(204)
    expect(await res.text()).toBe('')
    expect(signupEvents(bindings)).toEqual([])
  })

  it('still returns 204 when the count cannot be written', async () => {
    const broken = {
      prepare: () => {
        throw new Error('D1 is down')
      },
    }
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const res = await post(path, { email: 'tester@example.com' }, createBindings({ TELEMETRY_DB: broken }))
    expect(res.status).toBe(204)
  })
})

describe('POST /beta-signup privacy invariant', () => {
  it('lets no install id reach Listmonk or the signup count', async () => {
    const bindings = createBindings()
    await post(
      '/beta-signup',
      { email: 'tester@example.com', analId: 'anal_should-be-ignored', diagId: 'diag_should-be-ignored' },
      bindings,
    )
    const outbound = JSON.stringify([
      ...fetchMock.mock.calls.map(([, init]) => (init as RequestInit).body),
      ...signupEvents(bindings),
    ])
    expect(outbound).not.toContain('anal_')
    expect(outbound).not.toContain('diag_')
  })
})

describe.each(siteRoutes)('%s CORS', (path) => {
  it('answers the preflight for getcmdr.com', async () => {
    const res = await app.request(
      path,
      { method: 'OPTIONS', headers: { Origin: 'https://getcmdr.com' } },
      createBindings(),
    )
    expect(res.status).toBe(204)
    expect(res.headers.get('Access-Control-Allow-Origin')).toBe('https://getcmdr.com')
    expect(res.headers.get('Access-Control-Allow-Headers')).toContain('Content-Type')
  })

  it('allows getcmdr.com to read the result, even a failure', async () => {
    fetchMock.mockResolvedValueOnce(new Response('{}', { status: 500 }))
    const res = await post(path, { email: 'tester@example.com' }, createBindings(), {
      Origin: 'https://getcmdr.com',
    })
    expect(res.status).toBe(502)
    expect(res.headers.get('Access-Control-Allow-Origin')).toBe('https://getcmdr.com')
  })

  it('grants no other origin', async () => {
    const res = await post(path, { email: 'tester@example.com' }, createBindings(), {
      Origin: 'https://evil.example',
    })
    expect(res.headers.get('Access-Control-Allow-Origin')).toBeNull()
  })
})
