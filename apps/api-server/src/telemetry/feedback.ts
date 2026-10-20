import { Hono, type Context } from 'hono'
import { enforceIpRateLimit, hasEmailShape, isAbsent, type Bindings } from '../types'
import { SHORT_ID_PATTERN } from './error-report'

const feedback = new Hono<{ Bindings: Bindings }>()

/**
 * In-app "Send feedback" ingestion. JSON body (no log bundle, unlike `/error-report`):
 * required `feedback` text plus `appVersion` / `osVersion`, optional reply-to `email`,
 * `buildMode`, and `errorReportId`. The last is the `ERR-XXXXX` of the logs the sender chose to
 * include: the app sends them right after this request, as an ordinary `/error-report` upload
 * under that id, and the cron puts the two on one card (`report-outbox.ts`). The row is written to the D1 `feedback` table (the durable sink; the
 * write is awaited so a failure surfaces as a retryable soft 502, not a silent drop).
 * The cron files its card and mails it in the next digest.
 *
 * Abuse guards mirror the other public ingestion routes: an IP-keyed rate limiter
 * (the IP is never stored), a body byte cap, and strict shape validation.
 */

/**
 * Hard cap on the feedback text, counted in Unicode code points so it matches both the
 * dialog's counter (`Array.from(text).length`) and the Rust validator (`.chars().count()`).
 * Same number as the error reporter's user-note cap.
 */
const maxFeedbackChars = 100_000

/**
 * Byte cap on the whole request body. 100k code points of 4-byte code points is ~400 KB,
 * so 512 KB leaves headroom for the JSON envelope without letting anyone POST megabytes.
 */
const maxFeedbackBytes = 512 * 1024

interface FeedbackBody {
  feedback: string
  appVersion: string
  osVersion: string
  email?: string | null
  buildMode?: 'release' | 'debug' | null
  errorReportId?: string | null
}

/**
 * Validate the runtime shape of a `POST /feedback` body. Returns the error message to
 * surface as a 400, or `null` when the body is well-formed. Optional fields tolerate
 * both `undefined` and `null` (Rust serializes `Option::None` as `null`).
 */
function validateFeedbackShape(body: Record<string, unknown>): string | null {
  const text = body.feedback
  if (typeof text !== 'string' || text.trim().length === 0) {
    return 'Missing feedback text'
  }
  // Count code points (not UTF-16 units) so the cap matches the desktop validators.
  if (Array.from(text).length > maxFeedbackChars) {
    return `Feedback is too long (max ${String(maxFeedbackChars)} characters)`
  }
  for (const field of ['appVersion', 'osVersion'] as const) {
    const value = body[field]
    if (typeof value !== 'string' || value.length === 0) {
      return `Missing required field: ${field}`
    }
  }
  return validateOptionalFields(body)
}

/**
 * The fields a client may omit. Each tolerates `null` as well as `undefined` (see {@link isAbsent});
 * only a present-but-wrong value fails.
 */
function validateOptionalFields(body: Record<string, unknown>): string | null {
  const { email, buildMode, errorReportId } = body
  if (!isAbsent(email) && (typeof email !== 'string' || !hasEmailShape(email))) {
    return 'Invalid email'
  }
  if (!isAbsent(buildMode) && buildMode !== 'release' && buildMode !== 'debug') {
    return 'Invalid buildMode'
  }
  if (!isAbsent(errorReportId) && (typeof errorReportId !== 'string' || !SHORT_ID_PATTERN.test(errorReportId))) {
    return 'Invalid errorReportId'
  }
  return null
}

/** Read and parse the request body, enforcing the size cap. Returns the parsed object or an error. */
async function readFeedbackBody(c: Context<{ Bindings: Bindings }>): Promise<Record<string, unknown> | Response> {
  const contentLength = c.req.header('content-length')
  if (contentLength && parseInt(contentLength, 10) > maxFeedbackBytes) {
    return c.json({ error: 'Feedback too large' }, 413)
  }

  let rawBody: string
  try {
    rawBody = await c.req.text()
  } catch {
    return c.json({ error: 'Could not read request body' }, 400)
  }
  if (rawBody.length > maxFeedbackBytes) {
    return c.json({ error: 'Feedback too large' }, 413)
  }

  let parsed: unknown
  try {
    parsed = JSON.parse(rawBody)
  } catch {
    return c.json({ error: 'Invalid JSON' }, 400)
  }
  if (!parsed || typeof parsed !== 'object') {
    return c.json({ error: 'Invalid JSON' }, 400)
  }
  return parsed as Record<string, unknown>
}

feedback.post('/feedback', async (c) => {
  // Rate-limit by the caller IP before any parsing. The IP keys the limiter's sliding
  // window only and is never stored.
  const limited = await enforceIpRateLimit(c.env.FEEDBACK_LIMITER, c.req)
  if (limited) return limited

  const parsed = await readFeedbackBody(c)
  if (parsed instanceof Response) return parsed

  const validationError = validateFeedbackShape(parsed)
  if (validationError) {
    return c.json({ error: validationError }, 400)
  }
  const body = parsed as unknown as FeedbackBody
  const text = body.feedback.trim()

  // D1 is the durable sink, so this write is awaited: the desktop app should surface a retry on
  // failure rather than pretend it landed. The row is also the outbox entry: `issue_number` starts
  // NULL and the cron files its card (`report-outbox.ts`).
  try {
    await c.env.TELEMETRY_DB.prepare(
      `INSERT INTO feedback (feedback, email, app_version, os_version, build_mode, error_report_id)
         VALUES (?, ?, ?, ?, ?, ?)`,
    )
      .bind(
        text,
        body.email ?? null,
        body.appVersion,
        body.osVersion,
        body.buildMode ?? null,
        body.errorReportId ?? null,
      )
      .run()
  } catch (e) {
    console.error('Feedback: D1 write failed', e)
    return c.json({ error: 'Could not save the feedback right now' }, 502)
  }

  return c.body(null, 204)
})

export { feedback }
