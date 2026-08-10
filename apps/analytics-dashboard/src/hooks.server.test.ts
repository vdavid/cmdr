import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { RequestEvent } from '@sveltejs/kit'

const verifyAccessJwt = vi.fn()
vi.mock('./lib/server/access-jwt.js', () => ({ verifyAccessJwt }))

const { handle, handleError } = await import('./hooks.server.js')

function eventFor(path = '/api/report'): RequestEvent {
  return {
    request: new Request(`https://cmdr-analytics-dashboard.pages.dev${path}`),
    locals: {},
    url: new URL(`https://cmdr-analytics-dashboard.pages.dev${path}`),
  } as unknown as RequestEvent
}

describe('handle', () => {
  beforeEach(() => {
    // The gate is compiled out under `vite dev`; force the production branch under test.
    vi.stubEnv('DEV', false)
    verifyAccessJwt.mockReset()
  })

  afterEach(() => {
    vi.unstubAllEnvs()
  })

  it('refuses a request with no valid Access JWT and never runs the route', async () => {
    verifyAccessJwt.mockResolvedValue(null)
    const resolve = vi.fn()

    const response = await handle({ event: eventFor(), resolve })

    expect(response.status).toBe(403)
    expect(resolve).not.toHaveBeenCalled()
  })

  it('refuses page routes too, not only the API', async () => {
    verifyAccessJwt.mockResolvedValue(null)
    const resolve = vi.fn()

    for (const path of ['/', '/product', '/links']) {
      const response = await handle({ event: eventFor(path), resolve })
      expect(response.status).toBe(403)
    }
    expect(resolve).not.toHaveBeenCalled()
  })

  it('runs the route and records the identity when the JWT verifies', async () => {
    verifyAccessJwt.mockResolvedValue({ email: 'veszelovszki@gmail.com', sub: 'user-123' })
    const resolve = vi.fn(async () => new Response('ok', { status: 200 }))
    const event = eventFor('/product')

    const response = await handle({ event, resolve })

    expect(response.status).toBe(200)
    expect(resolve).toHaveBeenCalledOnce()
    expect(event.locals.email).toBe('veszelovszki@gmail.com')
  })

  it('refuses rather than failing open when verification throws', async () => {
    verifyAccessJwt.mockRejectedValue(new Error('jwks exploded'))
    const resolve = vi.fn()

    const response = await handle({ event: eventFor(), resolve })

    expect(response.status).toBe(403)
    expect(resolve).not.toHaveBeenCalled()
  })

  it('leaks nothing about why it refused', async () => {
    verifyAccessJwt.mockResolvedValue(null)
    const response = await handle({ event: eventFor(), resolve: vi.fn() })
    const body = await response.text()

    expect(body).not.toMatch(/jwt|token|audience|issuer|kid/i)
  })
})

describe('handleError', () => {
  it('never returns an exception message or stack to the client', async () => {
    const error = new Error('LICENSE_SERVER_ADMIN_TOKEN=super-secret leaked via stack')
    // The spoofable header any client can set. It must not unlock exception details.
    const event = eventFor()
    const spoofed = new Request(event.request, {
      headers: { 'cf-access-authenticated-user-email': 'attacker@evil.com' },
    })
    const result = await handleError({
      error,
      event: { ...event, request: spoofed },
      status: 500,
      message: 'Internal Error',
    })

    expect(result?.message).toBe('Internal Error')
    expect(JSON.stringify(result)).not.toContain('super-secret')
  })
})
