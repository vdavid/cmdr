/**
 * What goes on a card in the private reports board: titles, bodies, labels, and the one comment
 * that holds everything personal. Pure functions, no network; `github-issues.ts` is the transport
 * and `report-outbox.ts` decides when.
 *
 * The invariant that lives here, and is load-bearing for the privacy policy
 * (`apps/website/src/pages/privacy-policy.astro`): **personal data goes only in a comment that
 * expires.** The issue title and body carry technical facts that name nobody and may be kept
 * indefinitely. Anything a person wrote or attached goes in a separate comment stamped with its own
 * deletion date, which the retention sweep deletes on time (`scheduled.ts`). Deleting a comment
 * removes it outright; editing a body would leave the original text in GitHub's revision history,
 * and a title's old values stay in the issue's timeline, which is why the split exists at all.
 *
 * Untrusted text is always wrapped with {@link fencedBlock}, so a note can't become markup, an
 * `@mention`, or a cross-repo reference.
 */

import { formatBytes } from './types'

/**
 * Hard ceiling GitHub puts on an issue body and on a comment. We stay under it with margin rather
 * than at it, because the chrome around a note is counted too.
 */
const GITHUB_TEXT_LIMIT = 65_536
const SAFE_TEXT_BUDGET = GITHUB_TEXT_LIMIT - 5_536

/**
 * The prefix every personal-data comment starts with. The retention sweep matches on this to find
 * what it may delete, so ❌ never post personal data in a comment without it: an unmarked comment is
 * invisible to the sweep and would outlive its promise.
 */
export const PERSONAL_COMMENT_MARKER = '<!-- cmdr:personal-data'

/**
 * Default life of a personal-data comment, in days. Matches the error-report retention promise, and
 * it is the DEFAULT so that a caller who forgets to state one deletes too early rather than too
 * late.
 */
const DEFAULT_PERSONAL_RETENTION_DAYS = 90

/**
 * Wrap untrusted text in a code fence long enough that the text cannot close it. Everything inside
 * is inert: no headings, no `@mentions` that would notify a stranger, no `#123` cross-references.
 */
export function fencedBlock(text: string): string {
  let longestRun = 0
  let run = 0
  for (const char of text) {
    run = char === '`' ? run + 1 : 0
    if (run > longestRun) longestRun = run
  }
  const fence = '`'.repeat(Math.max(3, longestRun + 1))
  return `${fence}\n${text}\n${fence}`
}

/** Cut `text` to `maxChars`, marking the cut so nobody reads a truncated note as the whole note. */
function truncate(text: string, maxChars: number): string {
  if (text.length <= maxChars) return text
  return `${text.slice(0, maxChars)}\n\n[truncated at ${String(maxChars)} characters]`
}

/** `YYYY-MM-DD`, `days` from now. The stamp the retention sweep reads. */
function expiryDate(days: number): string {
  return new Date(Date.now() + days * 86_400_000).toISOString().slice(0, 10)
}

/**
 * `YYYY-MM-DD`, `days` after the day a row was written. Cards are filed by the cron, possibly days
 * after intake when GitHub was down, so a promise counted from "now" would outlive the one the
 * privacy policy makes, which counts from when the person sent it.
 *
 * Takes either `created_at` shape the tables use (`datetime('now')` or ISO 8601): only the day is read.
 */
export function expiryAfter(createdAt: string, days: number): string {
  const day = Date.parse(`${createdAt.slice(0, 10)}T00:00:00Z`)
  const base = Number.isNaN(day) ? Date.now() : day
  return new Date(base + days * 86_400_000).toISOString().slice(0, 10)
}

/** Read the deletion date off a personal-data comment. Null when it carries no valid stamp. */
export function personalCommentExpiry(commentBody: string): string | null {
  const match = /^<!-- cmdr:personal-data expires=(\d{4}-\d{2}-\d{2}) -->/.exec(commentBody)
  return match?.[1] ?? null
}

/** What a personal-data comment may carry. Every field is optional; all-absent means no comment. */
export interface PersonalCommentInput {
  userNote?: string | null
  email?: string | null
  downloadUrl?: string | null
  linkTtlHours?: number
  retentionDays?: number
  /**
   * An absolute `YYYY-MM-DD` deletion date, overriding `retentionDays`. An amendment uses this to
   * expire with the report it amends rather than 90 days from when it was written, which would
   * outlive the bundle it belongs to.
   */
  expiresOn?: string
}

