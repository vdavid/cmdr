import { Hono } from 'hono'
import { constantTimeEqual } from './paddle'
import { type Bindings, activationCountKey, verifyAdminAuth } from './types'

const admin = new Hono<{ Bindings: Bindings }>()

const validDownloadRanges = new Set(['24h', '7d', '30d', 'all'])
const validActiveUserRanges = new Set(['7d', '30d', '90d', 'all'])
const validCrashRanges = new Set(['7d', '30d', '90d', 'all'])

// Values are hardcoded, never from user input — safe to interpolate into SQL.
const rangeToSqliteInterval: Record<string, string> = {
  '24h': '-1 day',
  '7d': '-7 days',
  '30d': '-30 days',
  '90d': '-90 days',
}

// Admin stats — returns activation count and device count
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

// Admin downloads — aggregated download data from D1
admin.get('/admin/downloads', async (c) => {
  const authError = verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '7d'
  if (!validDownloadRanges.has(range)) {
    return c.json({ error: 'Invalid range. Use 24h, 7d, 30d, or all' }, 400)
  }

  const interval = rangeToSqliteInterval[range]
  const whereClause = interval ? `WHERE created_at >= datetime('now', '${interval}')` : ''

  const { results } = await c.env.TELEMETRY_DB.prepare(
    `SELECT date(created_at) AS date, app_version AS version, arch, country, COUNT(*) AS count
         FROM downloads ${whereClause}
         GROUP BY date, version, arch, country
         ORDER BY date ASC`,
  ).all<{ date: string; version: string; arch: string; country: string; count: number }>()

  return c.json(results)
})

// Admin active users — aggregated daily active user data from D1
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

// Admin crashes — aggregated crash data from D1
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

export { admin }
