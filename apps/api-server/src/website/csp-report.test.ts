import { afterEach, describe, expect, it, vi } from 'vitest'
import { app } from '../index'
import { parseCspReports, isActionableViolation } from './csp-report'

function createKv() {
  const store = new Map<string, string>()
  const kv = {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
  return { kv, store }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    CSP_ALERTS: createKv().kv,
    DISCORD_WEBHOOK_URL: 'https://discord.test/webhook',
    LICENSE_CODES: {} as KVNamespace,
    ...overrides,
  }
}

/** The legacy `report-uri` shape Firefox and Safari send. */
function legacyReport(fields: Record<string, string>) {
  return JSON.stringify({ 'csp-report': fields })
}

/** The Reporting API shape Chromium sends to a `report-to` endpoint. */
function reportingApiReport(body: Record<string, string>) {
  return JSON.stringify([{ type: 'csp-violation', age: 10, url: body.documentURL, body }])
}

function post(body: string, bindings: ReturnType<typeof createBindings>, contentType = 'application/csp-report') {
  return app.request(
    '/csp-report',
    { method: 'POST', headers: { 'content-type': contentType, 'cf-connecting-ip': '203.0.113.7' }, body },
    bindings,
  )
}

const likeButtonViolation = {
  'document-uri': 'https://getcmdr.com/blog/total-commander-for-macos/?utm_source=x',
  'blocked-uri': 'https://api.getcmdr.com/likes/total-commander-for-macos',
  'effective-directive': 'connect-src',
  'source-file': 'https://getcmdr.com/blog/total-commander-for-macos/',
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('parseCspReports', () => {
  it('reads the legacy report-uri shape', () => {
    expect(parseCspReports(JSON.parse(legacyReport(likeButtonViolation)))).toEqual([
      {
        documentUrl: likeButtonViolation['document-uri'],
        blockedUrl: likeButtonViolation['blocked-uri'],
        directive: 'connect-src',
        sourceFile: likeButtonViolation['source-file'],
      },
    ])
  })

  it('reads the Reporting API shape and skips other report types', () => {
    const payload = [
      ...(JSON.parse(
        reportingApiReport({
          documentURL: 'https://getcmdr.com/',
          blockedURL: 'https://api.getcmdr.com/r-codes.json',
          effectiveDirective: 'connect-src',
        }),
      ) as unknown[]),
      { type: 'deprecation', body: { id: 'x' } },
    ]
    expect(parseCspReports(payload)).toEqual([
      {
        documentUrl: 'https://getcmdr.com/',
        blockedUrl: 'https://api.getcmdr.com/r-codes.json',
        directive: 'connect-src',
        sourceFile: undefined,
      },
    ])
  })

  it('falls back to violated-directive when effective-directive is missing', () => {
    const [v] = parseCspReports({
      'csp-report': {
        'document-uri': 'https://getcmdr.com/',
        'blocked-uri': 'https://x.test/',
        'violated-directive': 'img-src',
      },
    })
    expect(v.directive).toBe('img-src')
  })

  it('returns nothing for junk', () => {
    expect(parseCspReports(null)).toEqual([])
    expect(parseCspReports('nope')).toEqual([])
    expect(parseCspReports({ other: 1 })).toEqual([])
    expect(parseCspReports([{ type: 'csp-violation' }])).toEqual([])
  })
})

describe('isActionableViolation', () => {
  const base = {
    documentUrl: 'https://getcmdr.com/blog/x/',
    blockedUrl: 'https://api.getcmdr.com/likes/x',
    directive: 'connect-src',
  }

  it('flags a blocked request from our own page', () => {
    expect(isActionableViolation(base)).toBe(true)
    expect(isActionableViolation({ ...base, documentUrl: 'https://www.getcmdr.com/' })).toBe(true)
  })

  it('ignores pages that are not ours', () => {
    expect(isActionableViolation({ ...base, documentUrl: 'https://evil.test/' })).toBe(false)
    expect(isActionableViolation({ ...base, documentUrl: 'http://localhost:4321/' })).toBe(false)
  })

  it.each(['inline', 'eval', 'data', 'blob', 'chrome-extension://abc/x.js', 'moz-extension://abc/x.js', ''])(
    'ignores the non-network blocked value %j',
    (blockedUrl) => {
      expect(isActionableViolation({ ...base, blockedUrl })).toBe(false)
    },
  )

  it('ignores violations raised by browser extension code', () => {
    expect(isActionableViolation({ ...base, sourceFile: 'chrome-extension://abc/content.js' })).toBe(false)
    expect(isActionableViolation({ ...base, sourceFile: 'safari-web-extension://abc/content.js' })).toBe(false)
  })

  it('ignores blocked fonts, which only extensions and browsers request', () => {
    const font = { ...base, directive: 'font-src', blockedUrl: 'https://fonts.gstatic.com/s/opensans/v44/x.woff2' }
    expect(isActionableViolation(font)).toBe(false)
    // Regression anchor: the scite extension's font arrived attributed to our own PostHog recorder script.
    expect(
      isActionableViolation({ ...font, sourceFile: 'https://getcmdr.com/ph/static/1.435.3/posthog-recorder.js' }),
    ).toBe(false)
  })

  it("ignores Paddle.js loading Retain's script, which we block on purpose", () => {
    const retain = {
      ...base,
      directive: 'script-src-elem',
      blockedUrl: 'https://public.profitwell.com/js/profitwell.js',
    }
    expect(isActionableViolation(retain)).toBe(false)
    expect(isActionableViolation({ ...retain, directive: 'script-src' })).toBe(false)
    // Only that script: anything else from the origin still alerts.
    expect(isActionableViolation({ ...retain, directive: 'connect-src' })).toBe(true)
    expect(isActionableViolation({ ...retain, blockedUrl: 'https://public.profitwell.com/js/other.js' })).toBe(true)
  })
})

describe('POST /csp-report', () => {
  it('alerts Discord once per directive and blocked origin, and answers 204', async () => {
    const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })))
    vi.stubGlobal('fetch', fetchMock)
    const { kv, store } = createKv()
    const bindings = createBindings({ CSP_ALERTS: kv })

    const first = await post(legacyReport(likeButtonViolation), bindings)
    const second = await post(
      legacyReport({ ...likeButtonViolation, 'blocked-uri': 'https://api.getcmdr.com/likes/other-post' }),
      bindings,
    )

    expect(first.status).toBe(204)
    expect(second.status).toBe(204)
    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('https://discord.test/webhook')
    const content = (JSON.parse(init.body as string) as { content: string }).content
    expect(content).toContain('connect-src')
    expect(content).toContain('https://api.getcmdr.com/likes/total-commander-for-macos')
    expect(content).toContain('https://getcmdr.com/blog/total-commander-for-macos/')
    // Query strings can carry anything (tracking params, tokens), so they never reach Discord.
    expect(content).not.toContain('utm_source')
    expect([...store.keys()]).toEqual(['csp:connect-src:https://api.getcmdr.com'])
  })

  it('accepts the Reporting API content type', async () => {
    const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })))
    vi.stubGlobal('fetch', fetchMock)

    const res = await post(
      reportingApiReport({
        documentURL: 'https://getcmdr.com/',
        blockedURL: 'https://api.getcmdr.com/r-codes.json',
        effectiveDirective: 'connect-src',
      }),
      createBindings(),
      'application/reports+json',
    )

    expect(res.status).toBe(204)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('stays quiet for noise and still answers 204', async () => {
    const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })))
    vi.stubGlobal('fetch', fetchMock)
    const { kv, store } = createKv()

    const res = await post(
      legacyReport({ ...likeButtonViolation, 'source-file': 'chrome-extension://abc/content.js' }),
      createBindings({ CSP_ALERTS: kv }),
    )

    expect(res.status).toBe(204)
    expect(fetchMock).not.toHaveBeenCalled()
    expect(store.size).toBe(0)
  })

  it('answers 204 to a malformed body without alerting', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    const res = await post('{not json', createBindings())

    expect(res.status).toBe(204)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('refuses an oversized body', async () => {
    const res = await post('x'.repeat(64 * 1024), createBindings())
    expect(res.status).toBe(413)
  })

  it('is rate limited per IP', async () => {
    const bindings = createBindings({ CSP_REPORT_LIMITER: { limit: () => Promise.resolve({ success: false }) } })
    const res = await post(legacyReport(likeButtonViolation), bindings)
    expect(res.status).toBe(429)
  })
})

describe('OPTIONS /csp-report', () => {
  it('lets getcmdr.com send Reporting API reports cross-origin', async () => {
    const res = await app.request(
      '/csp-report',
      { method: 'OPTIONS', headers: { origin: 'https://getcmdr.com', 'access-control-request-method': 'POST' } },
      createBindings(),
    )
    expect(res.status).toBe(204)
    expect(res.headers.get('access-control-allow-origin')).toBe('https://getcmdr.com')
    expect(res.headers.get('access-control-allow-headers')).toBe('Content-Type')
  })

  it('grants nothing to other origins', async () => {
    const res = await app.request(
      '/csp-report',
      { method: 'OPTIONS', headers: { origin: 'https://evil.test' } },
      createBindings(),
    )
    expect(res.headers.get('access-control-allow-origin')).toBeNull()
  })
})