/**
 * Build the one comment that holds everything a person wrote or attached, stamped with the date the
 * retention sweep must delete it. Returns null when there is nothing personal to store, so an
 * anonymous auto-report never gets an empty comment that would later need deleting.
 */
export function buildPersonalComment(input: PersonalCommentInput): string | null {
  const note = input.userNote?.trim() ?? ''
  const email = input.email?.trim() ?? ''
  const hasLink = Boolean(input.downloadUrl)
  if (!note && !email && !hasLink) return null

  const retentionDays = input.retentionDays ?? DEFAULT_PERSONAL_RETENTION_DAYS
  const expiresOn = input.expiresOn ?? expiryDate(retentionDays)
  const sections: string[] = [
    `${PERSONAL_COMMENT_MARKER} expires=${expiresOn} -->`,
    `_Deleted automatically on ${expiresOn}, per the privacy policy. The issue above stays._`,
  ]

  if (note) {
    sections.push('**What they wrote**', fencedBlock(truncate(note, SAFE_TEXT_BUDGET / 2)))
  }
  if (email) {
    sections.push('**Reply to**', fencedBlock(email))
  }
  if (input.downloadUrl) {
    const ttl = input.linkTtlHours
    const suffix = ttl ? ` (link expires in ${String(ttl)} hours)` : ''
    sections.push('**Bundle**', `[Download the bundle](${input.downloadUrl})${suffix}`)
  }

  return truncate(sections.join('\n\n'), SAFE_TEXT_BUDGET)
}

/**
 * `macOS <version>`, without doubling the prefix when the client already sent one.
 *
 * Older clients send a bare `15.3.1`, some send `macOS 10.15.8`. ERR-KVERS was titled
 * "macOS macOS 10.15.8" before this existed.
 */
function macOsLabel(osVersion: string): string {
  const trimmed = osVersion.trim()
  return /^macos\b/i.test(trimmed) ? trimmed : `macOS ${trimmed}`
}

/** The three parts of an issue, built before anything touches the network. */
export interface IssueContent {
  title: string
  body: string
  labels: string[]
}

/**
 * Every new card starts here. The morning triage agent researches each one, renames it, and takes
 * the label off, so an open card that still carries it is one nobody has looked at yet.
 */
export const UNPROCESSED_LABEL = 'unprocessed'

/** A person attached an address and is waiting on an answer. */
export const NEEDS_REPLY_LABEL = 'needs-reply'

/** The labels every card gets: where it came from, `unprocessed`, and `needs-reply` when someone waits. */
function cardLabels(source: IssueSource, email: string | null | undefined): string[] {
  const labels: string[] = [source === 'feedback' ? 'in-app-feedback' : source, UNPROCESSED_LABEL]
  if (email?.trim()) labels.push(NEEDS_REPLY_LABEL)
  return labels
}

/** Cut a title fragment to `maxChars`, so one long symbol can't push the facts off the end. */
function clip(text: string, maxChars: number): string {
  return text.length <= maxChars ? text : `${text.slice(0, maxChars - 1)}…`
}

/** Technical facts about one uploaded bundle. Nothing here identifies a person. */
export interface ErrorReportIssueInput {
  id: string
  kind: 'user' | 'auto'
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  arch: string
  sizeBytes: number
  r2Key: string
  uploadedUnixSeconds: number
  userNote?: string | null
  email?: string | null
  /**
   * Whether an amendment is what earned this card, rather than the upload. Says so on the card,
   * because an auto-sent report with a card is otherwise a puzzle for whoever triages it.
   */
  filedFromAmendment?: boolean
  /**
   * The card of the feedback message these logs were sent with, when that card was filed before
   * the logs arrived. Normally the logs ride on the feedback card and never get one of their own.
   */
  feedbackCard?: number | null
}

/**
 * The issue for one error report. The note and the reply-to address are deliberately NOT here: they
 * belong in the expiring comment, because this body is kept for as long as the bug is. That holds
 * for the TITLE too, which is why it's technical until the triage agent writes a better one: GitHub
 * keeps every earlier title in the issue's timeline, so a note in it could never be deleted.
 */
