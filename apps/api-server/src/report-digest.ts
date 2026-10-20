/**
 * The three-hour digest job: gather every release-build crash, error report, amendment, and
 * feedback message nobody has been mailed about, send ONE email (`email/digest.ts`), then stamp
 * `notified_at` on exactly the rows it carried. Nothing new, no email.
 *
 * Runs after the outbox on the same tick, so the cards it links were filed seconds earlier.
 *
 * ❗ Stamps AFTER the send. `sendViaResend` throws on a rejected send, so a failure leaves every row
 * unstamped and the next tick retries it in a bigger digest. Stamping first would drop the alert
 * permanently, and a dead credential is exactly when that happens.
 */

import { sendReportDigestEmail, digestSize, type DigestContent } from './email/digest'
import type { CrashFate } from './email/crash'
import type { FeedbackEmailLogs } from './email/feedback'
import type { BoardCard } from './email/layout'
import { humanReportRecipient } from './email/send'
import { SIGNUP_KINDS, type OpsEventKind } from './ops-events'
import { parseOsFrames } from './report-outbox'
import { PRESIGN_TTL_HOURS, buildPresignedUrl } from './telemetry/error-report'
import type { Bindings } from './types'

/** A NULL `build_mode` is an old client, which means a release build. Debug builds are our own E2E traffic. */
const RELEASE_ONLY = `COALESCE(build_mode, 'release') != 'debug'`

/**
 * Map the DB `app_fate` column to the label the email shows. `keptRunning` is the one value that
 * says the app survived; everything else either says it went down or says nothing, and a row that
 * says nothing must not be rendered as a crash.
 */
function appFateToLabel(appFate: string | null | undefined): CrashFate {
  if (appFate === 'ended') return 'crashed'
  if (appFate === 'keptRunning') return 'kept running'
  return '?'
}

/** The card link for an `issue_number`, or null for a row the outbox hasn't filed (or never will). */
function cardFor(env: Bindings, issueNumber: number | null): BoardCard | null {
  if (!issueNumber || !env.GITHUB_ISSUES_REPO) return null
  return { number: issueNumber, url: `https://github.com/${env.GITHUB_ISSUES_REPO}/issues/${String(issueNumber)}` }
}

interface CrashRow {
  id: number
  created_at: string
  app_version: string
  signal: string
  top_function: string
  short_id: string | null
  email: string | null
  panic_message: string | null
  app_fate: string | null
  image_base: string | null
  os_exception: string | null
  os_frames: string | null
  issue_number: number | null
}

interface ErrorReportRow {
  id: number
  short_id: string
  kind: string
  app_version: string
  os_version: string
  arch: string
  size_bytes: number
  r2_key: string
  user_note: string | null
  email: string | null
  issue_number: number | null
}

interface AmendmentRow {
  id: number
  short_id: string
  amendment_count: number
  note: string | null
  email: string | null
  report_issue: number | null
}

interface FeedbackRow {
  id: number
  created_at: string
  feedback: string
  email: string | null
  app_version: string
  os_version: string
  issue_number: number | null
  /** The `ERR-XXXXX` of the logs the sender included, or null when they included none. */
  error_report_id: string | null
  /** That report's bundle, when it has arrived. */
  report_size: number | null
  report_key: string | null
}

interface OpsRow {
  id: number
  created_at: string
  kind: string
  message: string
  /** More than one for a repeating event (`countOpsEvent`). */
  occurrences: number
}

/** The rows one digest carries, by table, so the stamp touches exactly these and nothing newer. */
interface PendingRows {
  crashes: CrashRow[]
  errorReports: ErrorReportRow[]
  amendments: AmendmentRow[]
  feedback: FeedbackRow[]
  opsAlerts: OpsRow[]
}

/** Signups are the morning brief's to count, never an alert, so the digest neither mails nor stamps them. */
const NOT_A_SIGNUP = `kind NOT IN (${SIGNUP_KINDS.map((kind) => `'${kind}'`).join(', ')})`

