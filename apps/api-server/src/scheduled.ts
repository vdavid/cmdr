import { sendCrashNotificationEmail, sendDbSizeAlert, type CrashEmailRow, type CrashFate } from './email'
import type { Bindings } from './types'
import {
  recomputeTotal,
  tryEvict,
  EVICTION_HIGH_WATERMARK,
  EVICTION_LOW_WATERMARK,
} from './telemetry/error-report-eviction'
import { isIntakePaused, resumeIntake } from './telemetry/error-report-intake'
import { postEvictionBlockedNotification, postEvictionNotification } from './discord'

const dbSizeThresholdBytes = 100 * 1024 * 1024 // 100 MB

/** Map the DB `build_mode` column to the friendly `prod`/`dev` label users see. */
function buildModeToEnv(buildMode: string | null | undefined): 'prod' | 'dev' | '?' {
  if (buildMode === 'release') return 'prod'
  if (buildMode === 'debug') return 'dev'
  return '?'
}

/**
 * Map the DB `app_fate` column to the label the email shows. `keptRunning` is the one value that
 * says the app survived; everything else either says it went down or says nothing, and a row that
 * says nothing must not be rendered as a crash. `unconfirmed` shouldn't reach the DB at all (the
 * client resolves it at the next launch), and it claims nothing either way, so it reads as `'?'`.
 */
function appFateToLabel(appFate: string | null | undefined): CrashFate {
  if (appFate === 'ended') return 'crashed'
  if (appFate === 'keptRunning') return 'kept running'
  return '?'
}

