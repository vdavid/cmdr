/**
 * The report outbox: every three hours, the cron files each pending crash, error report, feedback
 * message, and amendment as a card on the private reports board (`github-issues.ts`).
 *
 * Filing happens HERE rather than at intake on purpose. Intake writes a D1 row and is done; a row
 * with `issue_number IS NULL` is a card still owed. A GitHub outage, an expired token, or a tripped
 * daily cap leaves the row pending and a later tick files it, so a report can be late but never
 * lost. The digest (`report-digest.ts`) runs after this on the same tick and links the new cards.
 *
 * Every job throws when a filing failed, after trying the rest, so `runCronJob` raises the alarm:
 * "cards aren't reaching the board" must never be quiet. A tripped cap is not a failure.
 */

import {
  claimIssueSlot,
  fileIssue,
  recordAmendmentOnBoard,
  rememberIssueNumber,
  resolveIssueTarget,
  todayUtc,
  type FileIssueInput,
} from './github-issues'
import {
  CRASH_EMAIL_RETENTION_DAYS,
  ERROR_REPORT_RETENTION_DAYS,
  FEEDBACK_EMAIL_RETENTION_DAYS,
  buildAmendmentComment,
  buildCrashIssue,
  buildErrorReportIssue,
  buildFeedbackIssue,
  buildPersonalComment,
  expiryAfter,
  type FeedbackLogs,
  type IssueSource,
} from './report-cards'
import { readReportFacts } from './telemetry/error-report-amend'
import { PRESIGN_TTL_HOURS, buildPresignedUrl } from './telemetry/error-report'
import type { Bindings } from './types'

/**
 * Rows looked at per job per tick. The daily cap (20) is the real bound; this only keeps one tick's
 * runtime predictable when a backlog builds up.
 */
const ROWS_PER_TICK = 25

/** SQL that keeps debug builds out. A NULL `build_mode` is an old client, which means a release build. */
const RELEASE_ONLY = `COALESCE(build_mode, 'release') != 'debug'`

/** What one pending row turns into, once its source has said how to build it. */
interface PendingCard {
  rowId: number
  content: FileIssueInput
  /** Runs after the card exists, for bookkeeping beyond the row's own `issue_number`. */
  afterFiling?: (issueNumber: number) => Promise<void>
}

/** Tally of one job's tick, turned into the throw that wakes someone when anything failed. */
interface FilingOutcome {
  filed: number
  failed: number
}

/**
 * File `cards` from one source in order, stopping when the day's allowance runs out. Each success
 * stamps its row's `issue_number`, so a crash between two cards never files the first one twice.
 */
async function fileCards(
  env: Bindings,
  source: IssueSource,
  table: 'crash_reports' | 'error_reports' | 'feedback',
  cards: PendingCard[],
): Promise<FilingOutcome> {
  const outcome: FilingOutcome = { filed: 0, failed: 0 }
  const target = resolveIssueTarget(env)
  if (!target) return outcome

  for (const card of cards) {
    if (!(await claimIssueSlot(env.ERROR_REPORT_META, source, todayUtc()))) {
      console.log(`Report outbox: the daily ${source} cap is spent; the rest wait for tomorrow`)
      break
    }
    const issueNumber = await fileIssue(target, card.content)
    if (issueNumber === null) {
      outcome.failed++
      continue
    }
    await env.TELEMETRY_DB.prepare(`UPDATE ${table} SET issue_number = ? WHERE id = ?`)
      .bind(issueNumber, card.rowId)
      .run()
    outcome.filed++
    if (card.afterFiling) {
      try {
        await card.afterFiling(issueNumber)
      } catch (e) {
        console.error(`Report outbox: bookkeeping after filing #${String(issueNumber)} failed`, e)
      }
    }
  }
  return outcome
}

/** Throw when anything failed, after everything was tried. */
function raiseOnFailure(source: string, outcome: FilingOutcome): void {
  if (outcome.failed === 0) return
  throw new Error(
    `${String(outcome.failed)} ${source} card(s) failed to file (${String(outcome.filed)} filed); they stay pending and retry next tick. Workers logs have the GitHub responses.`,
  )
}

