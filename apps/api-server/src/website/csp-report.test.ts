import { afterEach, describe, expect, it, vi } from 'vitest'
import { app } from '../index'
import { parseCspReports, isActionableViolation } from './csp-report'
import { createSqliteD1 } from '../sqlite-d1-test-helper'

function createKv() {
  const store = new Map<string, string>()
  const ttls = new Map<string, number | undefined>()
  const kv = {
    get: (key: string, type?: string) => {
      const value = store.get(key) ?? null
      return Promise.resolve(type === 'json' && value !== null ? (JSON.parse(value) as unknown) : value)
    },
    put: (key: string, value: string, options?: { expirationTtl?: number }) => {
      store.set(key, value)
      ttls.set(key, options?.expirationTtl)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
  return { kv, store, ttls }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  const db = createSqliteD1()
  return {
    CSP_ALERTS: createKv().kv,
    TELEMETRY_DB: db.d1,
    LICENSE_CODES: {} as KVNamespace,
    /** Test-only handle on the engine behind `TELEMETRY_DB`, for reading the ops events back. */
    sqlite: db.sqlite,
    ...overrides,
  }
}

/** The CSP alerts the next digest would carry. */
function cspAlerts(bindings: ReturnType<typeof createBindings>): string[] {
  const rows = bindings.sqlite.prepare(`SELECT message FROM ops_events WHERE kind = 'csp-violation'`).all() as {
    message: string
  }[]
  return rows.map((row) => row.message)
}

/** The legacy `report-uri` shape Firefox and Safari send. */
function legacyReport(fields: Record<string, string>) {
  return JSON.stringify({ 'csp-report': fields })
}

/** The Reporting API shape Chromium sends to a `report-to` endpoint. */
function reportingApiReport(body: Record<string, string>) {
  return JSON.stringify([{ type: 'csp-violation', age: 10, url: body.documentURL, body }])
}

function post(
  body: string,
  bindings: ReturnType<typeof createBindings>,
  contentType = 'application/csp-report',
  ip = '203.0.113.7',
) {
  return app.request(
    '/csp-report',
    { method: 'POST', headers: { 'content-type': contentType, 'cf-connecting-ip': ip }, body },
    bindings,
  )
}

/** A second visitor, so a violation counts as the site's own rather than one browser's. */
const otherVisitor = '198.51.100.23'

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

  it('reads the disposition and script sample in both shapes', () => {
    const [legacy] = parseCspReports({
      'csp-report': {
        'document-uri': 'https://getcmdr.com/',
        'blocked-uri': 'inline',
        'effective-directive': 'script-src-elem',
        disposition: 'report',
        'script-sample': ';window.__cmdrRReady',
      },
    })
    expect(legacy).toMatchObject({ disposition: 'report', sample: ';window.__cmdrRReady' })

    const [reportingApi] = parseCspReports([
      {
        type: 'csp-violation',
        body: {
          documentURL: 'https://getcmdr.com/',
          blockedURL: 'inline',
          effectiveDirective: 'script-src-elem',
          disposition: 'enforce',
          sample: 'console.log(1)',
        },
      },
    ])
    expect(reportingApi).toMatchObject({ disposition: 'enforce', sample: 'console.log(1)' })
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

  it("ignores a same-origin block under a directive our policies give 'self', which only a sandboxed page can raise", () => {
    // Regression anchor: `frame-src` "blocked" the home page's own /demo/ embed, which `'self'` allows.
    const demo = {
      documentUrl: 'https://getcmdr.com/',
      blockedUrl: 'https://getcmdr.com/demo/',
      directive: 'frame-src',
    }
    expect(isActionableViolation(demo)).toBe(false)
    expect(isActionableViolation({ ...demo, directive: 'script-src-elem' })).toBe(false)
    expect(isActionableViolation({ ...demo, directive: 'connect-src' })).toBe(false)
    // `www.` is a different origin, so the block may be real.
    expect(isActionableViolation({ ...demo, documentUrl: 'https://www.getcmdr.com/' })).toBe(true)
    // The demo's `object-src` and `form-action` don't include `'self'`, so a same-origin block there is real.
    expect(isActionableViolation({ ...demo, directive: 'object-src' })).toBe(true)
    expect(isActionableViolation({ ...demo, directive: 'form-action' })).toBe(true)
  })
})

describe('POST /csp-report', () => {
  it('raises one ops alert per directive and blocked origin once two visitors report it, and answers 204', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({ CSP_ALERTS: kv })

    // Two paths on one origin count as one pair; the alert carries the report that tipped it.
    const first = await post(
      legacyReport({ ...likeButtonViolation, 'blocked-uri': 'https://api.getcmdr.com/likes/other-post' }),
      bindings,
    )
    const second = await post(legacyReport(likeButtonViolation), bindings, 'application/csp-report', otherVisitor)
    await post(legacyReport(likeButtonViolation), bindings, 'application/csp-report', '192.0.2.99')

    expect(first.status).toBe(204)
    expect(second.status).toBe(204)
    const alerts = cspAlerts(bindings)
    expect(alerts).toHaveLength(1)
    expect(alerts[0]).toContain('connect-src')
    expect(alerts[0]).toContain('https://api.getcmdr.com/likes/total-commander-for-macos')
    expect(alerts[0]).toContain('https://getcmdr.com/blog/total-commander-for-macos/')
    expect(alerts[0]).toContain('two different visitors')
    // Query strings can carry anything (tracking params, tokens), so they never reach the alert.
    expect(alerts[0]).not.toContain('utm_source')
    expect([...store.keys()]).toEqual(['csp:connect-src:https://api.getcmdr.com'])
  })

  it("never alerts on one visitor's reports, however many: that's their browser, not our site", async () => {
    const { kv, store, ttls } = createKv()
    const bindings = createBindings({ CSP_ALERTS: kv })

    for (let i = 0; i < 5; i++) await post(legacyReport(likeButtonViolation), bindings)

    expect(cspAlerts(bindings)).toHaveLength(0)
    const entry = store.get('csp:connect-src:https://api.getcmdr.com') ?? ''
    // Only a peppered pseudonym is kept, never the address, and only for the day's window.
    expect(entry).not.toContain('203.0.113.7')
    expect((JSON.parse(entry) as { visitors: string[] }).visitors).toHaveLength(1)
    expect(ttls.get('csp:connect-src:https://api.getcmdr.com')).toBeLessThanOrEqual(24 * 60 * 60)
  })

  it('forgets the visitor pseudonyms once the alert fires', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({ CSP_ALERTS: kv })

    await post(legacyReport(likeButtonViolation), bindings)
    await post(legacyReport(likeButtonViolation), bindings, 'application/csp-report', otherVisitor)

    const entry = JSON.parse(store.get('csp:connect-src:https://api.getcmdr.com') ?? '{}') as Record<string, unknown>
    expect(entry.alerted).toBe(true)
    expect(entry).not.toHaveProperty('visitors')
  })

  it('accepts the Reporting API content type', async () => {
    const bindings = createBindings()
    const report = reportingApiReport({
      documentURL: 'https://getcmdr.com/',
      blockedURL: 'https://api.getcmdr.com/r-codes.json',
      effectiveDirective: 'connect-src',
    })

    const res = await post(report, bindings, 'application/reports+json')
    await post(report, bindings, 'application/reports+json', otherVisitor)

    expect(res.status).toBe(204)
    expect(cspAlerts(bindings)).toHaveLength(1)
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

  it('logs a report-only violation on our page with its sample, and never alerts on it', async () => {
    const log = vi.spyOn(console, 'log').mockImplementation(() => undefined)
    const { kv, store } = createKv()
    const bindings = createBindings({ CSP_ALERTS: kv })

    // Would alert if enforced: a real network URL blocked on our own page.
    const network = await post(legacyReport({ ...likeButtonViolation, disposition: 'report' }), bindings)
    // The strict policy's whole point: an inline script the build didn't hash.
    const inline = await post(
      reportingApiReport({
        documentURL: 'https://getcmdr.com/pricing/?utm_source=x',
        blockedURL: 'inline',
        effectiveDirective: 'script-src-elem',
        disposition: 'report',
        sample: ';Paddle.Initialize({ token: "live_x" })',
        sourceFile: 'https://getcmdr.com/pricing/',
      }),
      bindings,
      'application/reports+json',
    )
    // Someone else's page reporting to us is nobody's concern.
    await post(
      legacyReport({ ...likeButtonViolation, 'document-uri': 'https://evil.test/', disposition: 'report' }),
      bindings,
    )

    expect(network.status).toBe(204)
    expect(inline.status).toBe(204)
    expect(cspAlerts(bindings)).toEqual([])
    expect(store.size).toBe(0)
    const logged = log.mock.calls.map((call) => call[1] as Record<string, unknown>)
    expect(logged).toHaveLength(2)
    expect(logged[1]).toEqual({
      directive: 'script-src-elem',
      blockedUrl: 'inline',
      documentUrl: 'https://getcmdr.com/pricing/',
      sourceFile: 'https://getcmdr.com/pricing/',
      sample: ';Paddle.Initialize({ token: "live_x" })',
    })
    log.mockRestore()
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
