/**
 * Minimal Discord webhook client for #error-reports notifications.
 *
 * No queue infra. Single retry on 429 honoring `Retry-After`, then
 * `console.error` and drop. The channel is internal-only, so at most one
 * dropped notification is acceptable.
 */

import { formatBytes } from './types'

export interface ErrorReportNotification {
  id: string
  kind: 'user' | 'auto'
  /**
   * Forwarded from the manifest. The embed title gets `[DEV]` for debug builds
   * (`cfg!(debug_assertions)`) and `[PROD]` for release builds so triage can tell
   * them apart at a glance regardless of channel. Defaults to `'release'` upstream
   * when older clients don't set it.
   */
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  arch: string
  sizeBytes: number
  uploadedUnixSeconds: number
  downloadUrl: string
  /** How long {@link downloadUrl} works, stated next to it. */
  linkTtlHours: number
  userNote?: string
}

export interface FeedbackNotification {
  /**
   * `[DEV]`/`[PROD]` prefix logic mirrors error reports: dev-build feedback (mostly
   * the maintainer testing) stays visually separate from real beta-tester traffic.
   */
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  /**
   * Whether the sender attached a reply-to address. Only the fact travels: ❌ no email address ever
   * goes to Discord (the privacy policy promises it). The address itself lives in the D1 `feedback`
   * row and the reports-repo comment.
   */
  hasReplyTo: boolean
  feedback: string
}

/** Which Listmonk list a signup joined. */
export type SignupList = 'beta' | 'newsletter'

/**
 * A signup Listmonk mailed a confirmation for (`website/listmonk-signup.ts` only pings then). Carries
 * no email on purpose: Discord never receives an address. The Listmonk link finds the subscriber.
 */
export interface ListSignupNotification {
  list: SignupList
  /** When the signup landed, rendered as a Discord relative timestamp (`<t:…:R>`). */
  signupUnixSeconds: number
  /** Deep link to the Listmonk admin filtered to the list. */
  listAdminUrl: string
}

export interface IntakeRejectedInfo {
  /** The ceiling that was hit, so the alert states the number without a lookup. */
  budgetBytes: number
  /** UTC day (`yyyy-mm-dd`) whose budget ran out. */
  date: string
}

export interface NotificationsSuppressedInfo {
  /** Pings allowed per day before suppression kicks in. */
  cap: number
  /** UTC day (`yyyy-mm-dd`) whose allowance ran out. */
  date: string
}

export interface EvictionBlockedInfo {
  /** Current bucket size. */
  totalBytes: number
  /** Bytes held by bundles old enough to evict. */
  evictableBytes: number
  /** Bytes that would have to go to reach the low watermark. */
  neededBytes: number
}

export interface EvictionInfo {
  evictedCount: number
  freedBytes: number
  newTotalBytes: number
}

export interface CronFailureInfo {
  /** The job that threw, named as `index.ts` labels it (for example `Crash notifications`). */
  job: string
  /** The tick's `scheduledTime` as ISO 8601, so the alert lines up with the Workers log. */
  when: string
  /** The thrown value, stringified. Capped in the payload; Discord rejects a body over 2000 chars. */
  detail: string
}

const ERROR_REPORT_EMBED_COLOR = 0xff6b6b
const USER_NOTE_EMBED_CAP = 500
const FEEDBACK_EMBED_COLOR = 0x5bc0de
/** Discord's green. Distinct from the error-report red and the feedback blue at a glance. */
const SIGNUP_EMBED_COLOR = 0x57f287
/**
 * Discord caps embed descriptions at 4096 chars. The full text always lives in the
 * D1 `feedback` table, so a truncated embed never loses data.
 */
const FEEDBACK_EMBED_CAP = 3500

/** Build the Discord webhook JSON body for a new error report. */
export function buildErrorReportPayload(n: ErrorReportNotification): unknown {
  const truncatedNote =
    n.userNote && n.userNote.length > USER_NOTE_EMBED_CAP
      ? n.userNote.slice(0, USER_NOTE_EMBED_CAP) + '… (full note in bundle)'
      : n.userNote

  const fields: { name: string; value: string; inline?: boolean }[] = [
    { name: 'Kind', value: n.kind, inline: true },
    { name: 'App version', value: n.appVersion, inline: true },
    { name: 'OS', value: n.osVersion, inline: true },
    { name: 'Arch', value: n.arch, inline: true },
    { name: 'Size', value: formatBytes(n.sizeBytes), inline: true },
    { name: 'Uploaded', value: `<t:${n.uploadedUnixSeconds.toString()}:R>`, inline: true },
    { name: 'Download', value: `[Download bundle](${n.downloadUrl}) (link valid ${n.linkTtlHours.toString()} hours)` },
  ]
  if (truncatedNote) {
    fields.push({ name: 'User note', value: truncatedNote })
  }

  const titlePrefix = n.buildMode === 'debug' ? '[DEV] ' : '[PROD] '
  return {
    embeds: [
      {
        title: `${titlePrefix}Error report ${n.id}`,
        color: ERROR_REPORT_EMBED_COLOR,
        fields,
      },
    ],
  }
}