async function readPending(db: D1Database): Promise<PendingRows> {
  const crashes = await db
    .prepare(
      `SELECT id, created_at, app_version, signal, top_function, short_id, email, panic_message, app_fate, image_base, os_exception, os_frames, issue_number
         FROM crash_reports WHERE notified_at IS NULL AND ${RELEASE_ONLY} ORDER BY created_at DESC`,
    )
    .all<CrashRow>()
  const errorReports = await db
    .prepare(
      `SELECT id, short_id, kind, app_version, os_version, arch, size_bytes, r2_key, user_note, email, issue_number
         FROM error_reports WHERE notified_at IS NULL AND build_mode = 'release' ORDER BY id DESC`,
    )
    .all<ErrorReportRow>()
  const amendments = await db
    .prepare(
      `SELECT a.id, a.short_id, a.amendment_count, a.note, a.email,
              (SELECT r.issue_number FROM error_reports r WHERE r.short_id = a.short_id ORDER BY r.id DESC LIMIT 1) AS report_issue
         FROM error_report_amendments a WHERE a.notified_at IS NULL AND a.env = 'prod' ORDER BY a.id DESC`,
    )
    .all<AmendmentRow>()
  const feedback = await db
    .prepare(
      `SELECT f.id, f.created_at, f.feedback, f.email, f.app_version, f.os_version, f.issue_number, f.error_report_id,
              r.size_bytes AS report_size, r.r2_key AS report_key
         FROM feedback f
         LEFT JOIN error_reports r ON r.id = (
           SELECT MAX(id) FROM error_reports WHERE short_id = f.error_report_id AND build_mode = 'release'
         )
         WHERE f.notified_at IS NULL AND COALESCE(f.build_mode, 'release') != 'debug'
         ORDER BY f.created_at DESC`,
    )
    .all<FeedbackRow>()
  const opsAlerts = await db
    .prepare(
      `SELECT id, created_at, kind, message, occurrences FROM ops_events
         WHERE notified_at IS NULL AND ${NOT_A_SIGNUP} ORDER BY id DESC`,
    )
    .all<OpsRow>()

  return {
    crashes: crashes.results,
    errorReports: errorReports.results,
    amendments: amendments.results,
    feedback: feedback.results,
    opsAlerts: opsAlerts.results,
  }
}

/**
 * Kinds that only alert once one digest window holds this many occurrences, across all their rows. A
 * payment provider retrying a delivery can produce a bad signature now and then; a run of them is
 * the signal. Every other kind alerts on its first occurrence, rejected admin requests included.
 */
const ALERT_THRESHOLDS: Partial<Record<OpsEventKind, number>> = { 'webhook-signature-rejected': 3 }

/**
 * Split the window's ops events into what the digest mails and what stays below its kind's
 * threshold. The quiet ones are stamped all the same, so the window closes on them and a few now
 * plus a few next time never add up to an alert.
 */
function splitBelowThreshold(rows: OpsRow[]): { alerts: OpsRow[]; quiet: OpsRow[] } {
  const totals = new Map<string, number>()
  for (const row of rows) totals.set(row.kind, (totals.get(row.kind) ?? 0) + row.occurrences)
  const alerts: OpsRow[] = []
  const quiet: OpsRow[] = []
  for (const row of rows) {
    const threshold = ALERT_THRESHOLDS[row.kind as OpsEventKind] ?? 1
    if ((totals.get(row.kind) ?? 0) >= threshold) alerts.push(row)
    else quiet.push(row)
  }
  return { alerts, quiet }
}

/** The alert's text, with how often it happened when that's more than once. */
function opsAlertMessage(row: OpsRow): string {
  return row.occurrences > 1 ? `${row.message} (${String(row.occurrences)} times)` : row.message
}

async function presignQuietly(env: Bindings, key: string): Promise<string | null> {
  try {
    return await buildPresignedUrl(env, key)
  } catch (e) {
    console.error('Report digest: presigning a bundle link failed', e)
    return null
  }
}

/**
 * The feedback entry's logs: the bundle and its link, or a line saying it hadn't arrived. Null when
 * the sender included no logs.
 */
async function feedbackLogs(env: Bindings, row: FeedbackRow): Promise<FeedbackEmailLogs | null> {
  if (!row.error_report_id) return null
  if (row.report_size === null || row.report_key === null) return { id: row.error_report_id, arrived: false }
  return {
    id: row.error_report_id,
    arrived: true,
    sizeBytes: row.report_size,
    downloadUrl: await presignQuietly(env, row.report_key),
    linkTtlHours: PRESIGN_TTL_HOURS,
  }
}