/** Release crashes still owed a card, oldest first. */
export async function fileCrashCards(env: Bindings): Promise<void> {
  if (!resolveIssueTarget(env)) return

  const { results } = await env.TELEMETRY_DB.prepare(
    `SELECT id, created_at, app_version, os_version, arch, signal, top_function, short_id, email, panic_message, app_fate, image_base, os_exception, os_frames
         FROM crash_reports
         WHERE issue_number IS NULL AND ${RELEASE_ONLY}
         ORDER BY id
         LIMIT ?`,
  )
    .bind(ROWS_PER_TICK)
    .all<{
      id: number
      created_at: string
      app_version: string
      os_version: string
      arch: string
      signal: string
      top_function: string
      short_id: string | null
      email: string | null
      panic_message: string | null
      app_fate: string | null
      image_base: string | null
      os_exception: string | null
      os_frames: string | null
    }>()

  const cards = results.map((row) => ({
    rowId: row.id,
    content: {
      ...buildCrashIssue({
        rowId: row.id,
        shortId: row.short_id,
        createdAt: row.created_at,
        appVersion: row.app_version,
        osVersion: row.os_version,
        arch: row.arch,
        signal: row.signal,
        topFunction: row.top_function,
        appFate: row.app_fate,
        panicMessage: row.panic_message,
        osException: row.os_exception,
        osFrames: parseOsFrames(row.os_frames),
        imageBase: row.image_base,
        email: row.email,
      }),
      personalComment: buildPersonalComment({
        email: row.email,
        expiresOn: expiryAfter(row.created_at, CRASH_EMAIL_RETENTION_DAYS),
      }),
    },
  }))

  raiseOnFailure('crash', await fileCards(env, 'crash-report', 'crash_reports', cards))
}

/** Release error reports, hand-written and auto-sent alike, still owed a card, oldest first. */
export async function fileErrorReportCards(env: Bindings): Promise<void> {
  if (!resolveIssueTarget(env)) return

  // Logs sent with a feedback message wait for that message's card and ride on it
  // (`fileFeedbackCards`), so a report whose feedback still owes a card is held back. Logs that
  // arrive after the feedback card was filed get their own card, pointing at it.
  const { results } = await env.TELEMETRY_DB.prepare(
    `SELECT id, short_id, created_at, kind, app_version, os_version, arch, size_bytes, r2_key, user_note, email,
            (SELECT f.issue_number FROM feedback f WHERE f.error_report_id = error_reports.short_id ORDER BY f.id DESC LIMIT 1) AS feedback_issue
         FROM error_reports
         WHERE issue_number IS NULL AND build_mode = 'release'
           AND NOT EXISTS (
             SELECT 1 FROM feedback f
               WHERE f.error_report_id = error_reports.short_id AND f.issue_number IS NULL
                 AND COALESCE(f.build_mode, 'release') != 'debug'
           )
         ORDER BY id
         LIMIT ?`,
  )
    .bind(ROWS_PER_TICK)
    .all<{
      id: number
      short_id: string
      created_at: string
      kind: string
      app_version: string
      os_version: string
      arch: string
      size_bytes: number
      r2_key: string
      user_note: string | null
      email: string | null
      feedback_issue: number | null
    }>()

  const cards: PendingCard[] = []
  for (const row of results) {
    const downloadUrl = await presignQuietly(env, row.r2_key)
    cards.push({
      rowId: row.id,
      content: {
        ...buildErrorReportIssue({
          id: row.short_id,
          kind: row.kind === 'user' ? 'user' : 'auto',
          buildMode: 'release',
          appVersion: row.app_version,
          osVersion: row.os_version,
          arch: row.arch,
          sizeBytes: row.size_bytes,
          r2Key: row.r2_key,
          uploadedUnixSeconds: Math.floor(Date.parse(`${row.created_at.replace(' ', 'T')}Z`) / 1000),
          email: row.email,
          feedbackCard: row.feedback_issue,
        }),
        personalComment: buildPersonalComment({
          userNote: row.user_note,
          email: row.email,
          downloadUrl,
          linkTtlHours: PRESIGN_TTL_HOURS,
          expiresOn: expiryAfter(row.created_at, ERROR_REPORT_RETENTION_DAYS),
        }),
      },
      // The KV pointer is what an amendment to a report from before this table finds its card by.
      afterFiling: (issueNumber) => rememberIssueNumber(env.ERROR_REPORT_META, row.short_id, issueNumber),
    })
  }

  raiseOnFailure('error report', await fileCards(env, 'error-report', 'error_reports', cards))
}

