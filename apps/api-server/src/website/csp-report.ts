import { Hono, type Context } from 'hono'
import { enforceIpRateLimit, readCappedBody, scheduleBackground, type Bindings } from '../types'
import { postCspViolationNotification } from '../discord'

/**
 * `POST /csp-report`: where getcmdr.com's Content-Security-Policy sends its violation reports
 * (`report-uri` and `report-to` in `apps/website/nginx-security-headers.conf`).
 *
 * A blocked request never reaches us and the page's own code only sees a generic network failure,
 * so this is the one place a CSP that's too strict for our own site shows up at all. The blog's
 * like button was blocked that way for six months with no trace anywhere.
 *
 * Browsers send violations from every visitor, including ones caused by extensions, so only
 * violations that look like OUR breakage alert, and each (directive, blocked origin) pair alerts at
 * most once a day. Nothing about the visitor is stored: the dedupe key holds only the directive and
 * an origin, and URLs lose their query strings before they reach Discord.
 */
const cspReport = new Hono<{ Bindings: Bindings }>()

/** A single report is well under 2 KB; Chromium batches a few. */
const maxCspReportBytes = 32 * 1024
const alertDedupeSeconds = 24 * 60 * 60

const ourPageHosts = new Set(['getcmdr.com', 'www.getcmdr.com'])
const reportAllowedOrigins = new Set(['https://getcmdr.com', 'https://www.getcmdr.com'])
const paddleRetainScriptUrl = 'https://public.profitwell.com/js/profitwell.js'

export interface CspViolation {
  documentUrl: string
  blockedUrl: string
  directive: string
  sourceFile?: string
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
    return [{ documentUrl, blockedUrl, directive, sourceFile: stringField(r, 'source-file') }]
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
    violations.push({ documentUrl, blockedUrl, directive, sourceFile: stringField(b, 'sourceFile') })
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
 */
export function isActionableViolation(v: CspViolation): boolean {
  if (v.directive === 'font-src') return false
  if (v.directive.startsWith('script-src') && withoutQuery(v.blockedUrl) === paddleRetainScriptUrl) return false
  const page = parseHttpUrl(v.documentUrl)
  if (!page || page.protocol !== 'https:' || !ourPageHosts.has(page.hostname)) return false
  if (!parseHttpUrl(v.blockedUrl)) return false
  if (v.sourceFile && !parseHttpUrl(v.sourceFile)) return false
  return true
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

/** Alert once per (directive, blocked origin) per day. Returns whether this call claimed the alert. */
async function claimAlert(kv: KVNamespace, v: CspViolation): Promise<boolean> {
  const blockedOrigin = parseHttpUrl(v.blockedUrl)?.origin ?? v.blockedUrl
  const key = `csp:${v.directive}:${blockedOrigin}`
  if ((await kv.get(key)) !== null) return false
  await kv.put(key, '1', { expirationTtl: alertDedupeSeconds })
  return true
}

async function alertActionable(env: Bindings, violations: CspViolation[]): Promise<void> {
  for (const v of violations) {
    const clean = {
      directive: v.directive,
      blockedUrl: withoutQuery(v.blockedUrl),
      documentUrl: withoutQuery(v.documentUrl),
      sourceFile: v.sourceFile ? withoutQuery(v.sourceFile) : undefined,
    }
    console.warn('CSP violation on getcmdr.com', clean)
    if (!env.DISCORD_WEBHOOK_URL) continue
    if (!(await claimAlert(env.CSP_ALERTS, v))) continue
    await postCspViolationNotification(env.DISCORD_WEBHOOK_URL, clean)
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

  const actionable = parseCspReports(payload).filter(isActionableViolation)
  if (actionable.length > 0) {
    await scheduleBackground(c, alertActionable(c.env, actionable))
  }
  return c.body(null, 204)
})

export { cspReport }