async function toContent(env: Bindings, rows: PendingRows): Promise<DigestContent> {
  // Logs sent with a feedback message in this digest show on that message's entry, so they aren't
  // listed again as an error report. They're still stamped with the rest (`handleReportDigest`).
  const logsOnFeedback = new Set(rows.feedback.map((row) => row.error_report_id).filter(Boolean))
  const feedback = []
  for (const row of rows.feedback) {
    feedback.push({
      when: row.created_at,
      version: row.app_version,
      osVersion: row.os_version,
      message: row.feedback,
      email: row.email,
      logs: await feedbackLogs(env, row),
      card: cardFor(env, row.issue_number),
    })
  }

  const errorReports = []
  for (const row of rows.errorReports.filter((report) => !logsOnFeedback.has(report.short_id))) {
    errorReports.push({
      id: row.short_id,
      kind: row.kind === 'user' ? ('user' as const) : ('auto' as const),
      appVersion: row.app_version,
      osVersion: row.os_version,
      arch: row.arch,
      sizeBytes: row.size_bytes,
      userNote: row.user_note,
      email: row.email,
      downloadUrl: await presignQuietly(env, row.r2_key),
      linkTtlHours: PRESIGN_TTL_HOURS,
      card: cardFor(env, row.issue_number),
    })
  }

  return {
    crashes: rows.crashes.map((row) => ({
      when: row.created_at,
      fate: appFateToLabel(row.app_fate),
      id: row.short_id ?? '?',
      site: row.top_function,
      signal: row.signal,
      version: row.app_version,
      email: row.email,
      message: row.panic_message,
      imageBase: row.image_base,
      osException: row.os_exception,
      osFrames: parseOsFrames(row.os_frames),
      card: cardFor(env, row.issue_number),
    })),
    errorReports,
    amendments: rows.amendments.map((row) => ({
      id: row.short_id,
      note: row.note,
      email: row.email,
      amendmentCount: row.amendment_count,
      card: cardFor(env, row.report_issue),
    })),
    feedback,
    opsAlerts: rows.opsAlerts.map((row) => ({ when: row.created_at, kind: row.kind, message: opsAlertMessage(row) })),
  }
}

async function stamp(db: D1Database, table: string, ids: number[], now: string): Promise<void> {
  if (ids.length === 0) return
  const placeholders = ids.map(() => '?').join(', ')
  await db
    .prepare(`UPDATE ${table} SET notified_at = ? WHERE id IN (${placeholders})`)
    .bind(now, ...ids)
    .run()
}

/** Mail whatever arrived since the last digest. A silent no-op without a recipient or a Resend key. */
export async function handleReportDigest(env: Bindings): Promise<void> {
  const to = humanReportRecipient(env)
  if (!to || !env.RESEND_API_KEY) return

  const pending = await readPending(env.TELEMETRY_DB)
  const { alerts, quiet } = splitBelowThreshold(pending.opsAlerts)
  const rows = { ...pending, opsAlerts: alerts }
  const quietIds = quiet.map((row) => row.id)
  const content = await toContent(env, rows)
  if (digestSize(content) === 0) {
    await stamp(env.TELEMETRY_DB, 'ops_events', quietIds, new Date().toISOString())
    return
  }

  await sendReportDigestEmail({ content, to, resendApiKey: env.RESEND_API_KEY })

  const now = new Date().toISOString()
  await stamp(
    env.TELEMETRY_DB,
    'crash_reports',
    rows.crashes.map((row) => row.id),
    now,
  )
  await stamp(
    env.TELEMETRY_DB,
    'error_reports',
    rows.errorReports.map((row) => row.id),
    now,
  )
  await stamp(
    env.TELEMETRY_DB,
    'error_report_amendments',
    rows.amendments.map((row) => row.id),
    now,
  )
  await stamp(
    env.TELEMETRY_DB,
    'feedback',
    rows.feedback.map((row) => row.id),
    now,
  )
  await stamp(env.TELEMETRY_DB, 'ops_events', [...rows.opsAlerts.map((row) => row.id), ...quietIds], now)
}
