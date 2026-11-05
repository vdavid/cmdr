import { checkEmailPath } from './email/send'
import { sendDbSizeAlert } from './email/ops-alerts'
import type { Bindings } from './types'
import {
  recomputeTotal,
  tryEvict,
  recordEvictionOutcome,
  EVICTION_HIGH_WATERMARK,
  EVICTION_LOW_WATERMARK,
} from './telemetry/error-report-eviction'
import { isIntakePaused, resumeIntake } from './telemetry/error-report-intake'
import { backupLicenseLedger } from './licensing/license-backup'
import { deleteComment, listExpiredPersonalComments, resolveIssueTarget } from './github-issues'

const MB = 1024 * 1024

/**
 * D1's per-database cap on the plan this account is on: Workers Free, 500 MB (verified on the
 * D1 limits page and the account's $0.00 monthly invoices, 2026-10-23). Workers Paid raises it to
 * 10 GB; switch this when the plan changes, or the alerts land at the wrong sizes.
 */
const d1MaxDbBytes = 500 * MB

/** Shares of the cap that each alert once, on the day the size first passes them. */
const dbSizeAlertSteps = [0.4, 0.6, 0.8, 0.9]

const dbSizeSampleRetentionDays = 400

/** How far back the growth rate looks, in days. */
const dbGrowthWindowDays = 30

/**
 * Check the SMTP2Go key and quota once a day, so a dead `SMTP2GO_API_KEY` or a spent quota surfaces
 * on its own schedule rather than at the moment something important needs to send. It sends no
 * email (`checkEmailPath` says why). A failure throws, and `runCronJob` raises the alarm.
 */
async function handleEmailPathProbe(env: Bindings): Promise<void> {
  if (!env.SMTP2GO_API_KEY) return
  await checkEmailPath(env.SMTP2GO_API_KEY)
}

/**
 * Snapshot every license into R2, once a day. The ledger is the only record of who holds a license
 * and why, and the signed keys beside it live in KV, which has no point-in-time recovery at all.
 *
 * Append-only on purpose: nothing prunes these, and nothing should until there are enough of them
 * to be worth a rule. Each day's object is a few kilobytes. `licensing/license-backup.ts`.
 */
async function handleLicenseBackup(env: Bindings): Promise<void> {
  const backup = await backupLicenseLedger(env, new Date())
  console.log('License backup written:', backup.licenses.length, 'licenses,', Object.keys(backup.keys).length, 'keys')
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
 * How long an error report's outbox row (and every amendment to it) lives: the R2 bundle's own 90
 * days, because the row carries the same note and reply-to the bundle does.
 */
const errorReportRetentionDays = 90

/**
 * How long raw heartbeat rows live. This is the one table keyed by a stable per-install id
 * (`anal_id`), so it's the one that needs an actual delete rather than a column clear: the id IS the
 * data. Two years covers every window the dashboard computes (DAU, new installs, D7 retention) with
 * room to spare.
 */
const heartbeatRetentionDays = 730

/**
 * How long an install may go silent without ever having persisted a single setting before its
 * beats are read as a tooling instance rather than a person.
 *
 * The signal: the frontend settings store stamps `_schemaVersion` into `settings.json` on every
 * save, and the heartbeat's config snapshot carries every number-valued key, so one beat carrying
 * `_schemaVersion` proves a person changed something. Measured over the beta, 293 of 303 real
 * installs stamped it within their first HOUR and only one took longer than 48 hours, while 1,786
 * ids never stamped it at all.
 *
 * The grace period is what protects the case that makes this delicate: a genuinely brand-new user
 * has no `settings.json` either, so for their first minutes they are indistinguishable from a test
 * shard. A week is far past the observed spread and still converges.
 */
const syntheticHeartbeatGraceDays = 7

/**
 * The installs that have NEVER persisted a setting and have been silent since `?1`: the synthetic
 * ones. Both deletes below share it, and both are exported so `synthetic-heartbeats.test.ts` can run
 * them against a real SQLite.
 *
 * Decided per INSTALL, not per row: one `_schemaVersion` beat vouches for that id's whole history,
 * so a real user's launch beats from before their first settings save survive.
 *
 * `instr` rather than `LIKE`: in SQLite `_` is a single-character wildcard, so
 * `LIKE '%"_schemaVersion"%'` would let a config with an unrelated `"xschemaVersion"` key vouch
 * for a synthetic install.
 */
const syntheticInstallIdsSql = `SELECT anal_id FROM heartbeat
       GROUP BY anal_id
       HAVING MAX(CASE WHEN instr(COALESCE(config_json, ''), '"_schemaVersion"') > 0 THEN 1 ELSE 0 END) = 0
          AND MAX(created_at) < ?1`

/** Every beat of a synthetic install. */
const deleteSyntheticHeartbeatsSql = `DELETE FROM heartbeat WHERE anal_id IN (${syntheticInstallIdsSql})`

/**
 * The feature events of the same installs. ❌ Must run BEFORE `deleteSyntheticHeartbeatsSql`: the
 * classifier reads `heartbeat`, so once the beats are gone nothing marks these events as synthetic,
 * and they'd stay until retention. Run in that order, a failure between the two is harmless: the
 * next day's run finds the same beats and deletes them.
 */
const deleteSyntheticEventsSql = `DELETE FROM analytics_event WHERE anal_id IN (${syntheticInstallIdsSql})`

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
 * and there's no reason to lose them. Only `heartbeat` and `analytics_event` get rows deleted,
 * because their identity IS the row.
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

  // The error-report outbox rows carry the note and the reply-to, so they go whole with the bundle's
  // 90 days. An amendment goes with the report it amends (`report_date`), never 90 days from itself.
  await db.prepare(`DELETE FROM error_reports WHERE created_at < ?1`).bind(cutoff(errorReportRetentionDays)).run()
  // `report_date` is a bare `YYYY-MM-DD`, which sorts BEFORE `YYYY-MM-DD 00:00:00`, so it's widened to
  // the cutoff's shape first; compared bare, the cutoff day's amendments would go a day early.
  await db
    .prepare(`DELETE FROM error_report_amendments WHERE report_date || ' 00:00:00' < ?1`)
    .bind(cutoff(errorReportRetentionDays))
    .run()

  await db.prepare(`DELETE FROM heartbeat WHERE created_at < ?1`).bind(cutoff(heartbeatRetentionDays)).run()

  // Relayed feature events are the same usage stats under the same promise. Aged by `received_at`,
  // OUR clock: `occurred_at` is the client's, and a wrong one could keep a row forever.
  await db.prepare(`DELETE FROM analytics_event WHERE received_at < ?1`).bind(cutoff(heartbeatRetentionDays)).run()
}