/** Build the Discord webhook JSON body for a new in-app feedback message. */
export function buildFeedbackPayload(n: FeedbackNotification): unknown {
  const truncated =
    n.feedback.length > FEEDBACK_EMBED_CAP
      ? n.feedback.slice(0, FEEDBACK_EMBED_CAP) + '… (full text in the feedback table)'
      : n.feedback

  const fields: { name: string; value: string; inline?: boolean }[] = [
    { name: 'App version', value: n.appVersion, inline: true },
    { name: 'OS', value: n.osVersion, inline: true },
  ]
  if (n.hasReplyTo) {
    fields.push({ name: 'Reply-to attached', value: 'Yes (address in the feedback table)', inline: true })
  }

  const titlePrefix = n.buildMode === 'debug' ? '[DEV] ' : '[PROD] '
  return {
    embeds: [
      {
        title: `${titlePrefix}Feedback`,
        description: truncated,
        color: FEEDBACK_EMBED_COLOR,
        fields,
      },
    ],
  }
}

/** Build the Discord webhook JSON body for a signup Listmonk mailed a confirmation for. */
export function buildListSignupPayload(n: ListSignupNotification): unknown {
  const beta = n.list === 'beta'
  return {
    embeds: [
      {
        title: beta ? 'New beta-tester signup' : 'New newsletter signup',
        description: 'Status: unconfirmed — Listmonk sent them the confirmation email.',
        color: SIGNUP_EMBED_COLOR,
        fields: [
          { name: 'When', value: `<t:${n.signupUnixSeconds.toString()}:R>`, inline: true },
          { name: 'Listmonk', value: `[${beta ? 'Beta list' : 'Newsletter'} subscribers](${n.listAdminUrl})` },
        ],
      },
    ],
  }
}

/** Build the Discord webhook JSON body for an eviction summary. */
export function buildEvictionPayload(info: EvictionInfo): unknown {
  return {
    content:
      `Eviction sweep: removed ${info.evictedCount.toString()} oldest bundle(s), ` +
      `freed ${formatBytes(info.freedBytes)}. New total: ${formatBytes(info.newTotalBytes)}.`,
  }
}

/**
 * Build the Discord webhook JSON body for an exhausted daily intake budget. Sent at most once a
 * day (`claimBudgetAlert`), because the flood that triggers it would otherwise be the thing that
 * floods the webhook.
 */
export function buildIntakeRejectedPayload(info: IntakeRejectedInfo): unknown {
  return {
    content:
      `Error report intake hit its daily budget of ${formatBytes(info.budgetBytes)} on ${info.date} ` +
      `and is turning uploads away (503) until tomorrow. Legitimate traffic is nowhere near this, ` +
      `so check for a flood before raising \`DAILY_INTAKE_BUDGET_BYTES\`.`,
  }
}

/**
 * Build the Discord webhook JSON body for an eviction that refused to run. This is the alert that
 * matters most in the set: it means the bucket is full of bundles too young to touch, so intake is
 * now paused and reports are being turned away.
 */
export function buildEvictionBlockedPayload(info: EvictionBlockedInfo): unknown {
  return {
    content:
      `Error report intake PAUSED. The bucket holds ${formatBytes(info.totalBytes)} and needs to shed ` +
      `${formatBytes(info.neededBytes)}, but only ${formatBytes(info.evictableBytes)} is old enough to evict, ` +
      `so nothing was deleted. Either a flood filled the bucket with fresh bundles, or real traffic outgrew ` +
      `the watermarks. Intake resumes on its own once the bucket is back under the low watermark.`,
  }
}

/**
 * Build the Discord webhook JSON body for the one notice that says per-upload pings are done for
 * the day. Sent once, so the channel knows it went quiet on purpose.
 */
export function buildNotificationsSuppressedPayload(info: NotificationsSuppressedInfo): unknown {
  return {
    content:
      `That's ${info.cap.toString()} error report pings for ${info.date}, so the rest of today's are suppressed ` +
      `to keep this channel usable. Nothing is lost: every bundle is still in R2 and listed by ` +
      `\`GET /admin/error-reports\`. Pings resume tomorrow.`,
  }
}

/**
 * How much of a thrown error survives into the alert. Discord rejects a `content` over 2000
 * characters outright, and a rejected alert is the same as no alarm at all, so the cap is generous
 * enough for a stack's useful head and still leaves room for the surrounding prose.
 */
const CRON_FAILURE_DETAIL_CAP = 1200

