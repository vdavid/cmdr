import { Hono } from 'hono'
import { constantTimeEqual } from '../licensing/paddle'
import { type Bindings, activationCountKey, verifyAdminAuth } from '../types'
import { extractDateSegment, isAmendSidecarKey } from '../telemetry/error-report-eviction'

const admin = new Hono<{ Bindings: Bindings }>()

const validDownloadRanges = new Set(['24h', '7d', '30d', 'all'])
const validActiveUserRanges = new Set(['7d', '30d', '90d', 'all'])
const validCrashRanges = new Set(['7d', '30d', '90d', 'all'])
const validHeartbeatRanges = new Set(['7d', '30d', '90d', 'all'])
const validFeedbackRanges = new Set(['7d', '30d', '90d', 'all'])
const validErrorReportRanges = new Set(['7d', '30d', '90d', 'all'])

// Values are hardcoded, never from user input, so it's safe to interpolate into SQL.
const rangeToSqliteInterval: Record<string, string> = {
  '24h': '-1 day',
  '7d': '-7 days',
  '30d': '-30 days',
  '90d': '-90 days',
}

// Days per range, for filtering R2-listed error reports by their key's date segment.
const rangeToDays: Record<string, number> = {
  '7d': 7,
  '30d': 30,
  '90d': 90,
}

// Error report bundles are keyed `error-reports/{prod|dev}/{date}/{id}-{uuid}.zip`. The dashboard
// surfaces real-user reports only, so we list the prod prefix and leave dev (E2E/test noise) out.
const errorReportProdPrefix = 'error-reports/prod/'

interface ErrorReportRow {
  id: string
  kind: string
  appVersion: string
  osVersion: string
  arch: string
  date: string
  generatedAt: string
}

/**
 * One listed bundle as a row. Every metadata field is treated as possibly absent: a bundle could in
 * principle have been stored without one, and a missing field should cost that column, not the row.
 */
function toErrorReportRow(obj: R2Object, date: string): ErrorReportRow {
  const meta: Record<string, string | undefined> = obj.customMetadata ?? {}
  return {
    id: meta.id ?? '',
    kind: meta.kind ?? '',
    appVersion: meta.appVersion ?? '',
    osVersion: meta.osVersion ?? '',
    arch: meta.arch ?? '',
    date: date || obj.uploaded.toISOString().slice(0, 10),
    generatedAt: meta.generatedAt ?? obj.uploaded.toISOString(),
  }
}

/**
 * Lists prod error-report bundles newer than `cutoffDate` (null = no cutoff), mapping each to its
 * metadata row. Pages through R2 with custom metadata included; the zip bodies stay in the bucket.
 */
async function listProdErrorReports(bucket: R2Bucket, cutoffDate: string | null): Promise<ErrorReportRow[]> {
  const rows: ErrorReportRow[] = []
  let cursor: string | undefined
  do {
    const list = await bucket.list({ prefix: errorReportProdPrefix, cursor, include: ['customMetadata'] })
    for (const obj of list.objects) {
      // An amended report also carries a `.amend.json` sidecar under this prefix. It is part of the
      // report, not a second one, so counting it would double the report in every aggregation.
      if (isAmendSidecarKey(obj.key)) continue
      const date = extractDateSegment(obj.key)
      // The date segment sorts lexically (yyyy-mm-dd), so a string compare is a valid window filter.
      if (cutoffDate && date && date < cutoffDate) continue
      rows.push(toErrorReportRow(obj, date))
    }
    cursor = list.truncated ? list.cursor : undefined
  } while (cursor)

  rows.sort((a, b) => b.generatedAt.localeCompare(a.generatedAt))
  return rows
}

// Admin stats: returns activation count and device count
// Auth: dedicated ADMIN_API_TOKEN, separate from the Paddle secrets used by /admin/generate
admin.get('/admin/stats', async (c) => {
  const token = c.env.ADMIN_API_TOKEN
  if (!token) {
    return c.json({ error: 'Admin API not configured' }, 500)
  }

  const authHeader = c.req.header('Authorization')
  if (!authHeader || !constantTimeEqual(authHeader, `Bearer ${token}`)) {
    return c.json({ error: 'Unauthorized' }, 401)
  }

  const raw = await c.env.LICENSE_CODES.get(activationCountKey)
  const totalActivations = parseInt(raw ?? '0', 10)

  // TODO: `activeDevices` requires querying the CF Analytics Engine SQL API
  // (`POST /v4/accounts/{id}/analytics_engine/sql`), which is an external HTTP call,
  // not available via the `DEVICE_COUNTS` binding (bindings only support `writeDataPoint`).
  // For v1, return null. The analytics dashboard queries CF Analytics Engine directly.
  const activeDevices: number | null = null

  return c.json({ totalActivations, activeDevices })
})