/**
 * The half of the retention promise that lives outside D1: the personal-data comments on the
 * triage issues in the private reports repo (`github-issues.ts`).
 *
 * Each such comment carries its own `expires=YYYY-MM-DD` stamp, so this job needs no knowledge of
 * which promise produced it: it deletes whatever is due today. That's why a report's note (90 days)
 * and a feedback reply-to address (two years) can share one mechanism.
 *
 * Deleting the COMMENT rather than editing the issue body is the point. An edit leaves the original
 * text in GitHub's revision history, so it would not be a deletion at all.
 *
 * A no-op when the integration is unconfigured. Failures throw, so `runCronJob` turns them into the
 * alarm: a sweep that quietly stopped running would leave personal data past its promise,
 * which is exactly the failure that has to be loud.
 */
async function handlePersonalCommentSweep(env: Bindings): Promise<void> {
  const target = resolveIssueTarget(env)
  if (!target) return

  const today = new Date().toISOString().slice(0, 10)
  const due = await listExpiredPersonalComments(target, today)

  for (const comment of due) {
    await deleteComment(target, comment.commentId)
    console.log(
      `Personal comment sweep: deleted comment ${String(comment.commentId)} on issue #${String(comment.issueNumber)} (due ${comment.expiresOn})`,
    )
  }
}

/**
 * Daily integrity sweep: removes the beats of installs that were never a person.
 *
 * A fresh data dir mints a fresh `anal_` id, so any instance the app's own tooling launches
 * registers as a brand-new user on every launch. That went unnoticed through the beta and left the
 * table 6x over-counted (2,089 ids against 303 real ones), which is what
 * `analytics/DETAILS.md` § "Why an isolated instance must never send" now guards against at the
 * source.
 *
 * This is the second half of that fix, and it stays because it also ANSWERS a question: with the
 * app-side gate in place this should delete nothing, so anything it does delete means a new tooling
 * path started leaking. Cleaning at write time isn't available: the Worker can't tell a robot from
 * a person at intake, only over an install's history.
 *
 * Idempotent and bounded by the same predicate every day, so re-running it after an outage is free.
 */
async function handleSyntheticHeartbeatSweep(env: Bindings): Promise<void> {
  const graceCutoff = cutoff(syntheticHeartbeatGraceDays)
  await env.TELEMETRY_DB.prepare(deleteSyntheticEventsSql).bind(graceCutoff).run()
  const result = await env.TELEMETRY_DB.prepare(deleteSyntheticHeartbeatsSql).bind(graceCutoff).run()

  if (result.meta.changes > 0) {
    const deleted = result.meta.changes.toString()
    console.log(`Synthetic heartbeat sweep: deleted ${deleted} beats from installs that never saved a setting`)
  }
}

interface SizeSample {
  day: string
  size_bytes: number
}

function utcDay(date: Date): string {
  return date.toISOString().slice(0, 10)
}

function daysBetween(fromDay: string, toDay: string): number {
  return Math.round((Date.parse(toDay) - Date.parse(fromDay)) / (24 * 60 * 60 * 1000))
}