/**
 * Build the Discord webhook JSON body for a cron job that threw.
 *
 * Plain `content`, deliberately: an embed is a nested shape Discord can reject on a field it
 * doesn't like, and this is the message that carries the news that something is broken. It's the
 * one alert that must not have its own failure mode.
 */
export function buildCronFailurePayload(info: CronFailureInfo): unknown {
  const detail =
    info.detail.length > CRON_FAILURE_DETAIL_CAP
      ? `${info.detail.slice(0, CRON_FAILURE_DETAIL_CAP)}… (truncated)`
      : info.detail

  return {
    content:
      `Cron job **${info.job}** threw on the ${info.when} tick, so its work didn't happen.\n` +
      '```\n' +
      detail.replace(/```/g, "'''") +
      '\n```\n' +
      'The other jobs on that tick still ran. Full stack: the `cmdr-license-server` Workers logs.',
  }
}

export interface CspViolationInfo {
  /** The CSP directive that blocked it, for example `connect-src`. */
  directive: string
  /** What the browser refused to load, query string stripped. */
  blockedUrl: string
  /** The page it happened on, query string stripped. */
  documentUrl: string
  /** The script that made the request, when the browser says. */
  sourceFile?: string
}

/**
 * Build the Discord webhook JSON body for a CSP violation on getcmdr.com. Plain `content`, like the
 * cron alert: it's short, and it must not have a failure mode of its own.
 */
export function buildCspViolationPayload(info: CspViolationInfo): unknown {
  const source = info.sourceFile ? `\nScript: ${info.sourceFile}` : ''
  return {
    content:
      `getcmdr.com's CSP (\`${info.directive}\`) blocked ${info.blockedUrl}\n` +
      `Page: ${info.documentUrl}${source}\n` +
      `Visitors' browsers are refusing this request, so whatever feature makes it is broken for them. ` +
      `Allow the origin in \`apps/website/nginx-security-headers.conf\` if it's ours. ` +
      `This pair stays quiet for 24 hours.`,
  }
}

async function postOnce(url: string, body: unknown): Promise<Response> {
  return fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
}

/**
 * POST `body` to the webhook. On 429, sleep for `Retry-After` and retry once.
 * On second failure, log and drop. No exception to caller.
 */
async function postWithRetry(url: string, body: unknown, label: string): Promise<void> {
  try {
    let res = await postOnce(url, body)
    if (res.status === 429) {
      const retryAfterRaw = res.headers.get('Retry-After') ?? '1'
      const retryAfterSec = Math.max(0, Math.min(60, parseFloat(retryAfterRaw) || 1))
      await new Promise((r) => setTimeout(r, retryAfterSec * 1000))
      res = await postOnce(url, body)
    }
    if (!res.ok) {
      console.error(`Discord ${label} POST failed: HTTP ${res.status.toString()}`)
    }
  } catch (e) {
    console.error(`Discord ${label} POST threw:`, e)
  }
}

export async function postErrorReportNotification(
  webhookUrl: string,
  notification: ErrorReportNotification,
): Promise<void> {
  await postWithRetry(webhookUrl, buildErrorReportPayload(notification), 'error-report')
}

export async function postEvictionNotification(webhookUrl: string, info: EvictionInfo): Promise<void> {
  await postWithRetry(webhookUrl, buildEvictionPayload(info), 'eviction')
}

export async function postIntakeRejectedNotification(webhookUrl: string, info: IntakeRejectedInfo): Promise<void> {
  await postWithRetry(webhookUrl, buildIntakeRejectedPayload(info), 'intake-rejected')
}

export async function postEvictionBlockedNotification(webhookUrl: string, info: EvictionBlockedInfo): Promise<void> {
  await postWithRetry(webhookUrl, buildEvictionBlockedPayload(info), 'eviction-blocked')
}

export async function postNotificationsSuppressedNotification(
  webhookUrl: string,
  info: NotificationsSuppressedInfo,
): Promise<void> {
  await postWithRetry(webhookUrl, buildNotificationsSuppressedPayload(info), 'notifications-suppressed')
}

export async function postCronFailureNotification(webhookUrl: string, info: CronFailureInfo): Promise<void> {
  await postWithRetry(webhookUrl, buildCronFailurePayload(info), 'cron-failure')
}

export async function postCspViolationNotification(webhookUrl: string, info: CspViolationInfo): Promise<void> {
  await postWithRetry(webhookUrl, buildCspViolationPayload(info), 'csp-violation')
}

export async function postFeedbackNotification(webhookUrl: string, notification: FeedbackNotification): Promise<void> {
  await postWithRetry(webhookUrl, buildFeedbackPayload(notification), 'feedback')
}

export async function postListSignupNotification(
  webhookUrl: string,
  notification: ListSignupNotification,
): Promise<void> {
  await postWithRetry(webhookUrl, buildListSignupPayload(notification), `${notification.list}-signup`)
}
