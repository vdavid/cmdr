import type { Bindings } from './types'

/**
 * Error report bundle eviction.
 *
 * Bookkeeping lives in KV (`ERROR_REPORT_META`), bundles live in R2
 * (`ERROR_REPORTS_BUCKET`). The KV counter is approximate ground truth — refresh
 * via `recomputeTotal` whenever exact numbers matter.
 *
 * Key patterns:
 * - `total_bytes` — running byte total across all bundles. Approximate, drift-prone.
 * - `eviction_in_progress` — short-lived (60 s TTL) lock to prevent concurrent eviction.
 *
 * The R2 key shape is `error-reports/{prod|dev}/{yyyy-mm-dd}/{ERR-XXXXX}-{uuid}.zip`.
 *
 * Legacy shape (still present in the bucket; aged out via the 90-day R2 lifecycle):
 * `error-reports/{yyyy-mm-dd}/{ERR-XXXXX}-{uuid}.zip`. Eviction sorts oldest-first
 * via {@link extractDateSegment}, which handles both shapes.
 */

/** Eviction starts when total bytes exceed this. */
export const EVICTION_HIGH_WATERMARK = 8 * 1024 ** 3 // 8 GB
/** Eviction stops once total bytes drop to or below this. */
export const EVICTION_LOW_WATERMARK = 6 * 1024 ** 3 // 6 GB

/** R2 key prefix for all error report bundles. */
export const ERROR_REPORT_PREFIX = 'error-reports/'

/** KV keys. */
export const TOTAL_BYTES_KEY = 'total_bytes'
export const EVICTION_LOCK_KEY = 'eviction_in_progress'
const EVICTION_LOCK_TTL_SECONDS = 60

export type TryEvictResult =
  | { evictedCount: number; freedBytes: number; newTotal: number }
  | { skipped: 'lock_held' | 'under_threshold' }

/**
 * Atomically-ish add `deltaBytes` to the running total.
 * Race condition: KV doesn't support atomic increment. Concurrent uploads can
 * lose updates. The daily cron (`recomputeTotal` + conditional `tryEvict`)
 * corrects drift. Same pattern as `_meta:activation_count`.
 */
export async function incrementTotalBytes(kv: KVNamespace, deltaBytes: number): Promise<number> {
  const current = parseInt((await kv.get(TOTAL_BYTES_KEY)) ?? '0', 10)
  const next = current + deltaBytes
  await kv.put(TOTAL_BYTES_KEY, String(next))
  return next
}

/**
 * Walk the entire R2 prefix and sum object sizes. Used by `tryEvict` after
 * deletion (to reset the counter) and by the daily cron sweep (to correct drift).
 */
export async function recomputeTotal(
  env: Pick<Bindings, 'ERROR_REPORTS_BUCKET' | 'ERROR_REPORT_META'>,
): Promise<number> {
  let total = 0
  let cursor: string | undefined
  do {
    const list = await env.ERROR_REPORTS_BUCKET.list({ prefix: ERROR_REPORT_PREFIX, cursor })
    for (const obj of list.objects) {
      total += obj.size
    }
    cursor = list.truncated ? list.cursor : undefined
  } while (cursor)
  await env.ERROR_REPORT_META.put(TOTAL_BYTES_KEY, String(total))
  return total
}

interface ListedObject {
  key: string
  size: number
  uploaded: Date
}

/**
 * Pull the `yyyy-mm-dd` date segment out of an R2 key. Handles both shapes:
 * - new: `error-reports/{prod|dev}/yyyy-mm-dd/{id}-{uuid}.zip`
 * - legacy: `error-reports/yyyy-mm-dd/{id}-{uuid}.zip`
 *
 * Returns the empty string when the key matches neither shape — that pushes
 * unrecognized keys to the front of an ascending sort, so they get evicted first.
 */
export function extractDateSegment(key: string): string {
  if (!key.startsWith(ERROR_REPORT_PREFIX)) return ''
  const rest = key.slice(ERROR_REPORT_PREFIX.length)
  const segments = rest.split('/')
  // Pick the first segment that looks like a date. Both shapes have exactly one.
  for (const segment of segments) {
    if (/^\d{4}-\d{2}-\d{2}$/.test(segment)) return segment
  }
  return ''
}

/** Fetch every object under the prefix into memory. R2 list page = 1000 objects max. */
async function listAllObjects(bucket: R2Bucket): Promise<ListedObject[]> {
  const out: ListedObject[] = []
  let cursor: string | undefined
  do {
    const list = await bucket.list({ prefix: ERROR_REPORT_PREFIX, cursor })
    for (const obj of list.objects) {
      out.push({ key: obj.key, size: obj.size, uploaded: obj.uploaded })
    }
    cursor = list.truncated ? list.cursor : undefined
  } while (cursor)
  return out
}

/**
 * If total bytes exceed `highWatermark`, delete oldest objects (sorted by date prefix
 * in the R2 key, then by upload time) until total ≤ `lowWatermark`. Holds a KV lock
 * to prevent concurrent eviction. Recomputes the counter from R2 ground truth before
 * returning. Best-effort: clears the lock even on error.
 */
export async function tryEvict(
  env: Pick<Bindings, 'ERROR_REPORTS_BUCKET' | 'ERROR_REPORT_META'>,
  options: { highWatermark?: number; lowWatermark?: number } = {},
): Promise<TryEvictResult> {
  const high = options.highWatermark ?? EVICTION_HIGH_WATERMARK
  const low = options.lowWatermark ?? EVICTION_LOW_WATERMARK

  const lock = await env.ERROR_REPORT_META.get(EVICTION_LOCK_KEY)
  if (lock) return { skipped: 'lock_held' }

  const current = parseInt((await env.ERROR_REPORT_META.get(TOTAL_BYTES_KEY)) ?? '0', 10)
  if (current <= high) return { skipped: 'under_threshold' }

  await env.ERROR_REPORT_META.put(EVICTION_LOCK_KEY, '1', { expirationTtl: EVICTION_LOCK_TTL_SECONDS })

  let evictedCount = 0
  let freedBytes = 0
  try {
    const all = await listAllObjects(env.ERROR_REPORTS_BUCKET)
    // Sort oldest first. The date segment inside the key is the primary signal
    // (yyyy-mm-dd sorts lexically) — extracted via `extractDateSegment` so the
    // sort works across both the new `{env}/{date}` layout and the legacy
    // `{date}` layout. R2 `uploaded` breaks ties for same-day uploads.
    all.sort((a, b) => {
      const da = extractDateSegment(a.key)
      const db = extractDateSegment(b.key)
      if (da < db) return -1
      if (da > db) return 1
      return a.uploaded.getTime() - b.uploaded.getTime()
    })

    let runningTotal = all.reduce((s, o) => s + o.size, 0)
    for (const obj of all) {
      if (runningTotal <= low) break
      await env.ERROR_REPORTS_BUCKET.delete(obj.key)
      runningTotal -= obj.size
      freedBytes += obj.size
      evictedCount++
    }

    const newTotal = await recomputeTotal(env)
    return { evictedCount, freedBytes, newTotal }
  } finally {
    // Best-effort lock release. KV TTL would clear it anyway after 60 s.
    await env.ERROR_REPORT_META.delete(EVICTION_LOCK_KEY).catch(() => {})
  }
}