/** The step of the cap that `sizeBytes` passed since `previousBytes`, the highest if several. */
function crossedStep(previousBytes: number, sizeBytes: number): number | null {
  const crossed = dbSizeAlertSteps.filter(
    (step) => previousBytes < step * d1MaxDbBytes && sizeBytes >= step * d1MaxDbBytes,
  )
  return crossed.at(-1) ?? null
}

/** Where the next alert lands after `step`, or null past the last one. */
function nextStepMb(step: number): number | null {
  const next = dbSizeAlertSteps.find((candidate) => candidate > step)
  return next === undefined ? null : (next * d1MaxDbBytes) / MB
}

/**
 * MB per month from the newest sample at least `dbGrowthWindowDays` old, or the oldest one when the
 * history is shorter. Null with under a week of history or no growth: too thin to project from.
 */
async function growthRate(db: D1Database, today: string, sizeBytes: number) {
  const windowStart = utcDay(new Date(Date.parse(today) - dbGrowthWindowDays * 24 * 60 * 60 * 1000))
  const base =
    (await db
      .prepare(`SELECT day, size_bytes FROM db_size_samples WHERE day <= ? ORDER BY day DESC LIMIT 1`)
      .bind(windowStart)
      .first<SizeSample>()) ??
    (await db
      .prepare(`SELECT day, size_bytes FROM db_size_samples WHERE day < ? ORDER BY day ASC LIMIT 1`)
      .bind(today)
      .first<SizeSample>())
  if (!base) return null
  const days = daysBetween(base.day, today)
  if (days < 7 || sizeBytes <= base.size_bytes) return null
  return { mbPerMonth: ((sizeBytes - base.size_bytes) / MB / days) * 30, overDays: days }
}

/**
 * Daily tripwire on the telemetry D1: records the day's size, and alerts once per step of the cap
 * crossed (`dbSizeAlertSteps`) with the growth rate and the per-table row counts that say where the
 * size went. The steps come from comparing today with the latest earlier sample, so a size that
 * sits past a line stays quiet; a missed day only widens the comparison.
 *
 * The size comes from `meta.size_after`, which D1 stamps on the result of EVERY statement, so a
 * throwaway `SELECT 1` reads it for free.
 *
 * ❌ Never reach for SQLite's `pragma_page_count` / `pragma_page_size` table-valued functions: they
 * look like the obvious source, and D1 rejects them with `SQLITE_AUTH` at runtime. That's invisible
 * to every test here, because a mocked D1 answers whatever it's told to.
 */
async function handleDbSizeCheck(env: Bindings, now: Date = new Date()): Promise<void> {
  if (!env.CRASH_NOTIFICATION_EMAIL || !env.SMTP2GO_API_KEY) return

  const db = env.TELEMETRY_DB
  const today = utcDay(now)
  const probe = await db.prepare(`SELECT 1`).run()
  const sizeBytes = probe.meta.size_after

  const previous = await db
    .prepare(`SELECT day, size_bytes FROM db_size_samples WHERE day < ? ORDER BY day DESC LIMIT 1`)
    .bind(today)
    .first<SizeSample>()
  await db
    .prepare(`INSERT OR REPLACE INTO db_size_samples (day, size_bytes) VALUES (?, ?)`)
    .bind(today, sizeBytes)
    .run()
  const pruneBefore = utcDay(new Date(now.getTime() - dbSizeSampleRetentionDays * 24 * 60 * 60 * 1000))
  await db.prepare(`DELETE FROM db_size_samples WHERE day < ?`).bind(pruneBefore).run()

  const step = crossedStep(previous?.size_bytes ?? 0, sizeBytes)
  if (step === null) return

  const growth = await growthRate(db, today, sizeBytes)

  // Only on an alert, so the daily quiet path skips one count per table.
  const tables = ['crash_reports', 'downloads', 'update_checks', 'daily_active_users', 'heartbeat', 'analytics_event']
  const tableCounts: Record<string, number> = {}
  for (const table of tables) {
    const row = await env.TELEMETRY_DB.prepare(`SELECT COUNT(*) AS cnt FROM ${table}`).first<{ cnt: number }>()
    tableCounts[table] = row?.cnt ?? 0
  }

  await sendDbSizeAlert({
    sizeMb: sizeBytes / MB,
    capMb: d1MaxDbBytes / MB,
    nextStepMb: nextStepMb(step),
    growth,
    tableCounts,
    to: env.CRASH_NOTIFICATION_EMAIL,
    smtp2goApiKey: env.SMTP2GO_API_KEY,
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

  await recordEvictionOutcome(env.TELEMETRY_DB, await tryEvict(env))
}

export {
  handleEmailPathProbe,
  handleLicenseBackup,
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  handlePersonalCommentSweep,
  handleSyntheticHeartbeatSweep,
  deleteSyntheticHeartbeatsSql,
  deleteSyntheticEventsSql,
  syntheticHeartbeatGraceDays,
  downloadIdentifierRetentionDays,
  crashIdentifierRetentionDays,
  feedbackEmailRetentionDays,
  heartbeatRetentionDays,
}
