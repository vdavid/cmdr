import { sendCrashNotificationEmail, sendDbSizeAlert, type CrashSummaryEntry } from './email'
import type { Bindings } from './types'

const dbSizeThresholdBytes = 100 * 1024 * 1024 // 100 MB

async function handleCrashNotifications(env: Bindings): Promise<void> {
  if (!env.CRASH_NOTIFICATION_EMAIL || !env.RESEND_API_KEY) return

  const { results } = await env.TELEMETRY_DB.prepare(
    `SELECT id, app_version, os_version, arch, signal, top_function, created_at
         FROM crash_reports WHERE notified_at IS NULL`,
  ).all<{
    id: number
    app_version: string
    os_version: string
    arch: string
    signal: string
    top_function: string
    created_at: string
  }>()

  if (results.length === 0) return

  // Group by top_function
  const grouped = new Map<string, { count: number; versions: Set<string>; mostRecent: string }>()
  for (const row of results) {
    const existing = grouped.get(row.top_function)
    if (existing) {
      existing.count++
      existing.versions.add(row.app_version)
      if (row.created_at > existing.mostRecent) existing.mostRecent = row.created_at
    } else {
      grouped.set(row.top_function, {
        count: 1,
        versions: new Set([row.app_version]),
        mostRecent: row.created_at,
      })
    }
  }

  const crashes: CrashSummaryEntry[] = [...grouped.entries()].map(([topFunction, data]) => ({
    topFunction,
    count: data.count,
    versions: [...data.versions],
    mostRecent: data.mostRecent,
  }))

  const ids = results.map((r) => r.id)
  const now = new Date().toISOString()

  // Mark as notified BEFORE sending email (prefer missed notification over duplicate)
  const placeholders = ids.map(() => '?').join(', ')
  await env.TELEMETRY_DB.prepare(`UPDATE crash_reports SET notified_at = ? WHERE id IN (${placeholders})`)
    .bind(now, ...ids)
    .run()

  await sendCrashNotificationEmail({
    crashes,
    totalCount: results.length,
    to: env.CRASH_NOTIFICATION_EMAIL,
    resendApiKey: env.RESEND_API_KEY,
  })
}

async function handleDailyAggregation(env: Bindings): Promise<void> {
  // Compute yesterday's date
  const yesterday = new Date(Date.now() - 86_400_000).toISOString().slice(0, 10)

  // Check if already aggregated
  const existing = await env.TELEMETRY_DB.prepare(`SELECT 1 FROM daily_active_users WHERE date = ? LIMIT 1`)
    .bind(yesterday)
    .first()

  if (existing) return

  // Aggregate raw update checks into daily_active_users
  await env.TELEMETRY_DB.prepare(
    `INSERT OR IGNORE INTO daily_active_users (date, app_version, arch, unique_users)
         SELECT date, app_version, arch, COUNT(*) AS unique_users
         FROM update_checks
         WHERE date = ?
         GROUP BY date, app_version, arch`,
  )
    .bind(yesterday)
    .run()

  // Prune raw update checks older than 7 days
  await env.TELEMETRY_DB.prepare(`DELETE FROM update_checks WHERE date < date('now', '-7 days')`).run()
}

async function handleDbSizeCheck(env: Bindings): Promise<void> {
  if (!env.CRASH_NOTIFICATION_EMAIL || !env.RESEND_API_KEY) return

  const sizeRow = await env.TELEMETRY_DB.prepare(
    `SELECT page_count * page_size AS total_size FROM pragma_page_count, pragma_page_size`,
  ).first<{ total_size: number }>()

  if (!sizeRow || sizeRow.total_size <= dbSizeThresholdBytes) return

  const sizeMb = sizeRow.total_size / (1024 * 1024)

  // Get row counts for each table
  const tables = ['crash_reports', 'downloads', 'update_checks', 'daily_active_users']
  const tableCounts: Record<string, number> = {}
  for (const table of tables) {
    const row = await env.TELEMETRY_DB.prepare(`SELECT COUNT(*) AS cnt FROM ${table}`).first<{ cnt: number }>()
    tableCounts[table] = row?.cnt ?? 0
  }

  await sendDbSizeAlert({
    sizeMb,
    tableCounts,
    to: env.CRASH_NOTIFICATION_EMAIL,
    resendApiKey: env.RESEND_API_KEY,
  })
}

export { handleCrashNotifications, handleDailyAggregation, handleDbSizeCheck }
