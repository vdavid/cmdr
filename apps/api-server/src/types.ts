export type Bindings = {
  // KV namespace for license code -> full key mappings
  LICENSE_CODES: KVNamespace
  // KV namespace for blog post likes
  BLOG_LIKES: KVNamespace
  // KV namespace for tracking-link short codes (the `?r=<code>` -> UTM map served at /r-codes.json)
  LINK_CODES: KVNamespace
  // Analytics Engine for device count tracking (fair use monitoring)
  DEVICE_COUNTS: AnalyticsEngineDataset
  // D1 database for telemetry persistence (crash reports, downloads, update checks, heartbeats)
  TELEMETRY_DB: D1Database
  // Workers rate-limit binding gating POST /heartbeat, keyed by the caller IP (never stored).
  // Optional so tests and incomplete envs can omit it; the route skips the gate when absent.
  HEARTBEAT_LIMITER?: RateLimit
  // Workers rate-limit binding gating POST /beta-signup, keyed by the caller IP (never stored).
  // Tighter than the heartbeat (signups are rare). Optional; the route skips the gate when absent.
  BETA_SIGNUP_LIMITER?: RateLimit
  // Workers rate-limit binding gating POST /feedback, keyed by the caller IP (never stored).
  // Optional; the route skips the gate when absent.
  FEEDBACK_LIMITER?: RateLimit
  // Workers rate-limit binding gating POST /error-report, keyed by the caller IP (never stored).
  // The tightest of the set: each accepted request stores up to 10 MB in R2. Optional; the route
  // skips the gate when absent.
  ERROR_REPORT_LIMITER?: RateLimit
  // Workers rate-limit binding gating POST /error-report/:id/amend, keyed by the caller IP (never
  // stored). Its own binding rather than a share of ERROR_REPORT_LIMITER: an amendment is a tiny
  // JSON POST that stores no bundle, so it can be looser, and a reporter adding a note should never
  // be turned away because their upload just spent the upload allowance. Optional; the route skips
  // the gate when absent.
  ERROR_REPORT_AMEND_LIMITER?: RateLimit
  // Workers rate-limit binding gating POST /crash-report, keyed by the caller IP (never stored).
  // Optional; the route skips the gate when absent.
  CRASH_REPORT_LIMITER?: RateLimit
  // Workers rate-limit binding gating POST and DELETE /likes/:slug, keyed by the caller IP (never
  // stored). Optional; the route skips the gate when absent.
  LIKES_LIMITER?: RateLimit
  // Paddle webhook secrets (both optional to support gradual rollout)
  PADDLE_WEBHOOK_SECRET_LIVE?: string
  PADDLE_WEBHOOK_SECRET_SANDBOX?: string
  // Paddle API keys for validation
  PADDLE_API_KEY_LIVE?: string
  PADDLE_API_KEY_SANDBOX?: string
  // Crypto keys
  ED25519_PRIVATE_KEY: string
  // Secret pepper mixed into every stored IP hash (`/download`, `/update-check`, `/likes/:slug`).
  // The salt beside it is public by design (a date, or a post slug), so this secret is the ONLY
  // thing making those hashes one-way: without it, the IPv4 space brute-forces in seconds. Optional
  // so tests and incomplete envs can omit it (the handler warns and still counts); required in any
  // deployed environment.
  IP_HASH_PEPPER?: string
  // Email
  RESEND_API_KEY: string
  // Config
  PRODUCT_NAME: string
  SUPPORT_EMAIL: string
  // "sandbox" (default) or "live": controls which Paddle API to use for /validate
  PADDLE_ENVIRONMENT?: string
  // Price IDs for license type mapping
  PRICE_ID_COMMERCIAL_SUBSCRIPTION?: string
  PRICE_ID_COMMERCIAL_PERPETUAL?: string
  // Dedicated admin API token for /admin/stats (separate from Paddle secrets)
  ADMIN_API_TOKEN?: string
  // Crash notification email recipient (for cron-based crash alerts)
  CRASH_NOTIFICATION_EMAIL?: string
  // Optional dedicated recipient for the in-app feedback digest. When unset, the cron job falls
  // back to CRASH_NOTIFICATION_EMAIL, so feedback email works with no new secret.
  FEEDBACK_NOTIFICATION_EMAIL?: string
  // R2 bucket for error report bundles (zips of redacted logs + manifest)
  ERROR_REPORTS_BUCKET: R2Bucket
  // KV namespace for error report bookkeeping (total bytes counter, eviction lock flag)
  ERROR_REPORT_META: KVNamespace
  // Discord webhook URL for #error-reports channel notifications
  DISCORD_WEBHOOK_URL?: string
  // Optional dedicated Discord webhook for in-app feedback notifications. When unset,
  // POST /feedback falls back to DISCORD_WEBHOOK_URL so feedback works with no new secret.
  DISCORD_FEEDBACK_WEBHOOK_URL?: string
  // Optional dedicated Discord webhook for beta-tester signup notifications. When unset,
  // POST /beta-signup falls back to DISCORD_WEBHOOK_URL (so pings land in #error-reports until
  // the #beta-signups channel and its webhook exist).
  DISCORD_BETA_SIGNUP_WEBHOOK_URL?: string
  // R2 S3-compatible credentials, used to mint long-TTL presigned download URLs
  // for the Discord embed. Bindings can't presign on their own, but the S3 API can.
  R2_ACCOUNT_ID?: string
  R2_ACCESS_KEY_ID?: string
  R2_SECRET_ACCESS_KEY?: string
  // R2 bucket name (used in presigned URL host/path). Defaults to "cmdr-error-reports".
  R2_ERROR_REPORTS_BUCKET_NAME?: string
  // Listmonk (the beta-tester mailing list). The base URL (for example https://mail.getcmdr.com),
  // the API user and token (sent as `Authorization: token <user>:<token>`), and the numeric id of
  // the double-opt-in "Cmdr beta testers" list. All optional so tests and incomplete envs omit
  // them; POST /beta-signup returns a soft failure when they're absent. The email NEVER co-occurs
  // with any analytics/diagnostics install id, by construction (see /beta-signup).
  LISTMONK_API_URL?: string
  LISTMONK_API_USER?: string
  LISTMONK_API_TOKEN?: string
  LISTMONK_BETA_LIST_ID?: number
  // Numeric id of the "Cmdr newsletter" Listmonk list, read by the funnel endpoint's per-day signups
  // column (it sums this list plus the beta list). Optional; defaults to 3 (the live newsletter list).
  LISTMONK_NEWSLETTER_LIST_ID?: number
}

