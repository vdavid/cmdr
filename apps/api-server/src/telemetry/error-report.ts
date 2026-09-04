import { Hono, type Context } from 'hono'
import { AwsClient } from 'aws4fetch'
import {
  enforceIpRateLimit,
  hasEmailShape,
  isAbsent,
  readCappedBody,
  scheduleBackground,
  type Bindings,
} from '../types'
import {
  ERROR_REPORT_PREFIX,
  incrementTotalBytes,
  tryEvict,
  EVICTION_HIGH_WATERMARK,
  EVICTION_LOW_WATERMARK,
} from './error-report-eviction'
import {
  DAILY_ERROR_REPORT_EMAIL_CAP,
  DAILY_INTAKE_BUDGET_BYTES,
  DAILY_NOTIFICATION_CAP,
  checkIntakeAllowed,
  claimBudgetAlert,
  claimErrorReportEmailSlot,
  claimNotificationSlot,
  recordIntakeBytes,
  type IntakeRejection,
} from './error-report-intake'
import { hashAmendKey, mintAmendKey, writeReportIndex } from './error-report-amend'
import { humanReportRecipient } from '../email/send'
import { sendErrorReportNotificationEmail, sendErrorReportsSuppressedEmail } from '../email/error-report'
import {
  postErrorReportNotification,
  postEvictionBlockedNotification,
  postEvictionNotification,
  postIntakeRejectedNotification,
  postNotificationsSuppressedNotification,
} from '../discord'

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

const PRESIGN_TTL_SECONDS = 7 * 24 * 60 * 60 // R2 max for presigned URLs

/** The same window in the unit the notification copy states, so the two can't drift apart. */
const PRESIGN_TTL_DAYS = PRESIGN_TTL_SECONDS / (24 * 60 * 60)
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
  /**
   * Reply-to address, set only when the person ticked "Attach my email" in the send dialog. The
   * auto-dispatcher never sets it: enabling auto-send is not consent to ship an address on every
   * report (enforced client-side by `bundle_builder::email_for_kind`). Shape-checked loosely, like
   * every other reply-to we take.
   */
  email?: string
  generatedAt: string
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
  return hasValidOptionalMetaFields(v)
}

/**
 * The manifest fields a client may omit. Each tolerates `null` as well as `undefined` (see
 * {@link isAbsent}); only a present-but-wrong value fails.
 */
