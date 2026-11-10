import { Hono, type Context } from 'hono'
import { callerIp, enforceIpRateLimit, hashCallerIp, readCappedBody, scheduleBackground, type Bindings } from '../types'
import { cspViolationMessage, recordOpsEvent } from '../ops-events'

/**
 * `POST /csp-report`: where getcmdr.com's Content-Security-Policy sends its violation reports
 * (`report-uri` and `report-to` in `apps/website/nginx-security-headers.conf`).
 *
 * A blocked request never reaches us and the page's own code only sees a generic network failure,
 * so this is the one place a CSP that's too strict for our own site shows up at all. The blog's
 * like button was blocked that way for six months with no trace anywhere.
 *
 * Browsers send violations from every visitor, including ones caused by extensions, so only
 * violations that look like OUR breakage count, and a (directive, blocked origin) pair alerts once
 * two different visitors have reported it within a day, at most once a day. Our own breakage hits
 * everyone who loads the page; an extension or a sandboxed preview hits one browser. To tell two
 * visitors apart, the pair's KV entry holds a peppered pseudonym per visitor until the alert fires
 * or the day runs out, never the address. URLs lose their query strings before they reach the event.
 */
const cspReport = new Hono<{ Bindings: Bindings }>()

/** A single report is well under 2 KB; Chromium batches a few. */
const maxCspReportBytes = 32 * 1024
/** How long a pair's window runs: two visitors within it alert, and then it stays quiet until it ends. */
const alertWindowSeconds = 24 * 60 * 60
/** Distinct visitors a pair needs within its window before it alerts. */
const visitorsToAlert = 2
/** KV's floor for `expirationTtl`. */
const minKvTtlSeconds = 60

/**
 * The directives under which BOTH of the site's policies allow `'self'` (`nginx-security-headers.conf`
 * and `nginx-demo-security-headers.conf`; `media-src` and the rest fall back to `default-src 'self'`).
 * A same-origin block under one of them can't be the policy talking: the page ran with an opaque
 * origin (a sandboxed preview, a proxy, a translator). Not here: the demo's `object-src blob:` and
 * `form-action 'none'`, where a same-origin block is real. Keep in sync with those two files.
 */
const selfAllowedDirectives = new Set([
  'default-src',
  'script-src',
  'script-src-elem',
  'script-src-attr',
  'style-src',
  'style-src-elem',
  'style-src-attr',
  'img-src',
  'font-src',
  'connect-src',
  'worker-src',
  'frame-src',
  'child-src',
  'media-src',
  'manifest-src',
])

const ourPageHosts = new Set(['getcmdr.com', 'www.getcmdr.com'])
const reportAllowedOrigins = new Set(['https://getcmdr.com', 'https://www.getcmdr.com'])
const paddleRetainScriptUrl = 'https://public.profitwell.com/js/profitwell.js'

export interface CspViolation {
  documentUrl: string
  blockedUrl: string
  directive: string
  sourceFile?: string
  /** `report` when a `Content-Security-Policy-Report-Only` policy raised it: nothing was blocked. */
  disposition?: string
  /** The first 40 characters of a blocked inline script, when the directive has `'report-sample'`. */
  sample?: string
}

function stringField(obj: Record<string, unknown>, key: string): string | undefined {
  const v = obj[key]
  return typeof v === 'string' ? v : undefined
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v)
}

/**
 * Normalize both report formats: the legacy `report-uri` body (`{"csp-report": {...}}`, kebab-case,
 * Firefox and Safari) and the Reporting API batch (`[{type: "csp-violation", body: {...}}]`,
 * camelCase, Chromium). Anything unrecognized is dropped.
 */