export interface PaddleWebhookPayload {
  event_type: string
  // Paddle's per-event id (`evt_...`). Stored on the fulfillment row for support and debugging;
  // idempotency keys off the transaction id, since one purchase must yield one set of licenses
  // however many events carry it.
  event_id?: string
  data?: {
    id?: string
    customer_id?: string
    items?: Array<{
      price?: {
        id?: string
      }
      quantity?: number
    }>
    custom_data?: {
      // Paddle preserves the key casing from checkout - we use camelCase
      organizationName?: string
    }
  }
}

export const maxOrganizationNameLength = 500

// KV key for the activation counter, read by /admin/stats.
// Starts from zero on deploy. Initialize via the CF API if you need historical count.
export const activationCountKey = '_meta:activation_count'
export const maxTransactionIdLength = 200

/** "1.23 GB", "456 MB", "789 KB", "12 B". */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes.toString()} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let i = 0
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024
    i++
  }
  return `${value.toFixed(value >= 100 ? 0 : value >= 10 ? 1 : 2)} ${units[i]}`
}

export function isValidEmail(email: string): boolean {
  const atIndex = email.indexOf('@')
  return atIndex > 0 && email.indexOf('.', atIndex) > atIndex + 1
}

/**
 * The loose shape check every REPLY-TO address goes through: crash reports, in-app feedback, error
 * reports, and error-report amendments.
 *
 * Deliberately weaker than {@link isValidEmail}, which gates licensing: a license email that
 * bounces means a paying customer got nothing, so that path is worth being strict about. A reply-to
 * is a favor the person did us, and the cost of over-validating it (silently dropping a real
 * address that our regex disagrees with) is worse than the cost of storing one that bounces.
 */