export function buildErrorReportIssue(input: ErrorReportIssueInput): IssueContent {
  const kindLine = input.kind === 'user' ? 'hand-written' : 'auto-sent'
  const titleKind = input.filedFromAmendment ? `${kindLine}, note added later` : kindLine
  const uploadedAt = new Date(input.uploadedUnixSeconds * 1000).toISOString().replace('T', ' ').slice(0, 16)

  const body = [
    `**Report id**: \`${input.id}\``,
    `**Kind**: ${kindLine}${input.filedFromAmendment ? ', with a note the reporter added afterwards' : ''}`,
    `**App version**: ${input.appVersion}`,
    `**OS**: ${macOsLabel(input.osVersion)} · **Arch**: ${input.arch}`,
    `**Bundle**: ${formatBytes(input.sizeBytes)}, uploaded ${uploadedAt} UTC`,
    `**R2 key**: \`${input.r2Key}\``,
    ...(input.feedbackCard ? [`**Sent with**: the feedback on #${String(input.feedbackCard)}`] : []),
    '',
    'The note, the reply-to address, and the download link (when there are any) are in the first',
    'comment, which is deleted 90 days after the upload. To fetch the bundle after the link expires,',
    'see `docs/tooling/feedback-and-error-digest.md`.',
  ].join('\n')

  return {
    title: `Error report ${input.id}: ${titleKind} on ${input.appVersion} (${macOsLabel(input.osVersion)})`,
    body,
    labels: cardLabels('error-report', input.email),
  }
}

/** One crash report, already in D1. Every field here is technical; the reply-to is the only personal one. */
export interface CrashIssueInput {
  rowId: number
  /** `CRASH-XXXXX`, or null for clients older than the field. */
  shortId: string | null
  createdAt: string
  appVersion: string
  osVersion: string
  arch: string
  signal: string
  topFunction: string
  /** `app_fate`: `ended`, `keptRunning`, or null when the row claims nothing. */
  appFate: string | null
  /** Redacted on the user's Mac before it was sent; kept indefinitely like the rest of the row. */
  panicMessage: string | null
  osException: string | null
  osFrames: string[]
  imageBase: string | null
  email?: string | null
}

/** Frames of the macOS stack a crash card shows. The full list is in the D1 row. */
const CRASH_FRAMES_SHOWN = 12

/**
 * The issue for one crash. The row's technical facts go in the body (the privacy policy keeps them
 * indefinitely, and they name nobody); only the reply-to, when there is one, goes in the comment.
 */
export function buildCrashIssue(input: CrashIssueInput): IssueContent {
  const survived = input.appFate === 'keptRunning'
  const fateLine = survived
    ? 'a background panic; the app kept running'
    : input.appFate === 'ended'
      ? 'the app went down'
      : 'unknown (the report says nothing either way)'
  const idPart = input.shortId ? ` ${input.shortId}` : ''
  const what = survived ? 'Panic' : 'Crash'

  const lines = [
    `**Crash id**: \`${input.shortId ?? 'none (older client)'}\` · D1 row \`${String(input.rowId)}\` in \`crash_reports\``,
    `**What happened**: ${fateLine}`,
    `**Where**: \`${input.topFunction}\` · **Signal**: ${input.signal}`,
    `**App version**: ${input.appVersion}`,
    `**OS**: ${macOsLabel(input.osVersion)} · **Arch**: ${input.arch}`,
    `**Reported**: ${input.createdAt.replace('T', ' ').slice(0, 16)} UTC`,
  ]
  if (input.panicMessage) lines.push('', '**Panic message**', '', fencedBlock(input.panicMessage))
  if (input.osException) lines.push('', '**macOS says**', '', fencedBlock(input.osException))
  if (input.osFrames.length) {
    const shown = input.osFrames.slice(0, CRASH_FRAMES_SHOWN)
    const rest = input.osFrames.length - shown.length
    const tail = rest > 0 ? `\n… ${String(rest)} more in the D1 row` : ''
    lines.push('', '**Top of the stack**', '', fencedBlock(shown.join('\n') + tail))
  }
  if (!input.panicMessage && input.imageBase) lines.push('', `**Image base**: \`${input.imageBase}\``)
  lines.push(
    '',
    input.email?.trim()
      ? 'A reply-to address is in the first comment, which is deleted 90 days after the report.'
      : 'No reply-to address was attached.',
  )

  return {
    title: `${what}${idPart} in ${clip(input.topFunction, 80)} (${input.signal}) on ${input.appVersion} (${macOsLabel(input.osVersion)})`,
    body: truncate(lines.join('\n'), SAFE_TEXT_BUDGET),
    labels: cardLabels('crash-report', input.email),
  }
}