function hasValidOptionalMetaFields(v: Record<string, unknown>): boolean {
  if (!isAbsent(v['userNote']) && typeof v['userNote'] !== 'string') return false
  if (!isAbsent(v['buildMode']) && v['buildMode'] !== 'release' && v['buildMode'] !== 'debug') return false
  if (!isAbsent(v['email']) && (typeof v['email'] !== 'string' || !hasEmailShape(v['email']))) return false
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
 * A lazy, memoized presigned GET URL for one bundle. Both notification channels hand out the same
 * link, an upload that notifies neither mints none, and a failure to sign is logged once and
 * reported as `null` rather than sinking the notification.
 */
function createPresigner(env: Bindings, key: string): () => Promise<string | null> {
  let attempted = false
  let url: string | null = null
  return async () => {
    if (attempted) return url
    attempted = true
    try {
      url = await buildPresignedUrl(env, key)
    } catch (e) {
      console.error('Error report: presign failed', e)
    }
    return url
  }
}

/** What `postUploadWork` and its helpers know about the upload they're following up on. */
interface UploadedReport {
  id: string
  key: string
  sizeBytes: number
  meta: ErrorReportMeta
  uploadedUnixSeconds: number
  date: string
}

/**
 * Everything `#error-reports` hears about one upload: the per-upload embed (capped for the day)
 * and the eviction alerts, which are not capped because they are rare and are the ones worth
 * waking up for.
 */
async function notifyDiscord(
  env: Bindings,
  webhookUrl: string,
  args: UploadedReport,
  presignedUrl: () => Promise<string | null>,
  evictionResult: Awaited<ReturnType<typeof tryEvict>> | null,
): Promise<void> {
  const decision = await claimNotificationSlot(env.ERROR_REPORT_META, args.date)
  if (decision === 'suppress-notice') {
    await postNotificationsSuppressedNotification(webhookUrl, { cap: DAILY_NOTIFICATION_CAP, date: args.date })
  }
  if (decision === 'notify') {
    await postErrorReportNotification(webhookUrl, {
      id: args.id,
      kind: args.meta.kind,
      buildMode: args.meta.buildMode ?? 'release',
      appVersion: args.meta.appVersion,
      osVersion: args.meta.osVersion,
      arch: args.meta.arch,
      sizeBytes: args.sizeBytes,
      uploadedUnixSeconds: args.uploadedUnixSeconds,
      downloadUrl: (await presignedUrl()) ?? '(presign unavailable; fetch via admin)',
      userNote: args.meta.userNote,
    })
  }

  if (evictionResult?.outcome === 'evicted' && evictionResult.evictedCount > 0) {
    await postEvictionNotification(webhookUrl, {
      evictedCount: evictionResult.evictedCount,
      freedBytes: evictionResult.freedBytes,
      newTotalBytes: evictionResult.newTotal,
    })
  }
  if (evictionResult?.outcome === 'paused') {
    await postEvictionBlockedNotification(webhookUrl, evictionResult)
  }
}

/**
 * Mail one hand-written report to whoever reads them. Auto-sends never reach here: one misbehaving
 * install produced 50+ auto bundles in three days, and that volume in an inbox buries everything
 * else, while Discord absorbs it. `kind` comes from the client's manifest, so a daily cap bounds
 * what a build that mislabels its auto-sends can cost.
 *
 * Silent no-op when no recipient or Resend key is configured.
 */
async function mailUserErrorReport(
  env: Bindings,
  args: UploadedReport,
  presignedUrl: () => Promise<string | null>,
): Promise<void> {
  const to = humanReportRecipient(env)
  if (!to || !env.RESEND_API_KEY) return

  const decision = await claimErrorReportEmailSlot(env.ERROR_REPORT_META, args.date)
  if (decision === 'silent') return
  if (decision === 'suppress-notice') {
    await sendErrorReportsSuppressedEmail({
      cap: DAILY_ERROR_REPORT_EMAIL_CAP,
      date: args.date,
      to,
      resendApiKey: env.RESEND_API_KEY,
    })
    return
  }

  await sendErrorReportNotificationEmail({
    report: {
      id: args.id,
      buildMode: args.meta.buildMode ?? 'release',
      appVersion: args.meta.appVersion,
      osVersion: args.meta.osVersion,
      arch: args.meta.arch,
      sizeBytes: args.sizeBytes,
      userNote: args.meta.userNote,
      email: args.meta.email,
      downloadUrl: await presignedUrl(),
      linkTtlDays: PRESIGN_TTL_DAYS,
    },
    to,
    resendApiKey: env.RESEND_API_KEY,
  })
}

/**
 * Background work that runs after the 200 has already shipped:
 * update the bytes counter, maybe evict, post Discord notification, mail a hand-written report.
 * Wrapped to never throw; failures here are logged, not propagated.
 */
async function postUploadWork(env: Bindings, args: UploadedReport): Promise<void> {
  const presignedUrl = createPresigner(env, args.key)

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
    // Own try/catch like every other side effect here, so a KV or webhook hiccup can't take the
    // email below down with it.
    try {
      await notifyDiscord(env, env.DISCORD_WEBHOOK_URL, args, presignedUrl, evictionResult)
    } catch (e) {
      console.error('Error report: Discord notification failed', e)
    }
  }

  if (args.meta.kind === 'user') {
    // A mail problem is ours, never the reporter's: the bundle is already stored and the client
    // has its 200.
    try {
      await mailUserErrorReport(env, args, presignedUrl)
    } catch (e) {
      console.error('Error report: notification email failed', e)
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

  // The ONE thing that runs before the 200 rather than in `postUploadWork`. The client is handed
  // its amend credential in this response, so an index written afterwards would be a credential
  // that opens nothing for however long the background work takes. It's a single small KV put.
  //
  // A failed put costs the amend flow, not the report: the bundle is already in R2, so the answer
  // is a 200 carrying `amendKey: null`, which clients treat as "amending isn't available for this
  // one".
  const amendKey = mintAmendKey()
  let issuedAmendKey: string | null = amendKey
  try {
    await writeReportIndex(c.env.ERROR_REPORT_META, id, {
      env,
      date: datePrefix,
      key,
      amendKeyHash: await hashAmendKey(amendKey),
    })
  } catch (e) {
    issuedAmendKey = null
    console.error('Error report: writeReportIndex failed; the report cannot be amended', e)
  }

  await scheduleBackground(c, postUploadWork(c.env, { id, key, sizeBytes, meta, uploadedUnixSeconds, date: today }))

  return c.json({ id, amendKey: issuedAmendKey })
})

export { errorReport, MAX_BUNDLE_BYTES, EVICTION_HIGH_WATERMARK, EVICTION_LOW_WATERMARK }