/**
 * Release feedback still owed a card, oldest first. Feedback sent with logs carries the report's
 * `ERR-XXXXX`, and that report rides on this card: its technical facts in the body, its download
 * link in the comment, and its row stamped with this card's number so it never files one of its own
 * (and an amendment to it lands here).
 */
export async function fileFeedbackCards(env: Bindings): Promise<void> {
  if (!resolveIssueTarget(env)) return

  const { results } = await env.TELEMETRY_DB.prepare(
    `SELECT f.id, f.created_at, f.feedback, f.email, f.app_version, f.os_version, f.error_report_id,
            r.id AS report_row, r.size_bytes AS report_size, r.r2_key AS report_key
         FROM feedback f
         LEFT JOIN error_reports r ON r.id = (
           SELECT MAX(id) FROM error_reports
             WHERE short_id = f.error_report_id AND build_mode = 'release' AND issue_number IS NULL
         )
         WHERE f.issue_number IS NULL AND COALESCE(f.build_mode, 'release') != 'debug'
         ORDER BY f.id
         LIMIT ?`,
  )
    .bind(ROWS_PER_TICK)
    .all<{
      id: number
      created_at: string
      feedback: string
      email: string | null
      app_version: string
      os_version: string
      error_report_id: string | null
      report_row: number | null
      report_size: number | null
      report_key: string | null
    }>()

  const cards: PendingCard[] = []
  for (const row of results) {
    const logs = feedbackLogs(row)
    const downloadUrl = logs?.arrived ? await presignQuietly(env, logs.r2Key) : null
    cards.push({
      rowId: row.id,
      content: {
        ...buildFeedbackIssue({
          rowId: row.id,
          buildMode: 'release' as const,
          appVersion: row.app_version,
          osVersion: row.os_version,
          feedback: row.feedback,
          email: row.email,
          logs,
        }),
        // Only the reply-to address and the bundle link expire; the message itself lives in the
        // body. The address sets the date when there is one, else the logs' 90 days do.
        personalComment: buildPersonalComment({
          email: row.email,
          downloadUrl,
          linkTtlHours: PRESIGN_TTL_HOURS,
          expiresOn: expiryAfter(
            row.created_at,
            row.email?.trim() ? FEEDBACK_EMAIL_RETENTION_DAYS : ERROR_REPORT_RETENTION_DAYS,
          ),
        }),
      },
      afterFiling: async (issueNumber) => {
        if (!logs?.arrived || row.report_row === null) return
        await env.TELEMETRY_DB.prepare(
          `UPDATE error_reports SET issue_number = ? WHERE id = ? AND issue_number IS NULL`,
        )
          .bind(issueNumber, row.report_row)
          .run()
        await rememberIssueNumber(env.ERROR_REPORT_META, logs.id, issueNumber)
      },
    })
  }

  raiseOnFailure('feedback', await fileCards(env, 'feedback', 'feedback', cards))
}

/** What a feedback row's linked report looks like on its card, or null when it came without logs. */
function feedbackLogs(row: {
  error_report_id: string | null
  report_size: number | null
  report_key: string | null
}): FeedbackLogs | null {
  if (!row.error_report_id) return null
  if (row.report_size === null || row.report_key === null) return { id: row.error_report_id, arrived: false }
  return { id: row.error_report_id, arrived: true, sizeBytes: row.report_size, r2Key: row.report_key }
}