export function parseCspReports(payload: unknown): CspViolation[] {
  if (isRecord(payload) && isRecord(payload['csp-report'])) {
    const r = payload['csp-report']
    const documentUrl = stringField(r, 'document-uri')
    const blockedUrl = stringField(r, 'blocked-uri')
    const directive = stringField(r, 'effective-directive') ?? stringField(r, 'violated-directive')
    if (documentUrl === undefined || blockedUrl === undefined || directive === undefined) return []
    return [
      {
        documentUrl,
        blockedUrl,
        directive,
        sourceFile: stringField(r, 'source-file'),
        disposition: stringField(r, 'disposition'),
        sample: stringField(r, 'script-sample'),
      },
    ]
  }
  if (!Array.isArray(payload)) return []
  const violations: CspViolation[] = []
  for (const report of payload) {
    if (!isRecord(report) || report.type !== 'csp-violation' || !isRecord(report.body)) continue
    const b = report.body
    const documentUrl = stringField(b, 'documentURL')
    const blockedUrl = stringField(b, 'blockedURL')
    const directive = stringField(b, 'effectiveDirective')
    if (documentUrl === undefined || blockedUrl === undefined || directive === undefined) continue
    violations.push({
      documentUrl,
      blockedUrl,
      directive,
      sourceFile: stringField(b, 'sourceFile'),
      disposition: stringField(b, 'disposition'),
      sample: stringField(b, 'sample'),
    })
  }
  return violations
}

function parseHttpUrl(value: string | undefined): URL | null {
  if (!value) return null
  try {
    const url = new URL(value)
    return url.protocol === 'https:' || url.protocol === 'http:' ? url : null
  } catch {
    return null
  }
}

/**
 * Whether a violation looks like our own site breaking: raised on one of our pages, for a real
 * network URL (not `inline`, `eval`, `data`, or an extension resource), by page code rather than a
 * browser extension. Extensions are most of the raw report volume, and none of it is actionable.
 *
 * `font-src` never alerts: the site self-hosts every font, so a blocked font is always an
 * extension's or a browser's, and some arrive attributed to our own scripts (PostHog's recorder
 * re-applies injected styles), which the source-file test can't catch.
 *
 * Paddle Retain's script is blocked on purpose: Paddle.js on a live account loads it on every page
 * that initializes checkout, with no setting to stop it, and we pass no `pwCustomer`, so Retain has
 * nothing to do there. Allowing it would add a tracker the privacy policy doesn't cover.
 *
 * A same-origin block under a directive that allows `'self'` never alerts (`selfAllowedDirectives`).
 */
export function isActionableViolation(v: CspViolation): boolean {
  if (v.disposition === 'report') return false
  if (v.directive === 'font-src') return false
  if (v.directive.startsWith('script-src') && withoutQuery(v.blockedUrl) === paddleRetainScriptUrl) return false
  const page = parseHttpUrl(v.documentUrl)
  if (!page || page.protocol !== 'https:' || !ourPageHosts.has(page.hostname)) return false
  const blocked = parseHttpUrl(v.blockedUrl)
  if (!blocked) return false
  if (blocked.origin === page.origin && selfAllowedDirectives.has(v.directive)) return false
  if (v.sourceFile && !parseHttpUrl(v.sourceFile)) return false
  return true
}

/**
 * Whether a report-only violation is worth a log line: raised on our own page and not by an
 * extension. Unlike an alert, `inline` and `eval` count here, because the Report-Only policy exists
 * to show which inline scripts the build's hashes miss before that policy gets enforced. They're
 * logged, never alerted: nothing was blocked, and the log is read on purpose
 * (`apps/api-server/src/website/DETAILS.md` § CSP reports).
 */
export function isReportOnlyFinding(v: CspViolation): boolean {
  if (v.disposition !== 'report') return false
  const page = parseHttpUrl(v.documentUrl)
  if (!page || page.protocol !== 'https:' || !ourPageHosts.has(page.hostname)) return false
  if (v.sourceFile && !parseHttpUrl(v.sourceFile)) return false
  return true
}

/** The violation as it may leave the Worker: no query strings, which can carry anything. */
function cleanViolation(v: CspViolation) {
  return {
    directive: v.directive,
    blockedUrl: withoutQuery(v.blockedUrl),
    documentUrl: withoutQuery(v.documentUrl),
    sourceFile: v.sourceFile ? withoutQuery(v.sourceFile) : undefined,
  }
}

function withoutQuery(value: string): string {
  const url = parseHttpUrl(value)
  return url ? `${url.origin}${url.pathname}` : value
}