// Admin downloads: aggregated download data from D1
admin.get('/admin/downloads', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '7d'
  if (!validDownloadRanges.has(range)) {
    return c.json({ error: 'Invalid range. Use 24h, 7d, 30d, or all' }, 400)
  }

  const interval = rangeToSqliteInterval[range]
  const whereClause = interval ? `WHERE created_at >= datetime('now', '${interval}')` : ''

  // `count` is the raw request count; `uniqueCount` deduplicates same-day downloaders via the
  // peppered `hashed_ip` (rows written before migration 0008 have NULL, which COUNT DISTINCT
  // skips). `source` is NULL for those old rows too, surfaced as 'other'.
  //
  // The retention sweep clears `hashed_ip` past 90 days, so the live count alone would read as 0 for
  // older days. `downloads_daily_unique` holds the count captured before that clear, and the LEFT
  // JOIN prefers it: rollup for swept days, live count for days still inside the window. Same
  // retention-proof union as `/admin/update-activity`, one grouping so a distinct count is never
  // summed across groups.
  const { results } = await c.env.TELEMETRY_DB.prepare(
    `SELECT live.date, live.version, live.arch, live.country, live.source, live.count,
                COALESCE(rollup.unique_downloaders, live.uniqueCount) AS uniqueCount
         FROM (
             SELECT date(created_at) AS date, app_version AS version, arch, country,
                    COALESCE(source, 'other') AS source,
                    COUNT(*) AS count, COUNT(DISTINCT hashed_ip) AS uniqueCount
             FROM downloads ${whereClause}
             GROUP BY date, version, arch, country, source
         ) AS live
         LEFT JOIN downloads_daily_unique AS rollup
             ON rollup.date = live.date AND rollup.app_version = live.version
             AND rollup.arch = live.arch AND rollup.country = live.country AND rollup.source = live.source
         ORDER BY live.date ASC`,
  ).all<{
    date: string
    version: string
    arch: string
    country: string
    source: string
    count: number
    uniqueCount: number
  }>()

  return c.json(results)
})

// Admin update activity: distinct update-enabled installs that checked for updates per day, stacked by
// the version each was running when it checked (visualizes the fleet rolling onto a new release).
//
// The raw `update_checks` table is pruned to the last 7 days by the cron, while `daily_active_users`
// keeps the per-day aggregate forever but excludes today (the cron aggregates yesterday at 00:00 UTC).
// So we union the two with no overlap: the retained aggregate for past days, plus a live
// `COUNT(DISTINCT hashed_ip)` from the raw table for today. That makes the series both retention-proof
// (30d range stays complete) and fresh (today shows without waiting for the cron).
admin.get('/admin/update-activity', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '7d'
  if (!validDownloadRanges.has(range)) {
    return c.json({ error: 'Invalid range. Use 24h, 7d, 30d, or all' }, 400)
  }

  const interval = rangeToSqliteInterval[range]
  // Past days from the retained aggregate (everything before today, optionally floored by the range).
  const aggLowerBound = interval ? `date >= date('now', '${interval}') AND ` : ''

  const { results } = await c.env.TELEMETRY_DB.prepare(
    `SELECT date, version, SUM(cnt) AS count FROM (
           SELECT date, app_version AS version, unique_users AS cnt
               FROM daily_active_users
               WHERE ${aggLowerBound}date < date('now')
           UNION ALL
           SELECT date, app_version AS version, COUNT(DISTINCT hashed_ip) AS cnt
               FROM update_checks
               WHERE date = date('now')
               GROUP BY date, app_version
         )
         GROUP BY date, version
         ORDER BY date ASC`,
  ).all<{ date: string; version: string; count: number }>()

  return c.json(results)
})

// Admin active users: aggregated daily active user data from D1
admin.get('/admin/active-users', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '7d'
  if (!validActiveUserRanges.has(range)) {
    return c.json({ error: 'Invalid range. Use 7d, 30d, 90d, or all' }, 400)
  }

  const interval = rangeToSqliteInterval[range]
  const whereClause = interval ? `WHERE date >= date('now', '${interval}')` : ''

  const { results } = await c.env.TELEMETRY_DB.prepare(
    `SELECT date, app_version AS version, arch, unique_users AS uniqueUsers
         FROM daily_active_users ${whereClause}
         ORDER BY date ASC`,
  ).all<{ date: string; version: string; arch: string; uniqueUsers: number }>()

  return c.json(results)
})

// Admin crashes: aggregated crash data from D1
admin.get('/admin/crashes', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '7d'
  if (!validCrashRanges.has(range)) {
    return c.json({ error: 'Invalid range. Use 7d, 30d, 90d, or all' }, 400)
  }

  const interval = rangeToSqliteInterval[range]
  const whereClause = interval ? `WHERE created_at >= datetime('now', '${interval}')` : ''

  const { results } = await c.env.TELEMETRY_DB.prepare(
    `SELECT date(created_at) AS date, top_function AS topFunction, signal,
                COUNT(*) AS count, GROUP_CONCAT(DISTINCT app_version) AS versions
         FROM crash_reports ${whereClause}
         GROUP BY date, topFunction, signal
         ORDER BY date ASC`,
  ).all<{ date: string; topFunction: string; signal: string; count: number; versions: string }>()

  return c.json(results)
})