async function handleCrashNotifications(env: Bindings): Promise<void> {
  if (!env.CRASH_NOTIFICATION_EMAIL || !env.RESEND_API_KEY) return

  // One row per crash, newest first. No grouping: the email shows every report.
  const { results } = await env.TELEMETRY_DB.prepare(
    `SELECT id, app_version, os_version, arch, signal, top_function, created_at, build_mode, short_id, email, panic_message, app_fate
         FROM crash_reports
         WHERE notified_at IS NULL
         ORDER BY created_at DESC`,
  ).all<{
    id: number
    app_version: string
    os_version: string
    arch: string
    signal: string
    top_function: string
    created_at: string
    build_mode: string | null
    short_id: string | null
    email: string | null
    panic_message: string | null
    app_fate: string | null
  }>()

  if (results.length === 0) return

  const crashes: CrashEmailRow[] = results.map((row) => ({
    when: row.created_at,
    env: buildModeToEnv(row.build_mode),
    fate: appFateToLabel(row.app_fate),
    id: row.short_id ?? '?',
    site: row.top_function,
    signal: row.signal,
    version: row.app_version,
    email: row.email,
    message: row.panic_message,
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

/**
 * How long a `downloads` row keeps the two columns that could identify the person behind it: the
 * peppered `hashed_ip` (same-day dedup) and the raw `user_agent` (lets us re-tune `classifyUaFamily`
 * against real strings). A quarter is long enough for both jobs and short enough to be a real limit.
 */
const downloadIdentifierRetentionDays = 90

/**
 * How long a crash report keeps the two fields tied to a person: the `email` a beta tester attached
 * for a reply, and the `diag_id` grouping their sequential reports. The technical row (version,
 * signal, `top_function`, redacted backtrace, redacted panic message) is kept indefinitely; it's what
 * long-standing stability work runs on and it names nobody.
 */
const crashIdentifierRetentionDays = 90

/** How long an in-app feedback row keeps its optional reply-to address. The message text stays. */
const feedbackEmailRetentionDays = 730

/**
 * How long raw heartbeat rows live. This is the one table keyed by a stable per-install id
 * (`anal_id`), so it's the one that needs an actual delete rather than a column clear: the id IS the
 * data. Two years covers every window the dashboard computes (DAU, new installs, D7 retention) with
 * room to spare.
 */
const heartbeatRetentionDays = 730

/**
 * The `created_at` cutoff `days` back, snapped to MIDNIGHT UTC so a day is always swept whole.
 * That matters for `downloads`: the rollup captures a day's distinct-downloader count and the clear
 * then erases the hashes it came from, so a cutoff mid-day would roll up half a day, clear that half,
 * and leave `/admin/downloads` preferring the partial number over the live one for that date.
 *
 * Returned as `YYYY-MM-DD 00:00:00`, which compares correctly against both `created_at` formats in
 * these tables (`datetime('now')` writes a space, `strftime(...)` writes `T`/`Z`): the differing
 * separator only ever decides a comparison WITHIN one second of the boundary, and midnight belongs to
 * the day we're keeping either way. Plain `<` on the column also keeps the `created_at` indexes usable,
 * which wrapping it in `date()` would not.
 */
function cutoff(days: number): string {
  const boundary = new Date(Date.now() - days * 86_400_000)
  return `${boundary.toISOString().slice(0, 10)} 00:00:00`
}

/**
 * Daily data-retention sweep: enforces the retention promises in the privacy policy
 * (`apps/website/src/pages/privacy-policy.astro` § "How long we keep your data") in code, so they
 * can't drift back into "kept forever" by default.
 *
 * The shape is deliberate: for `downloads`, `crash_reports`, and `feedback` we clear the identifying
 * COLUMNS and keep the row, because the counts and the engineering value live in the other columns
 * and there's no reason to lose them. Only `heartbeat` gets rows deleted, because its identity IS
 * the row.
 *
 * Every statement is idempotent (each `WHERE` excludes what it already cleared) and bounded by
 * `created_at`, so re-running the sweep, or running it after an outage, is free and safe.
 */
async function handleRetentionSweep(env: Bindings): Promise<void> {
  const db = env.TELEMETRY_DB

  // Capture the per-day distinct-downloader counts BEFORE clearing the hashes they're derived from.
  // `/admin/downloads` reads this rollup for any day whose hashes are gone (same union pattern as
  // `daily_active_users`). Doing this in the other order would silently zero every historical unique
  // count, and there'd be no way back.
  await db
    .prepare(
      `INSERT OR IGNORE INTO downloads_daily_unique (date, app_version, arch, country, source, unique_downloaders)
           SELECT date(created_at), app_version, arch, country, COALESCE(source, 'other'), COUNT(DISTINCT hashed_ip)
           FROM downloads
           WHERE created_at < ?1 AND hashed_ip IS NOT NULL
           GROUP BY date(created_at), app_version, arch, country, COALESCE(source, 'other')`,
    )
    .bind(cutoff(downloadIdentifierRetentionDays))
    .run()

  await db
    .prepare(
      `UPDATE downloads SET hashed_ip = NULL, user_agent = NULL
           WHERE created_at < ?1 AND (hashed_ip IS NOT NULL OR user_agent IS NOT NULL)`,
    )
    .bind(cutoff(downloadIdentifierRetentionDays))
    .run()

  await db
    .prepare(
      `UPDATE crash_reports SET email = NULL, diag_id = NULL
           WHERE created_at < ?1 AND (email IS NOT NULL OR diag_id IS NOT NULL)`,
    )
    .bind(cutoff(crashIdentifierRetentionDays))
    .run()

  await db
    .prepare(`UPDATE feedback SET email = NULL WHERE created_at < ?1 AND email IS NOT NULL`)
    .bind(cutoff(feedbackEmailRetentionDays))
    .run()

  await db.prepare(`DELETE FROM heartbeat WHERE created_at < ?1`).bind(cutoff(heartbeatRetentionDays)).run()
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

/**
 * Daily sweep that corrects `total_bytes` KV drift (KV increments are racy: see
 * `incrementTotalBytes`), lifts an intake pause once there's room again, and evicts oldest
 * bundles if still above the high watermark. Idempotent: safe to run multiple times.
 */
async function handleDailyEvictionSweep(env: Bindings): Promise<void> {
  const recomputed = await recomputeTotal(env)

  // Resume at the LOW watermark, not the high one: resuming the moment the bucket dips under the
  // high watermark would reopen intake straight into the level that paused it.
  if (recomputed <= EVICTION_LOW_WATERMARK && (await isIntakePaused(env.ERROR_REPORT_META))) {
    await resumeIntake(env.ERROR_REPORT_META)
    console.log(`Error report intake resumed: bucket back to ${recomputed.toString()} bytes`)
  }

  if (recomputed <= EVICTION_HIGH_WATERMARK) return

  const result = await tryEvict(env)
  if (!env.DISCORD_WEBHOOK_URL) return

  if (result.outcome === 'evicted' && result.evictedCount > 0) {
    await postEvictionNotification(env.DISCORD_WEBHOOK_URL, {
      evictedCount: result.evictedCount,
      freedBytes: result.freedBytes,
      newTotalBytes: result.newTotal,
    })
  }
  if (result.outcome === 'paused') {
    await postEvictionBlockedNotification(env.DISCORD_WEBHOOK_URL, result)
  }
}

export {
  handleCrashNotifications,
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  downloadIdentifierRetentionDays,
  crashIdentifierRetentionDays,
  feedbackEmailRetentionDays,
  heartbeatRetentionDays,
}
