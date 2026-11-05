/**
 * The morning brief job: read the board and the last day's D1 rows, check the tripwires, and send
 * the brief (`email/morning-brief.ts`).
 *
 * Its cron fires twice, at 04:30 and 05:30 UTC (`MORNING_CRON`), because Cloudflare crons are UTC
 * and Stockholm moves between UTC+1 and UTC+2. {@link isMorningBriefTick} picks the one that is
 * 06:30 local, an hour after the triage agent runs, so the brief shows the cards it renamed.
 */

import { sendMorningBriefEmail, type BriefArrivals, type BriefCard } from './email/morning-brief'
import { humanReportRecipient } from './email/send'
import { readCiBriefLine } from './ci-brief'
import { NOT_A_SIGNUP } from './ops-events'
import { listOpenCards, resolveIssueTarget } from './github-issues'
import { NEEDS_REPLY_LABEL, UNPROCESSED_LABEL } from './report-cards'
import { ERROR_REPORT_PREFIX } from './telemetry/error-report-eviction'
import type { Bindings } from './types'

/** The trigger in `wrangler.toml`. `index.ts` dispatches on it. */
export const MORNING_CRON = '30 4,5 * * *'

/** True on the tick that is 06:xx in Stockholm, whatever the season. */
export function isMorningBriefTick(when: Date): boolean {
  const hour = new Intl.DateTimeFormat('en-GB', {
    timeZone: 'Europe/Stockholm',
    hour: '2-digit',
    hourCycle: 'h23',
  }).format(when)
  return hour === '06'
}

const RELEASE_ONLY = `COALESCE(build_mode, 'release') != 'debug'`

/**
 * A card younger than this is normal, not stuck: the outbox files every three hours. Older and
 * still unfiled means filing is failing, which the brief has to say out loud.
 */
const STUCK_AFTER_HOURS = 6

/** `YYYY-MM-DD HH:MM:SS` (the `datetime('now')` shape) and ISO 8601 (the crash table's shape), `hours` back. */
function cutoffs(hours: number): { sqlite: string; iso: string } {
  const iso = new Date(Date.now() - hours * 3_600_000).toISOString().slice(0, 19)
  return { sqlite: iso.replace('T', ' '), iso: `${iso}Z` }
}

async function count(db: D1Database, sql: string, ...bindings: unknown[]): Promise<number> {
  const row = await db
    .prepare(sql)
    .bind(...bindings)
    .first<{ n: number }>()
  return row?.n ?? 0
}

async function readArrivals(db: D1Database): Promise<BriefArrivals> {
  const day = cutoffs(24)
  const opsCount = (kindClause: string) =>
    count(db, `SELECT COUNT(*) AS n FROM ops_events WHERE created_at >= ? AND ${kindClause}`, day.sqlite)
  return {
    crashes: await count(
      db,
      `SELECT COUNT(*) AS n FROM crash_reports WHERE created_at >= ? AND ${RELEASE_ONLY}`,
      day.iso,
    ),
    handWrittenErrors: await count(
      db,
      `SELECT COUNT(*) AS n FROM error_reports WHERE created_at >= ? AND build_mode = 'release' AND kind = 'user'`,
      day.sqlite,
    ),
    autoSentErrors: await count(
      db,
      `SELECT COUNT(*) AS n FROM error_reports WHERE created_at >= ? AND build_mode = 'release' AND kind != 'user'`,
      day.sqlite,
    ),
    notesAdded: await count(
      db,
      `SELECT COUNT(*) AS n FROM error_report_amendments WHERE created_at >= ? AND env = 'prod'`,
      day.sqlite,
    ),
    feedback: await count(
      db,
      `SELECT COUNT(*) AS n FROM feedback WHERE created_at >= ? AND ${RELEASE_ONLY}`,
      day.sqlite,
    ),
    betaSignups: await opsCount(`kind = 'beta-signup'`),
    newsletterSignups: await opsCount(`kind = 'newsletter-signup'`),
    subprocessorUpdatesSignups: await opsCount(`kind = 'subprocessor-updates-signup'`),
    opsAlerts: await opsCount(NOT_A_SIGNUP),
  }
}