// Admin heartbeat DAU: true daily-active counts from the raw heartbeat table.
// dau = distinct analytics ids per day, beats = total heartbeats per day (engagement signal).
admin.get('/admin/heartbeat-dau', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '7d'
  if (!validHeartbeatRanges.has(range)) {
    return c.json({ error: 'Invalid range. Use 7d, 30d, 90d, or all' }, 400)
  }

  const interval = rangeToSqliteInterval[range]
  const whereClause = interval ? `WHERE created_at >= datetime('now', '${interval}')` : ''

  const { results } = await c.env.TELEMETRY_DB.prepare(
    `SELECT date(created_at) AS date, COUNT(DISTINCT anal_id) AS dau, COUNT(*) AS beats
         FROM heartbeat ${whereClause}
         GROUP BY date
         ORDER BY date ASC`,
  ).all<{ date: string; dau: number; beats: number }>()

  return c.json(results)
})

// Admin config shape: how many installs report each value of each setting, split by app version.
//
// Only each install's LATEST heartbeat counts, so an install that beats hourly weighs the same as one
// that ran for an afternoon, and a setting someone changed mid-window reads at its current value.
//
// The app version has to ride along and can't be summed away here: the config shape is sparse (the
// app persists only settings a user explicitly changed), so an absent key means "on the default" OR
// "this setting didn't exist in that build", and only the per-version defaults manifest on the
// dashboard can tell those apart. `value` comes back beside its SQLite `type` because a JSON `true`
// arrives as `1` over D1; the dashboard rebuilds the real type from the pair.
admin.get('/admin/config-shape', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '30d'
  if (!validHeartbeatRanges.has(range)) {
    return c.json({ error: 'Invalid range. Use 7d, 30d, 90d, or all' }, 400)
  }

  const interval = rangeToSqliteInterval[range]
  const window = interval ? `AND created_at >= datetime('now', '${interval}')` : ''
  // `config_json` is nullable (releases before the config shape shipped sent none), and `json_each`
  // over a NULL is not something to find out about in production, so those rows never enter the CTE.
  // They'd be useless in a denominator anyway: an install with no config reports no settings.
  const latest = `WITH latest AS (
           SELECT anal_id, app_version, config_json,
                  ROW_NUMBER() OVER (PARTITION BY anal_id ORDER BY created_at DESC, id DESC) AS rn
               FROM heartbeat
               WHERE config_json IS NOT NULL ${window}
         )`

  const [installs, values] = await Promise.all([
    c.env.TELEMETRY_DB.prepare(
      `${latest}
         SELECT app_version AS appVersion, COUNT(*) AS installs
             FROM latest WHERE rn = 1
             GROUP BY appVersion
             ORDER BY appVersion`,
    ).all<{ appVersion: string; installs: number }>(),
    c.env.TELEMETRY_DB.prepare(
      `${latest}
         SELECT l.app_version AS appVersion, je.key AS key, je.type AS type,
                je.value AS value, COUNT(*) AS installs
             FROM latest l, json_each(l.config_json) je
             WHERE l.rn = 1
             GROUP BY appVersion, key, type, value
             ORDER BY appVersion, key`,
    ).all<{ appVersion: string; key: string; type: string; value: string | number; installs: number }>(),
  ])

  return c.json({ installs: installs.results, values: values.results })
})

// Admin feedback: in-app "Send feedback" messages from D1, newest first. The dashboard is private
// (Cloudflare Access), so the full message text and any reply-to email are returned for triage.
// No install id is stored alongside feedback, so there's nothing to join it to.
admin.get('/admin/feedback', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '7d'
  if (!validFeedbackRanges.has(range)) {
    return c.json({ error: 'Invalid range. Use 7d, 30d, 90d, or all' }, 400)
  }

  const interval = rangeToSqliteInterval[range]
  const whereClause = interval ? `WHERE created_at >= datetime('now', '${interval}')` : ''

  const { results } = await c.env.TELEMETRY_DB.prepare(
    `SELECT id, created_at AS createdAt, feedback, email,
                app_version AS appVersion, os_version AS osVersion, build_mode AS buildMode
         FROM feedback ${whereClause}
         ORDER BY created_at DESC
         LIMIT 1000`,
  ).all<{
    id: number
    createdAt: string
    feedback: string
    email: string | null
    appVersion: string
    osVersion: string
    buildMode: string | null
  }>()

  return c.json(results)
})

// Admin error reports: per-bundle metadata for real-user error reports, newest first. Lists the R2
// prod prefix (with custom metadata) rather than indexing into D1: the bundles live in R2 with a
// 90-day lifecycle, and `bucket.list` returns the id/kind/version metadata we need for aggregation
// without downloading any zip. Deep dives into a bundle's log tail stay in the local digest command.
admin.get('/admin/error-reports', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '7d'
  if (!validErrorReportRanges.has(range)) {
    return c.json({ error: 'Invalid range. Use 7d, 30d, 90d, or all' }, 400)
  }

  const days = rangeToDays[range]
  const cutoffDate = days ? new Date(Date.now() - days * 86_400_000).toISOString().slice(0, 10) : null

  const rows = await listProdErrorReports(c.env.ERROR_REPORTS_BUCKET, cutoffDate)
  return c.json(rows)
})

export { admin }
