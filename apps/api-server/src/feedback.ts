import { Hono, type Context } from 'hono'
import type { Bindings } from './types'
import { postFeedbackNotification } from './discord'

const feedback = new Hono<{ Bindings: Bindings }>()

/**
 * In-app "Send feedback" ingestion. JSON body (no log bundle, unlike `/error-report`):
 * required `feedback` text plus `appVersion` / `osVersion`, optional reply-to `email` and
 * `buildMode`. The row is written to the D1 `feedback` table (the durable sink; the
 * write is awaited so a failure surfaces as a retryable soft 502, not a silent drop),
 * then a Discord ping goes out in the background with a truncated preview.
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

/** Same loose reply-only shape check as the crash reporter; we never over-validate emails. */
const emailShapePattern = /^[^\s@]+@[^\s@]+$/

interface FeedbackBody {
  feedback: string
  appVersion: string
  osVersion: string
  email?: string | null
  buildMode?: 'release' | 'debug' | null
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
  const email = body.email
  if (email !== undefined && email !== null && (typeof email !== 'string' || !emailShapePattern.test(email))) {
    return 'Invalid email'
  }
  const buildMode = body.buildMode
  if (buildMode !== undefined && buildMode !== null && buildMode !== 'release' && buildMode !== 'debug') {
    return 'Invalid buildMode'
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
  // window only and is never stored. The binding is optional, so the gate is a no-op
  // when absent (tests, incomplete envs). Same pattern as /heartbeat and /beta-signup.
  const ip = c.req.header('cf-connecting-ip') ?? c.req.header('x-forwarded-for') ?? 'unknown'
  if (c.env.FEEDBACK_LIMITER) {
    const { success } = await c.env.FEEDBACK_LIMITER.limit({ key: ip })
    if (!success) {
      return c.json({ error: 'Too many requests' }, 429)
    }
  }

  const parsed = await readFeedbackBody(c)
  if (parsed instanceof Response) return parsed

  const validationError = validateFeedbackShape(parsed)
  if (validationError) {
    return c.json({ error: validationError }, 400)
  }
  const body = parsed as unknown as FeedbackBody
  const text = body.feedback.trim()

  // D1 is the durable sink (Discord truncates long messages), so this write is awaited:
  // the desktop app should surface a retry on failure rather than pretend it landed.
  try {
    await c.env.TELEMETRY_DB.prepare(
      `INSERT INTO feedback (feedback, email, app_version, os_version, build_mode)
         VALUES (?, ?, ?, ?, ?)`,
    )
      .bind(text, body.email ?? null, body.appVersion, body.osVersion, body.buildMode ?? null)
      .run()
  } catch (e) {
    console.error('Feedback: D1 write failed', e)
    return c.json({ error: 'Could not save the feedback right now' }, 502)
  }

  // Discord ping rides in the background after the 204 has shipped. A dedicated feedback
  // webhook (separate channel) wins when configured; otherwise reuse the error-report one.
  const webhookUrl = c.env.DISCORD_FEEDBACK_WEBHOOK_URL ?? c.env.DISCORD_WEBHOOK_URL
  if (webhookUrl) {
    const notify = postFeedbackNotification(webhookUrl, {
      buildMode: body.buildMode ?? 'release',
      appVersion: body.appVersion,
      osVersion: body.osVersion,
      email: body.email ?? undefined,
      feedback: text,
    })
    try {
      c.executionCtx.waitUntil(notify)
    } catch {
      // executionCtx unavailable (for example, in tests); await inline as fallback
      await notify
    }
  }

  return c.body(null, 204)
})

export { feedback }