/**
 * An optional field the Rust client may omit or send as `null`. serde serializes `Option::None` as
 * JSON `null`, not as an absent key (specta's unified mode rejects `skip_serializing_if` on a
 * Tauri command surface), so a validator that only tolerates `undefined` rejects exactly the
 * upgrade-window payloads worth keeping. `null` and `undefined` both mean "absent"; only a
 * present-but-wrong value fails.
 */
export function isAbsent(v: unknown): boolean {
  return v === undefined || v === null
}

export const emailShapePattern = /^[^\s@]+@[^\s@]+$/

/** {@link emailShapePattern} as a predicate, for the validators that want a boolean. */
export function hasEmailShape(value: string): boolean {
  return emailShapePattern.test(value)
}

export function isValidLicenseType(type: string): type is LicenseType {
  return (licenseTypes as readonly string[]).includes(type)
}

/** Redact an email for logging: "john@example.com" -> "j***@example.com" */
export function redactEmail(email: string): string {
  const atIndex = email.indexOf('@')
  if (atIndex <= 0) return '***'
  return email[0] + '***' + email.slice(atIndex)
}

/** Determine Paddle API config from PADDLE_ENVIRONMENT var (default: sandbox). */
export function getPaddleConfig(env: Bindings): { apiKey: string; environment: 'sandbox' | 'live' } | null {
  const environment = env.PADDLE_ENVIRONMENT === 'live' ? 'live' : 'sandbox'
  const apiKey = environment === 'live' ? env.PADDLE_API_KEY_LIVE : env.PADDLE_API_KEY_SANDBOX
  if (!apiKey) return null
  return { apiKey, environment }
}

/** Minimal request shape the header-reading helpers below need. Hono's `c.req` satisfies it. */
interface HeaderReader {
  header: (name: string) => string | undefined
}

/**
 * The caller's IP, as Cloudflare reports it. Used to key rate limiters (transient, never stored)
 * and to derive daily-salted hashes for dedup. `'unknown'` when neither header is present, which
 * lumps such callers into one shared bucket rather than exempting them.
 */
export function callerIp(req: HeaderReader): string {
  return req.header('cf-connecting-ip') ?? req.header('x-forwarded-for') ?? 'unknown'
}

/**
 * Gate a request on a Workers rate-limit binding, keyed by the caller IP. Returns a 429 to
 * return from the route, or null to continue. Call it before parsing a body, so an abusive
 * caller costs no CPU or storage.
 *
 * The binding is optional so tests and incomplete envs can omit it; an absent binding is a no-op.
 *
 * Gotcha: Cloudflare counts these limits per data center, NOT globally
 * (https://developers.cloudflare.com/workers/runtime-apis/bindings/rate-limit/), so one of these
 * bounds a single abusive client, not a distributed flood. Endpoints where a flood is expensive
 * need a global ceiling of their own too (see the error-report daily byte budget in
 * `error-report-intake.ts`).
 */
export async function enforceIpRateLimit(limiter: RateLimit | undefined, req: HeaderReader): Promise<Response | null> {
  if (!limiter) return null
  const { success } = await limiter.limit({ key: callerIp(req) })
  return success ? null : Response.json({ error: 'Too many requests' }, { status: 429 })
}

/**
 * Read a request body, stopping at `maxBytes`. Returns the bytes, or null when the body is over the
 * cap (the read is cancelled at that point, so the rest is never pulled off the socket).
 *
 * **Every route that reads a body under a size limit goes through this**, and ❌ never through
 * `c.req.text()` / `c.req.parseBody()`: `content-length` is advisory, since a chunked upload
 * declares no length and a declared one can lie. Both of those buffer whatever actually arrives
 * before anything can measure it, so a cap applied afterwards is decorative and the isolate (128 MB
 * against Cloudflare's 100 MB request limit) is what gives way. A `content-length` pre-check is
 * still worth keeping as a cheap fast-fail for an honest client; it just can't be the cap.
 *
 * Counting BYTES also means a limit means what it says: `String.length` counts UTF-16 code units,
 * which runs a byte budget roughly 1.5x loose on multi-byte text.
 *
 * Returning null rather than throwing keeps "too large" distinguishable from "malformed body"
 * without matching on a parser message.
 */