/**
 * The logs a feedback sender chose to include: an ordinary error report, uploaded right after the
 * feedback under the `ERR-XXXXX` the feedback row names. `arrived: false` when no such report was
 * in D1 when the card was filed (still in flight, or its upload didn't make it).
 */
export type FeedbackLogs =
  | { id: string; arrived: true; sizeBytes: number; r2Key: string }
  | { id: string; arrived: false }

/** One in-app feedback submission, already stored in D1. */
export interface FeedbackIssueInput {
  rowId: number
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  feedback?: string | null
  email?: string | null
  logs?: FeedbackLogs | null
}

/** The card's logs lines: the bundle's technical facts, or why there are none. Names nobody. */
function feedbackLogsLines(logs: FeedbackLogs | null | undefined): string[] {
  if (!logs) return []
  if (!logs.arrived) {
    return [
      `**Logs**: \`${logs.id}\`, sent separately, hadn't arrived when this card was filed. If it arrives, it gets its own card pointing here.`,
    ]
  }
  return [
    `**Logs**: \`${logs.id}\`, ${formatBytes(logs.sizeBytes)}, the past five hours, redacted on the Mac`,
    `**R2 key**: \`${logs.r2Key}\``,
  ]
}

/**
 * The issue for one feedback message. Unlike an error report, the MESSAGE lives in the body: the
 * privacy policy keeps feedback text so it can be acted on, and only the reply-to address expires.
 * So only the address goes in the expiring comment.
 */
export function buildFeedbackIssue(input: FeedbackIssueInput): IssueContent {
  const message = input.feedback?.trim() ?? ''
  const body = [
    `**Feedback row**: \`${String(input.rowId)}\` in the D1 \`feedback\` table`,
    `**App version**: ${input.appVersion}`,
    `**OS**: ${macOsLabel(input.osVersion)}`,
    ...feedbackLogsLines(input.logs),
    '',
    '**What they wrote**',
    '',
    fencedBlock(truncate(message, SAFE_TEXT_BUDGET / 2)),
    '',
    input.email?.trim()
      ? 'A reply-to address is in the first comment, which is deleted two years after it was sent.'
      : 'No reply-to address was attached.',
    ...(input.logs?.arrived
      ? [
          'The logs’ download link is in the first comment. To fetch the bundle after the link expires, see `docs/tooling/feedback-and-error-digest.md`.',
        ]
      : []),
  ].join('\n')

  return {
    title: `Feedback #${String(input.rowId)} on ${input.appVersion} (${macOsLabel(input.osVersion)})`,
    body: truncate(body, SAFE_TEXT_BUDGET),
    labels: cardLabels('feedback', input.email),
  }
}

/** Counted per source so a burst of one kind can never silence another. */
export type IssueSource = 'crash-report' | 'error-report' | 'feedback'

/** One amendment, as it goes onto the card. */
export interface AmendmentCommentInput {
  note?: string | null
  email?: string | null
  amendmentCount: number
  /** The ORIGINAL report's deletion date, so an amendment can't outlive the bundle it belongs to. */
  expiresOn: string
}

/**
 * The comment one amendment adds to its report's card.
 *
 * Always personal data by definition (an amendment is a note, an address, or both), so it is always
 * stamped and always swept. It carries the original report's date rather than its own: a report
 * amended on day 80 still has to disappear on day 90.
 */
export function buildAmendmentComment(input: AmendmentCommentInput): string {
  const ordinal = `Amendment #${String(input.amendmentCount)}`
  const comment = buildPersonalComment({
    userNote: input.note,
    email: input.email,
    expiresOn: input.expiresOn,
  })
  // `buildPersonalComment` returns null only when there is nothing personal, and the amend route
  // rejects a body carrying neither a note nor an address, so this cannot be empty in practice.
  const parts = comment ?? `${PERSONAL_COMMENT_MARKER} expires=${input.expiresOn} -->`
  return parts.replace('\n\n', `\n\n**${ordinal}**, added by the reporter after sending.\n\n`)
}

/**
 * The retention promises the cards' personal comments keep, from
 * `apps/website/src/pages/privacy-policy.astro` § "How long we keep your data". They mirror the
 * constants the D1 retention sweep uses in `scheduled.ts`; change one and the other has to follow.
 * A crash's reply-to shares the error report's 90 days.
 */
export const ERROR_REPORT_RETENTION_DAYS = 90
export const CRASH_EMAIL_RETENTION_DAYS = 90
export const FEEDBACK_EMAIL_RETENTION_DAYS = 730
