import { Hono, type Context } from 'hono'
import { AwsClient } from 'aws4fetch'
import { enforceIpRateLimit, type Bindings } from './types'
import {
  ERROR_REPORT_PREFIX,
  incrementTotalBytes,
  tryEvict,
  EVICTION_HIGH_WATERMARK,
  EVICTION_LOW_WATERMARK,
} from './error-report-eviction'
import {
  DAILY_INTAKE_BUDGET_BYTES,
  DAILY_NOTIFICATION_CAP,
  checkIntakeAllowed,
  claimBudgetAlert,
  claimNotificationSlot,
  recordIntakeBytes,
  type IntakeRejection,
} from './error-report-intake'
import {
  postErrorReportNotification,
  postEvictionBlockedNotification,
  postEvictionNotification,
  postIntakeRejectedNotification,
  postNotificationsSuppressedNotification,
} from './discord'

const errorReport = new Hono<{ Bindings: Bindings }>()

const MAX_BUNDLE_BYTES = 10 * 1024 * 1024 // 10 MB hard cap on the bundle part

/**
 * Hard cap on the whole multipart body, enforced against bytes actually read.
 *
 * The slack over {@link MAX_BUNDLE_BYTES} covers the `meta` part and the multipart framing. The
 * client caps `userNote` at 100,000 chars (`validate_user_note` in the desktop commands layer),
 * which is at most ~400 KB of UTF-8, so 1 MB leaves room without inviting a second payload.
 */
const MAX_BODY_BYTES = MAX_BUNDLE_BYTES + 1024 * 1024

/**
 * Read a request body, stopping at `maxBytes`. Returns the bytes, or null when the body is over
 * the cap (the read is cancelled at that point, so the rest is never pulled off the socket).
 *
 * `content-length` can't carry this weight: a chunked upload declares no length, and a declared
 * one can lie, either way leaving the multipart parser to buffer up to Cloudflare's 100 MB request
 * limit inside a 128 MB isolate. Reading it ourselves bounds that at `maxBytes` no matter what the
 * headers claim.
 *
 * Returning null rather than throwing keeps "too large" distinguishable from "malformed multipart"
 * without matching on a parser message (the `no-string-matching` rule).
 */