export async function readCappedBody(body: ReadableStream<Uint8Array>, maxBytes: number): Promise<ArrayBuffer | null> {
  const reader = body.getReader()
  const chunks: Uint8Array[] = []
  let total = 0
  try {
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      total += value.byteLength
      if (total > maxBytes) {
        await reader.cancel()
        return null
      }
      chunks.push(value)
    }
  } finally {
    reader.releaseLock()
  }

  const buffer = new ArrayBuffer(total)
  const out = new Uint8Array(buffer)
  let offset = 0
  for (const chunk of chunks) {
    out.set(chunk, offset)
    offset += chunk.byteLength
  }
  return buffer
}

/**
 * Hono `c.executionCtx.waitUntil` wrapper that falls back to inline await in tests.
 *
 * Takes only the `waitUntil` shape it actually calls: Hono's `Context.executionCtx` is its own
 * `ExecutionContext<unknown>`, which carries members (`tracing`) that the ambient
 * `@cloudflare/workers-types` `ExecutionContext` doesn't, so naming either type here breaks the
 * other whenever the two drift.
 */
export function scheduleBackground(
  c: { executionCtx: { waitUntil: (promise: Promise<unknown>) => void } },
  work: Promise<void>,
): Promise<void> {
  try {
    c.executionCtx.waitUntil(work)
    return Promise.resolve()
  } catch {
    return work
  }
}

/** Warn at most once per isolate, so a misconfigured deploy is visible without flooding the log. */
let warnedMissingPepper = false

/**
 * Hash a caller IP for storage: `SHA-256(pepper + ip + salt)`. The one place an IP becomes a stored
 * value, so every route that keeps one comes through here.
 *
 * Two independent ingredients, and BOTH are load-bearing:
 *
 * - The **salt** scopes the value so it can't be linked across whatever it separates. Telemetry
 *   passes the UTC day, so a visitor is unlinkable across days; `/likes/:slug` passes the post slug
 *   (a like has to stay recognizable for years, so a rotating salt would forget it), so a reader is
 *   unlinkable across posts. It's public and predictable by design, and provides no secrecy at all.
 * - The **pepper** (`IP_HASH_PEPPER`, a Cloudflare secret) is what makes the hash one-way. Without
 *   it, anyone holding the data recovers the address: IPv4 is 2^32 candidates, which a GPU walks in
 *   seconds, so an unpeppered hash IS the IP in a thin costume, and our privacy policy's "we don't
 *   store your IP address" would be false. ❌ Never drop the pepper to "simplify" the scheme.
 *
 * Rotating the pepper is safe and re-anonymizes every stored row against the old value. It costs one
 * day of same-day dedup accuracy in telemetry, and in likes it costs readers the filled heart on
 * posts they had liked (counts are untouched).
 *
 * A missing secret still hashes (losing the count would be worse than a weak hash) but logs loudly.
 * D1 rows age out of the retention sweep on their own; a likes pseudonym in KV never does, so a
 * pepper set late leaves weak values there until the `likes:` keys are deleted.
 */
export async function hashCallerIp(ip: string, salt: string, pepper: string | undefined): Promise<string> {
  if (!pepper && !warnedMissingPepper) {
    warnedMissingPepper = true
    console.warn('IP_HASH_PEPPER is not set: stored IP hashes are brute-forceable until it is')
  }
  const hashBuffer = await crypto.subtle.digest('SHA-256', new TextEncoder().encode((pepper ?? '') + ip + salt))
  return [...new Uint8Array(hashBuffer)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

/** Verify admin auth and return error response if unauthorized, or null if authorized. */
export function verifyAdminAuth(c: {
  env: Bindings
  req: { header: (name: string) => string | undefined }
}): Response | null {
  const token = c.env.ADMIN_API_TOKEN
  if (!token) {
    return Response.json({ error: 'Admin API not configured' }, { status: 500 })
  }
  const authHeader = c.req.header('Authorization')
  if (!authHeader || !constantTimeEqual(authHeader, `Bearer ${token}`)) {
    return Response.json({ error: 'Unauthorized' }, { status: 401 })
  }
  return null
}

import { licenseTypes, type LicenseType } from './licensing/license'
import { constantTimeEqual } from './licensing/paddle'