function reportCors(c: Context<{ Bindings: Bindings }>) {
  const origin = c.req.header('origin')
  if (origin && reportAllowedOrigins.has(origin)) {
    c.header('Access-Control-Allow-Origin', origin)
    c.header('Access-Control-Allow-Methods', 'POST, OPTIONS')
    c.header('Access-Control-Allow-Headers', 'Content-Type')
    c.header('Vary', 'Origin')
  }
}

/**
 * One (directive, blocked origin) pair's window, under `csp:<directive>:<origin>`. `visitors` holds
 * a pseudonym per visitor who reported it; once the alert fires they're dropped for `alerted`, which
 * keeps the pair quiet until the window ends.
 */
type PairWindow = { firstSeenMs: number; visitors: string[] } | { firstSeenMs: number; alerted: true }

/**
 * Count this visitor toward the pair's window. Returns whether this report tipped it into an alert.
 *
 * The pseudonym goes through the shared `hashCallerIp`, salted with the KV key, so it means nothing
 * outside this one pair (it can't be joined with telemetry's daily hash). A read-then-write KV can
 * race two reports past each other; the digest folds a duplicate alert into one line.
 */
async function claimAlert(env: Bindings, v: CspViolation, ip: string): Promise<boolean> {
  const blockedOrigin = parseHttpUrl(v.blockedUrl)?.origin ?? v.blockedUrl
  const key = `csp:${v.directive}:${blockedOrigin}`
  const now = Date.now()
  const stored = await env.CSP_ALERTS.get<PairWindow>(key, 'json')
  const window = stored && now - stored.firstSeenMs < alertWindowSeconds * 1000 ? stored : null
  if (window && 'alerted' in window) return false

  const firstSeenMs = window?.firstSeenMs ?? now
  // A put resets the TTL, so each write asks only for what's left of the window.
  const expirationTtl = Math.max(minKvTtlSeconds, alertWindowSeconds - Math.floor((now - firstSeenMs) / 1000))
  const visitor = (await hashCallerIp(ip, key, env.IP_HASH_PEPPER)).slice(0, 16)
  const visitors = window?.visitors ?? []
  if (visitors.includes(visitor)) return false
  visitors.push(visitor)

  const alerts = visitors.length >= visitorsToAlert
  const next: PairWindow = alerts ? { firstSeenMs, alerted: true } : { firstSeenMs, visitors }
  await env.CSP_ALERTS.put(key, JSON.stringify(next), { expirationTtl })
  return alerts
}

async function alertActionable(env: Bindings, violations: CspViolation[], ip: string): Promise<void> {
  for (const v of violations) {
    const clean = cleanViolation(v)
    console.warn('CSP violation on getcmdr.com', clean)
    if (!(await claimAlert(env, v, ip))) continue
    await recordOpsEvent(
      env.TELEMETRY_DB,
      'csp-violation',
      cspViolationMessage({ ...clean, visitors: visitorsToAlert }),
    )
  }
}

cspReport.options('/csp-report', (c) => {
  reportCors(c)
  return c.body(null, 204)
})

cspReport.post('/csp-report', async (c) => {
  reportCors(c)
  const limited = await enforceIpRateLimit(c.env.CSP_REPORT_LIMITER, c.req)
  if (limited) return limited

  const body = c.req.raw.body ? await readCappedBody(c.req.raw.body, maxCspReportBytes) : new ArrayBuffer(0)
  if (body === null) return c.body(null, 413)

  let payload: unknown
  try {
    payload = JSON.parse(new TextDecoder().decode(body))
  } catch {
    // A browser never sends this; there's nothing to learn from it and nothing to tell the sender.
    return c.body(null, 204)
  }

  const violations = parseCspReports(payload)
  for (const v of violations.filter(isReportOnlyFinding)) {
    console.log('CSP report-only violation on getcmdr.com', { ...cleanViolation(v), sample: v.sample })
  }
  const actionable = violations.filter(isActionableViolation)
  if (actionable.length > 0) {
    await scheduleBackground(c, alertActionable(c.env, actionable, callerIp(c.req)))
  }
  return c.body(null, 204)
})

export { cspReport }