/** Release rows older than {@link STUCK_AFTER_HOURS} still owed a card: the outbox's own tripwire. */
async function countStuck(db: D1Database): Promise<number> {
  const stuck = cutoffs(STUCK_AFTER_HOURS)
  return (
    (await count(
      db,
      `SELECT COUNT(*) AS n FROM crash_reports WHERE issue_number IS NULL AND created_at < ? AND ${RELEASE_ONLY}`,
      stuck.iso,
    )) +
    (await count(
      db,
      `SELECT COUNT(*) AS n FROM error_reports WHERE issue_number IS NULL AND created_at < ? AND build_mode = 'release'`,
      stuck.sqlite,
    )) +
    (await count(
      db,
      `SELECT COUNT(*) AS n FROM feedback WHERE issue_number IS NULL AND created_at < ? AND ${RELEASE_ONLY}`,
      stuck.sqlite,
    )) +
    (await count(
      db,
      `SELECT COUNT(*) AS n FROM error_report_amendments WHERE board_done_at IS NULL AND created_at < ? AND env = 'prod'`,
      stuck.sqlite,
    ))
  )
}

/**
 * Yesterday's (UTC) prod bundles in R2 against yesterday's `error_reports` rows. The upload route
 * writes both, but a failed D1 insert only logs, so a bundle with no row is a report nobody would
 * ever hear about. This is the check that makes that loud. Null when the counts agree.
 */
async function reconcileBundles(env: Bindings): Promise<string | null> {
  const day = new Date(Date.now() - 86_400_000).toISOString().slice(0, 10)
  const prefix = `${ERROR_REPORT_PREFIX}prod/${day}/`

  let bundles = 0
  let cursor: string | undefined
  do {
    const page = await env.ERROR_REPORTS_BUCKET.list({ prefix, ...(cursor ? { cursor } : {}) })
    bundles += page.objects.filter((object) => object.key.endsWith('.zip')).length
    cursor = page.truncated ? page.cursor : undefined
  } while (cursor)

  const rows = await count(
    env.TELEMETRY_DB,
    `SELECT COUNT(*) AS n FROM error_reports WHERE build_mode = 'release' AND created_at LIKE ?`,
    `${day} %`,
  )
  if (bundles <= rows) return null
  return `R2 holds ${String(bundles)} bundles from ${day}, but only ${String(rows)} reached the outbox, so the rest have no card and no digest line. Workers logs: "queueing the report for the board failed".`
}

function toBriefCards(cards: { number: number; title: string; url: string; createdAt: string }[]): BriefCard[] {
  return cards.map((card) => ({ ...card, openedOn: card.createdAt.slice(0, 10) }))
}

/** Send the brief. A silent no-op without a recipient or an SMTP2Go key. */
export async function handleMorningBrief(env: Bindings): Promise<void> {
  const to = humanReportRecipient(env)
  if (!to || !env.SMTP2GO_API_KEY) return

  const tripwires: string[] = []
  let needsReply: BriefCard[] | null = []
  let unprocessed: BriefCard[] | null = []
  const target = resolveIssueTarget(env)
  if (target) {
    try {
      needsReply = toBriefCards(await listOpenCards(target, NEEDS_REPLY_LABEL))
      unprocessed = toBriefCards(await listOpenCards(target, UNPROCESSED_LABEL))
    } catch (e) {
      console.error('Morning brief: reading the board failed', e)
      needsReply = null
      unprocessed = null
      tripwires.push(
        "Couldn't read the board on GitHub, so the two lists above are missing. Check GITHUB_ISSUES_TOKEN.",
      )
    }
  }

  const stuck = await countStuck(env.TELEMETRY_DB)
  if (stuck > 0) {
    tripwires.push(
      `${String(stuck)} ${stuck === 1 ? 'report still waiting for its card' : 'reports still waiting for their cards'} after ${String(STUCK_AFTER_HOURS)} hours. Filing is failing; the digest's ops alerts say why.`,
    )
  }

  const reconciled = await reconcileBundles(env)
  if (reconciled) tripwires.push(reconciled)

  await sendMorningBriefEmail({
    content: {
      needsReply,
      unprocessed,
      arrivals: await readArrivals(env.TELEMETRY_DB),
      tripwires,
      ci: await readCiBriefLine(env),
    },
    to,
    smtp2goApiKey: env.SMTP2GO_API_KEY,
  })
}