async function readCappedBody(body: ReadableStream<Uint8Array>, maxBytes: number): Promise<ArrayBuffer | null> {
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
const PRESIGN_TTL_SECONDS = 7 * 24 * 60 * 60 // R2 max for presigned URLs
const DEFAULT_BUCKET_NAME = 'cmdr-error-reports'

/**
 * Matches the client-side `ERR-XXXXX` short ID produced by
 * `error_reporter::generate_short_id` (alphabet kept in sync in
 * `apps/desktop/src-tauri/src/short_id.rs`).
 */
const SHORT_ID_PATTERN = /^ERR-[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{5}$/

export interface ErrorReportMeta {
  /**
   * Client-generated `ERR-XXXXX` shown in the UI before upload. The server uses this
   * id as-is. The trailing UUID in the R2 key guarantees object uniqueness, so we
   * never regenerate. The server validates the shape and rejects malformed ids.
   */
  id: string
  kind: 'user' | 'auto'
  /**
   * Set by the desktop client from `cfg!(debug_assertions)`. `'debug'` reports
   * come from a dev build of the app; the Discord notification gets a `[DEV]`
   * prefix so triage can keep them apart from production traffic. Optional for
   * backwards compatibility with older clients that didn't set it; unset is
   * treated as `'release'`.
   */
  buildMode?: 'release' | 'debug'
  appVersion: string
  osVersion: string
  arch: string
  userNote?: string
  generatedAt: string
}

/**
 * An optional field the Rust client may omit or send as `null`. serde serializes
 * `Option::None` as JSON `null`, so a validator that only tolerates `undefined`
 * rejects a note-less report with a 400 (the bug that once broke sending). `null`
 * and `undefined` both mean "absent"; only a present-but-wrong value fails.
 */
function isAbsent(v: unknown): boolean {
  return v === undefined || v === null
}

function isValidMeta(value: unknown): value is ErrorReportMeta {
  if (!value || typeof value !== 'object') return false
  const v = value as Record<string, unknown>
  if (typeof v['id'] !== 'string' || !SHORT_ID_PATTERN.test(v['id'])) return false
  if (v['kind'] !== 'user' && v['kind'] !== 'auto') return false
  for (const k of ['appVersion', 'osVersion', 'arch', 'generatedAt']) {
    const val = v[k]
    if (typeof val !== 'string' || val.length === 0) return false
  }
  if (!isAbsent(v['userNote']) && typeof v['userNote'] !== 'string') return false
  if (!isAbsent(v['buildMode']) && v['buildMode'] !== 'release' && v['buildMode'] !== 'debug') return false
  return true
}

function todayDatePrefix(): string {
  return new Date().toISOString().slice(0, 10) // YYYY-MM-DD
}

/** `'prod'` for release builds, `'dev'` for debug. Friendlier than `release`/`debug` for ops. */
function envSegment(buildMode: 'release' | 'debug' | undefined): 'prod' | 'dev' {
  return buildMode === 'debug' ? 'dev' : 'prod'
}

/**
 * R2 key shape: `error-reports/{prod|dev}/yyyy-mm-dd/{ERR-XXXXX}-{uuid}.zip`.
 * Env first so dev and prod sort into separate sub-prefixes (eviction by oldest still
 * works within each environment because the date segment sorts lexically).
 */
function buildR2Key(env: 'prod' | 'dev', datePrefix: string, id: string, uuid: string): string {
  return `${ERROR_REPORT_PREFIX}${env}/${datePrefix}/${id}-${uuid}.zip`
}

/**
 * Build a 7-day presigned GET URL using the R2 S3-compatible API.
 * Returns null if R2 credentials aren't configured.
 */
async function buildPresignedUrl(env: Bindings, key: string): Promise<string | null> {
  if (!env.R2_ACCOUNT_ID || !env.R2_ACCESS_KEY_ID || !env.R2_SECRET_ACCESS_KEY) return null
  const bucketName = env.R2_ERROR_REPORTS_BUCKET_NAME ?? DEFAULT_BUCKET_NAME
  const client = new AwsClient({
    accessKeyId: env.R2_ACCESS_KEY_ID,
    secretAccessKey: env.R2_SECRET_ACCESS_KEY,
    service: 's3',
    region: 'auto',
  })
  const url = new URL(`https://${env.R2_ACCOUNT_ID}.r2.cloudflarestorage.com/${bucketName}/${key}`)
  url.searchParams.set('X-Amz-Expires', PRESIGN_TTL_SECONDS.toString())
  // `client.sign` returns a `Request` whose `url` is a string (per AwsClient typings).
  const signed = await client.sign(url, { method: 'GET', aws: { signQuery: true } })
  return signed.url
}

/**
 * Hono `c.executionCtx.waitUntil` wrapper that falls back to inline await in tests.
 *
 * Takes only the `waitUntil` shape it actually calls: Hono's `Context.executionCtx` is its own
 * `ExecutionContext<unknown>`, which carries members (`tracing`) that the ambient
 * `@cloudflare/workers-types` `ExecutionContext` doesn't, so naming either type here breaks the
 * other whenever the two drift.
 */
function scheduleBackground(
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

/**
 * Background work that runs after the 200 has already shipped:
 * update the bytes counter, maybe evict, post Discord notification.
 * Wrapped to never throw; failures here are logged, not propagated.
 */
async function postUploadWork(
  env: Bindings,
  args: {
    id: string
    key: string
    sizeBytes: number
    meta: ErrorReportMeta
    uploadedUnixSeconds: number
    date: string
  },
): Promise<void> {
  try {
    await incrementTotalBytes(env.ERROR_REPORT_META, args.sizeBytes)
  } catch (e) {
    console.error('Error report: incrementTotalBytes failed', e)
  }

  try {
    await recordIntakeBytes(env.ERROR_REPORT_META, args.date, args.sizeBytes)
  } catch (e) {
    console.error('Error report: recordIntakeBytes failed', e)
  }

  let evictionResult: Awaited<ReturnType<typeof tryEvict>> | null = null
  try {
    evictionResult = await tryEvict(env)
  } catch (e) {
    console.error('Error report: tryEvict failed', e)
  }

  if (env.DISCORD_WEBHOOK_URL) {
    // Per-upload pings are capped for the day; the eviction alerts below are not. Those are rare
    // and are the ones worth waking up for.
    const decision = await claimNotificationSlot(env.ERROR_REPORT_META, args.date)
    if (decision === 'suppress-notice') {
      await postNotificationsSuppressedNotification(env.DISCORD_WEBHOOK_URL, {
        cap: DAILY_NOTIFICATION_CAP,
        date: args.date,
      })
    }
    if (decision === 'notify') {
      let downloadUrl: string | null = null
      try {
        downloadUrl = await buildPresignedUrl(env, args.key)
      } catch (e) {
        console.error('Error report: presign failed', e)
      }
      await postErrorReportNotification(env.DISCORD_WEBHOOK_URL, {
        id: args.id,
        kind: args.meta.kind,
        buildMode: args.meta.buildMode ?? 'release',
        appVersion: args.meta.appVersion,
        osVersion: args.meta.osVersion,
        arch: args.meta.arch,
        sizeBytes: args.sizeBytes,
        uploadedUnixSeconds: args.uploadedUnixSeconds,
        downloadUrl: downloadUrl ?? '(presign unavailable; fetch via admin)',
        userNote: args.meta.userNote,
      })
    }

    if (evictionResult?.outcome === 'evicted' && evictionResult.evictedCount > 0) {
      await postEvictionNotification(env.DISCORD_WEBHOOK_URL, {
        evictedCount: evictionResult.evictedCount,
        freedBytes: evictionResult.freedBytes,
        newTotalBytes: evictionResult.newTotal,
      })
    }
    if (evictionResult?.outcome === 'paused') {
      await postEvictionBlockedNotification(env.DISCORD_WEBHOOK_URL, evictionResult)
    }
  }
}

/**
 * How long a turned-away client should wait. One hour covers the common case (a pause cleared by
 * the next cron sweep) without pinning a client to a precise reopening time we can't promise.
 */
const INTAKE_RETRY_AFTER_SECONDS = 60 * 60

/** 503 + `Retry-After` for a request the global gates turned away. */
function intakeRejectedResponse(reason: IntakeRejection): Response {
  const message =
    reason === 'paused'
      ? "Error report intake is paused right now. Your report wasn't sent."
      : "Error report intake is at its limit for today. Your report wasn't sent."
  return Response.json(
    { error: message },
    { status: 503, headers: { 'Retry-After': INTAKE_RETRY_AFTER_SECONDS.toString() } },
  )
}

/** The route context the upload helpers below receive. */
type UploadContext = Context<{ Bindings: Bindings }>

/**
 * Run the global admission gates (daily byte budget + intake pause) and, when the budget is what
 * ran out, claim the day's single Discord ping. Returns the 503 to send, or null to continue.
 */
async function enforceIntakeGates(c: UploadContext, today: string): Promise<Response | null> {
  const decision = await checkIntakeAllowed(c.env.ERROR_REPORT_META, today)
  if (decision.accept) return null

  console.error(`Error report: intake rejected (${decision.reason})`)
  const webhookUrl = c.env.DISCORD_WEBHOOK_URL
  if (decision.reason === 'daily_budget' && webhookUrl) {
    // One ping per day, not one per rejected upload; a flood is exactly when this fires.
    await scheduleBackground(
      c,
      claimBudgetAlert(c.env.ERROR_REPORT_META, today).then(async (claimed) => {
        if (claimed) {
          await postIntakeRejectedNotification(webhookUrl, { budgetBytes: DAILY_INTAKE_BUDGET_BYTES, date: today })
        }
      }),
    )
  }
  return intakeRejectedResponse(decision.reason)
}

/**
 * Read the multipart body under a hard byte cap and validate both parts. Returns the error
 * Response to send, or the validated bundle and manifest.
 */
async function readReportUpload(c: UploadContext): Promise<Response | { bundle: File; meta: ErrorReportMeta }> {
  // Fast-fail on a declared oversize. Advisory only: an honest client saves everyone the transfer,
  // but the authority is the byte cap on the stream below.
  const contentLength = c.req.header('content-length')
  if (contentLength && parseInt(contentLength, 10) > MAX_BODY_BYTES) {
    return c.json({ error: 'Bundle too large (max 10 MB)' }, 413)
  }

  const rawBody = c.req.raw.body
  if (!rawBody) {
    return c.json({ error: 'Missing request body' }, 400)
  }

  // Read under our own cap rather than handing the socket to `c.req.parseBody()`, which buffers
  // whatever arrives.
  const bytes = await readCappedBody(rawBody, MAX_BODY_BYTES)
  if (!bytes) {
    return c.json({ error: 'Bundle too large (max 10 MB)' }, 413)
  }

  let form: FormData
  try {
    form = await new Response(bytes, {
      headers: { 'content-type': c.req.header('content-type') ?? '' },
    }).formData()
  } catch {
    return c.json({ error: 'Invalid multipart body' }, 400)
  }

  const bundle = form.get('bundle')
  const metaRaw = form.get('meta')

  if (!(bundle instanceof File)) {
    return c.json({ error: 'Missing or invalid "bundle" file part' }, 400)
  }
  if (typeof metaRaw !== 'string') {
    return c.json({ error: 'Missing or invalid "meta" field' }, 400)
  }
  if (bundle.size > MAX_BUNDLE_BYTES) {
    return c.json({ error: 'Bundle too large (max 10 MB)' }, 413)
  }

  let meta: unknown
  try {
    meta = JSON.parse(metaRaw)
  } catch {
    return c.json({ error: 'Malformed "meta" JSON' }, 400)
  }
  if (!isValidMeta(meta)) {
    return c.json({ error: 'Invalid meta shape' }, 400)
  }

  return { bundle, meta }
}

errorReport.post('/error-report', async (c) => {
  // Rate-limit by the caller IP before touching the body: every accepted request stores up to
  // 10 MB in R2 and posts a Discord notification, so an ungated flood is expensive.
  const limited = await enforceIpRateLimit(c.env.ERROR_REPORT_LIMITER, c.req)
  if (limited) return limited

  // Global gates. The per-IP limiter above counts per data center, so these are what actually
  // bound a distributed flood. See `error-report-intake.ts`.
  const today = todayDatePrefix()
  const rejected = await enforceIntakeGates(c, today)
  if (rejected) return rejected

  const upload = await readReportUpload(c)
  if (upload instanceof Response) return upload
  const { bundle, meta } = upload

  const id = meta.id
  // Same day the intake gate charged above, so the key prefix and the budget never disagree
  // across a UTC midnight mid-request.
  const datePrefix = today
  const env = envSegment(meta.buildMode)
  // The trailing UUID guarantees object uniqueness on its own. On the astronomically
  // rare (id, date, uuid) collision, retry with a fresh UUID, never a fresh id, so
  // the user-visible id the dialog showed stays stable.
  let key = buildR2Key(env, datePrefix, id, crypto.randomUUID())
  for (let attempt = 0; attempt < 3; attempt++) {
    const existing = await c.env.ERROR_REPORTS_BUCKET.head(key)
    if (!existing) break
    key = buildR2Key(env, datePrefix, id, crypto.randomUUID())
  }
  const sizeBytes = bundle.size
  const uploadedUnixSeconds = Math.floor(Date.now() / 1000)

  // R2 supports streaming directly from the File body. No buffering needed.
  await c.env.ERROR_REPORTS_BUCKET.put(key, bundle.stream(), {
    httpMetadata: { contentType: 'application/zip' },
    customMetadata: {
      id,
      kind: meta.kind,
      appVersion: meta.appVersion,
      osVersion: meta.osVersion,
      arch: meta.arch,
      generatedAt: meta.generatedAt,
    },
  })

  await scheduleBackground(c, postUploadWork(c.env, { id, key, sizeBytes, meta, uploadedUnixSeconds, date: today }))

  return c.json({ id })
})

export { errorReport, MAX_BUNDLE_BYTES, EVICTION_HIGH_WATERMARK, EVICTION_LOW_WATERMARK }
