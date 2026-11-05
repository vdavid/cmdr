/**
 * Operational notices (a cron job that threw, eviction, a CSP violation, a signup), as rows in the D1 `ops_events`
 * table. The three-hour report digest mails the alerts (`report-digest.ts`); the morning brief
 * counts the signups. Nothing personal is ever written here: no address, no note, no IP.
 *
 * Writing a row NEVER throws. These calls sit inside the paths whose failures they report (a cron
 * job that threw, a card that missed the board), so an ops-event failure taking the caller down with
 * it would turn one problem into two. A failed write is logged; for a cron failure, healthchecks.io
 * still alerts through its own channel (`cron-health.ts`).
 */

import { formatBytes } from './types'

export type OpsEventKind =
  | 'cron-failure'
  | 'board-add-failed'
  | 'board-rating-failed'
  | 'csp-violation'
  | 'eviction'
  | 'eviction-blocked'
  | 'intake-rejected'
  | 'admin-auth-rejected'
  | 'webhook-signature-rejected'
  | 'sale-notice-failed'
  | 'ci-health'
  | 'beta-signup'
  | 'newsletter-signup'
  | 'subprocessor-updates-signup'

/** Kinds the morning brief counts and the digest leaves out: a signup is news, not an alarm. */
export const SIGNUP_KINDS: readonly OpsEventKind[] = ['beta-signup', 'newsletter-signup', 'subprocessor-updates-signup']

/** SQL that keeps every non-signup `ops_events` row, for the digest's alerts and the brief's alert count. */
export const NOT_A_SIGNUP = `kind NOT IN (${SIGNUP_KINDS.map((kind) => `'${kind}'`).join(', ')})`

/** Write one event. A no-op without a database (some route tests), and never throws. */
export async function recordOpsEvent(db: D1Database | undefined, kind: OpsEventKind, message: string): Promise<void> {
  if (!db) return
  try {
    await db.prepare(`INSERT INTO ops_events (kind, message) VALUES (?, ?)`).bind(kind, message).run()
  } catch (e) {
    console.error(`Ops event (${kind}) could not be recorded: ${message}`, e)
  }
}

/**
 * Count one more occurrence of a repeating event: add to its unnotified row with the same kind and
 * message, or start one. The digest shows the count, so a burst is one line and one small row however
 * long it runs. Two first occurrences racing can each start a row; the digest sums them. Never throws.
 */
export async function countOpsEvent(db: D1Database | undefined, kind: OpsEventKind, message: string): Promise<void> {
  if (!db) return
  try {
    const bumped = await db
      .prepare(
        `UPDATE ops_events SET occurrences = occurrences + 1 WHERE kind = ? AND message = ? AND notified_at IS NULL`,
      )
      .bind(kind, message)
      .run()
    if (bumped.meta.changes > 0) return
    await db.prepare(`INSERT INTO ops_events (kind, message) VALUES (?, ?)`).bind(kind, message).run()
  } catch (e) {
    console.error(`Ops event (${kind}) could not be counted: ${message}`, e)
  }
}

/**
 * A request to an admin route that came without a valid admin token. Names the route PATTERN (as
 * Hono matched it), never the raw path, the caller, or what they sent, so the message set is the
 * route list and nothing a stranger chooses.
 */
export function rejectedAdminRequestMessage(method: string, routePattern: string): string {
  return `${method} ${routePattern} was turned away: the request had no valid admin token.`
}

export type SignedWebhookSource = 'Paddle' | 'GitHub'

export function rejectedWebhookMessage(source: SignedWebhookSource): string {
  return `A ${source} webhook delivery was turned away: its signature didn't verify.`
}

/**
 * How much of a thrown error survives into the event. Enough for a stack's useful head; the full
 * trace is in the Workers logs.
 */
const CRON_FAILURE_DETAIL_CAP = 1200

export interface CronFailureInfo {
  /** The job that threw, named as `index.ts` labels it (for example `Crash cards`). */
  job: string
  /** The tick's `scheduledTime` as ISO 8601, so the event lines up with the Workers log. */
  when: string
  /** The thrown value, stringified. */
  detail: string
  /** Ticks in a row the job has thrown, this one included (`cron-streaks.ts`). */
  failedTicks: number
  /** The `scheduledTime` of the streak's first tick, as ISO 8601. */
  firstFailedAt: string
}

export function cronFailureMessage(info: CronFailureInfo): string {
  const detail =
    info.detail.length > CRON_FAILURE_DETAIL_CAP
      ? `${info.detail.slice(0, CRON_FAILURE_DETAIL_CAP)}… (truncated)`
      : info.detail
  const streak =
    info.failedTicks > 1 ? ` (making ${String(info.failedTicks)} ticks in a row since ${info.firstFailedAt})` : ''
  return `Cron job "${info.job}" threw on the ${info.when} tick${streak}, so its work didn't happen: ${detail}. The other jobs on that tick still ran. Full stack: the Workers logs.`
}

export interface EvictionInfo {
  evictedCount: number
  freedBytes: number
  newTotalBytes: number
}

export function evictionMessage(info: EvictionInfo): string {
  return `Eviction removed the ${String(info.evictedCount)} oldest error report bundle(s), freeing ${formatBytes(info.freedBytes)}. The bucket now holds ${formatBytes(info.newTotalBytes)}.`
}

export interface EvictionBlockedInfo {
  totalBytes: number
  evictableBytes: number
  neededBytes: number
}

/**
 * The one in this set that matters most: the bucket is full of bundles too young to touch, so intake
 * is paused and reports are being turned away.
 */
export function evictionBlockedMessage(info: EvictionBlockedInfo): string {
  return `Error report intake is PAUSED. The bucket holds ${formatBytes(info.totalBytes)} and needs to shed ${formatBytes(info.neededBytes)}, but only ${formatBytes(info.evictableBytes)} is old enough to evict, so nothing was deleted. Either a flood filled the bucket with fresh bundles, or real traffic outgrew the watermarks. Intake resumes on its own once the bucket is back under the low watermark.`
}

export interface IntakeRejectedInfo {
  budgetBytes: number
  date: string
}

export function intakeRejectedMessage(info: IntakeRejectedInfo): string {
  return `Error report intake hit its daily budget of ${formatBytes(info.budgetBytes)} on ${info.date} and is turning uploads away (503) until tomorrow. Legitimate traffic is nowhere near this, so check for a flood before raising DAILY_INTAKE_BUDGET_BYTES.`
}

export interface CspViolationInfo {
  directive: string
  blockedUrl: string
  documentUrl: string
  sourceFile?: string
}

export function cspViolationMessage(info: CspViolationInfo): string {
  const source = info.sourceFile ? ` Script: ${info.sourceFile}.` : ''
  return `getcmdr.com's CSP (${info.directive}) blocked ${info.blockedUrl} on ${info.documentUrl}.${source} Visitors' browsers are refusing this request, so whatever feature makes it is broken for them. Allow the origin in apps/website/nginx-security-headers.conf if it's ours. This pair stays quiet for 24 hours.`
}