/**
 * Amendments still owed a place on the board, oldest first. Runs after the report jobs on the same
 * tick, so a note added minutes after its upload lands as a comment on the card filed just before.
 *
 * - The report's card exists: comment on it (and add `needs-reply` when the amendment brought an
 *   address).
 * - The report is in the outbox but its card is still owed: wait. Filing an "amended" card now
 *   would leave the report's own card a duplicate a tick later.
 * - The report predates the outbox: `recordAmendmentOnBoard` asks the KV pointer, and files the
 *   card itself when there's none.
 */
export async function postAmendmentsToBoard(env: Bindings): Promise<void> {
  if (!resolveIssueTarget(env)) return

  const { results } = await env.TELEMETRY_DB.prepare(
    `SELECT a.id, a.short_id, a.report_date, a.env, a.r2_key, a.amendment_count, a.note, a.email,
            (SELECT r.issue_number FROM error_reports r WHERE r.short_id = a.short_id ORDER BY r.id DESC LIMIT 1) AS report_issue,
            (SELECT COUNT(*) FROM error_reports r WHERE r.short_id = a.short_id) AS report_rows
         FROM error_report_amendments a
         WHERE a.board_done_at IS NULL
         ORDER BY a.id
         LIMIT ?`,
  )
    .bind(ROWS_PER_TICK)
    .all<{
      id: number
      short_id: string
      report_date: string
      env: string
      r2_key: string
      amendment_count: number
      note: string | null
      email: string | null
      report_issue: number | null
      report_rows: number
    }>()

  let failed = 0
  for (const row of results) {
    // Debug builds are our own E2E traffic, and their reports never get a card to comment on.
    const reachesBoard = row.env === 'prod'
    if (reachesBoard) {
      if (row.report_rows > 0 && row.report_issue === null) continue

      const result = await recordAmendmentOnBoard(env, {
        id: row.short_id,
        comment: buildAmendmentComment({
          note: row.note,
          email: row.email,
          amendmentCount: row.amendment_count,
          // The ORIGINAL report's deletion date, so an amendment can't outlive the bundle it belongs to.
          expiresOn: expiryAfter(row.report_date, ERROR_REPORT_RETENTION_DAYS),
        }),
        // An amendment is often how someone supplies the reply-to they left out, and `needs-reply`
        // is the only thing on the board that says somebody is waiting.
        addLabels: row.email ? ['needs-reply'] : [],
        readReportFacts: () =>
          readReportFacts(
            env.ERROR_REPORTS_BUCKET,
            { env: 'prod', key: row.r2_key },
            {
              id: row.short_id,
              email: row.email,
            },
          ),
        ...(row.report_issue ? { issueNumber: row.report_issue } : {}),
      })
      if (result === 'skipped') {
        failed++
        continue
      }
    }

    await env.TELEMETRY_DB.prepare(`UPDATE error_report_amendments SET board_done_at = ? WHERE id = ?`)
      .bind(new Date().toISOString(), row.id)
      .run()
  }

  if (failed > 0) {
    throw new Error(
      `${String(failed)} amendment(s) didn't reach the board; they stay pending and retry next tick. Workers logs have the reason.`,
    )
  }
}

/** A presigned bundle link, or null when R2 credentials are missing or signing fails. */
async function presignQuietly(env: Bindings, key: string): Promise<string | null> {
  try {
    return await buildPresignedUrl(env, key)
  } catch (e) {
    console.error('Report outbox: presigning a bundle link failed', e)
    return null
  }
}

/**
 * `os_frames` is stored as the JSON array the client sent, truncated to a byte budget at write
 * time, so a long stack can leave the column holding a partial array. A parse failure therefore
 * means "the stack didn't fit": drop the frames and file the rest. ❌ Never let this throw.
 */
export function parseOsFrames(stored: string | null | undefined): string[] {
  if (!stored) return []
  try {
    const parsed: unknown = JSON.parse(stored)
    return Array.isArray(parsed) ? parsed.filter((frame): frame is string => typeof frame === 'string') : []
  } catch {
    return []
  }
}
