import { Hono } from 'hono'
import { AwsClient } from 'aws4fetch'
import type { Bindings } from './types'
import {
  ERROR_REPORT_PREFIX,
  incrementTotalBytes,
  tryEvict,
  EVICTION_HIGH_WATERMARK,
  EVICTION_LOW_WATERMARK,
} from './error-report-eviction'
import { postErrorReportNotification, postEvictionNotification } from './discord'

const errorReport = new Hono<{ Bindings: Bindings }>()

const MAX_BUNDLE_BYTES = 10 * 1024 * 1024 // 10 MB hard cap on the multipart payload
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

function isValidMeta(value: unknown): value is ErrorReportMeta {
  if (!value || typeof value !== 'object') return false
  const v = value as Record<string, unknown>
  if (typeof v['id'] !== 'string' || !SHORT_ID_PATTERN.test(v['id'])) return false
  if (v['kind'] !== 'user' && v['kind'] !== 'auto') return false
  for (const k of ['appVersion', 'osVersion', 'arch', 'generatedAt']) {
    const val = v[k]
    if (typeof val !== 'string' || val.length === 0) return false
  }
  if (v['userNote'] !== undefined && typeof v['userNote'] !== 'string') return false
  if (v['buildMode'] !== undefined && v['buildMode'] !== 'release' && v['buildMode'] !== 'debug') return false
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

/** Hono `c.executionCtx.waitUntil` wrapper that falls back to inline await in tests. */
function scheduleBackground(c: { executionCtx: ExecutionContext }, work: Promise<void>): Promise<void> {
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
  },
): Promise<void> {
  try {
    await incrementTotalBytes(env.ERROR_REPORT_META, args.sizeBytes)
  } catch (e) {
    console.error('Error report: incrementTotalBytes failed', e)
  }

  let evictionResult: Awaited<ReturnType<typeof tryEvict>> | null = null
  try {
    evictionResult = await tryEvict(env)
  } catch (e) {
    console.error('Error report: tryEvict failed', e)
  }

  if (env.DISCORD_WEBHOOK_URL) {
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
      downloadUrl: downloadUrl ?? '(presign unavailable — fetch via admin)',
      userNote: args.meta.userNote,
    })

    if (evictionResult && 'evictedCount' in evictionResult && evictionResult.evictedCount > 0) {
      await postEvictionNotification(env.DISCORD_WEBHOOK_URL, {
        evictedCount: evictionResult.evictedCount,
        freedBytes: evictionResult.freedBytes,
        newTotalBytes: evictionResult.newTotal,
      })
    }
  }
}

errorReport.post('/error-report', async (c) => {
  // Pre-parse size guard. The body parser would slurp it all anyway, but cheap to fail fast.
  const contentLength = c.req.header('content-length')
  if (contentLength && parseInt(contentLength, 10) > MAX_BUNDLE_BYTES) {
    return c.json({ error: 'Bundle too large (max 10 MB)' }, 413)
  }

  let form: Record<string, string | File>
  try {
    form = await c.req.parseBody()
  } catch {
    return c.json({ error: 'Invalid multipart body' }, 400)
  }

  const bundle = form['bundle']
  const metaRaw = form['meta']

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

  const id = meta.id
  const datePrefix = todayDatePrefix()
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

  await scheduleBackground(c, postUploadWork(c.env, { id, key, sizeBytes, meta, uploadedUnixSeconds }))

  return c.json({ id })
})

export { errorReport, MAX_BUNDLE_BYTES, EVICTION_HIGH_WATERMARK, EVICTION_LOW_WATERMARK }
