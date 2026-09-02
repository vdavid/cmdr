/**
 * The three emails a hand-written error report can produce: the report itself, an amendment
 * someone adds afterwards, and the one-line notice when the day's allowance runs out.
 *
 * All three go to the same inbox and share the card chrome with the feedback digest, so a person
 * writing to us always looks the same in the inbox whichever surface they used.
 */

import { Resend } from 'resend'
import { formatBytes } from '../types'
import { sendViaResend } from './send'
import {
  CARD_FOOTER_STYLE,
  CARD_HEADER_STYLE,
  CARD_PROSE_STYLE,
  CARD_STYLE,
  envChip,
  escapeHtml,
  notificationPage,
  replyToLine,
} from './layout'

/**
 * One hand-written error report, as its notification email renders it. Everything here either
 * comes off the reporter's machine or was typed by them, so every field is escaped at render time.
 */
export interface ErrorReportEmailRow {
  /** The `ERR-XXXXX` the person read in the send dialog, so a reply can name the same id. */
  id: string
  /** From the manifest. Drives the `dev` chip and the subject's `[DEV]` mark, same rule as the Discord embed. */
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  arch: string
  /** Bundle size in bytes, rendered through {@link formatBytes}. */
  sizeBytes: number
  /** What the person wrote. The reason this email exists; absent when they sent one without a note. */
  userNote?: string
  /** Reply-to address, present only when the reporter ticked "Attach my email" in the send dialog. */
  email?: string
  /** Presigned R2 GET URL, or `null` when the credentials to mint one aren't configured. */
  downloadUrl: string | null
  /** How long {@link downloadUrl} keeps working, so a stale click a week later isn't a mystery. */
  linkTtlDays: number
}

/**
 * The subject line, which is the whole email for anyone who doesn't open it. Debug builds are
 * marked so they don't read as real user traffic; release builds carry no mark, because a tag on
 * every ordinary report is noise in a list.
 */
function errorReportSubject(report: ErrorReportEmailRow): string {
  const prefix = report.buildMode === 'debug' ? '[DEV] ' : ''
  return `${prefix}Cmdr: someone sent error report ${report.id}`
}

/** The prose block: what they wrote, or a plain line saying there wasn't anything. */
function noteBlock(note: string | null | undefined, emptyLine: string): string {
  const trimmed = note?.trim()
  return trimmed
    ? `<div style="${CARD_PROSE_STYLE}">${escapeHtml(trimmed)}</div>`
    : `<div style="${CARD_PROSE_STYLE} color: #6b7280;">${escapeHtml(emptyLine)}</div>`
}

/** The card body: the note, the machine facts, and where the bundle is. */
function renderErrorReportCard(report: ErrorReportEmailRow): string {
  const days = report.linkTtlDays.toString()
  const downloadLine = report.downloadUrl
    ? `<a href="${escapeHtml(report.downloadUrl)}" style="color: #2563eb;">Download the bundle</a> &middot; the link works for ${days} days`
    : 'No download link this time. Fetch the bundle through the admin API.'

  return `
    <div style="${CARD_STYLE}">
        <div style="${CARD_HEADER_STYLE}">
            ${escapeHtml(report.id)} &middot; app ${escapeHtml(report.appVersion)} &middot; OS ${escapeHtml(report.osVersion)} &middot; ${escapeHtml(report.arch)} &middot; ${escapeHtml(formatBytes(report.sizeBytes))}
            ${envChip(report.buildMode === 'debug' ? 'dev' : 'prod')}
        </div>
        ${noteBlock(report.userNote, 'No note came with this one.')}
        <div style="${CARD_FOOTER_STYLE}">${replyToLine(report.email)}</div>
        <div style="${CARD_FOOTER_STYLE}">${downloadLine}</div>
    </div>`
}

interface ErrorReportNotificationParams {
  report: ErrorReportEmailRow
  to: string
  resendApiKey: string
}

/**
 * Mail one hand-written error report the moment it lands. One report per email: they run about
 * four per 60 days and each is a person waiting for an answer, so there is nothing to batch and
 * nothing to wait for. Auto-sent reports never come through here, they stay on Discord.
 *
 * An attached address becomes the message's `Reply-To`, so answering the reporter is a plain reply.
 * One report per email means there is always exactly one right answer to reply to, unlike the
 * feedback digest, which has to decide.
 */
export async function sendErrorReportNotificationEmail(params: ErrorReportNotificationParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const subject = errorReportSubject(params.report)

  await sendViaResend(
    resend,
    {
      from: 'Cmdr Error Reports <noreply@getcmdr.com>',
      to: params.to,
      subject,
      ...(params.report.email ? { replyTo: params.report.email } : {}),
      html: notificationPage(
        subject,
        renderErrorReportCard(params.report),
        'The Cmdr API server sends this whenever someone writes an error report by hand. Auto-sent reports go to Discord only.',
      ),
    },
    'error report notification',
  )
}

/** One amendment someone added to a report that was already sent. */
export interface ErrorReportAmendmentRow {
  /** The `ERR-XXXXX` of the report this lands on, so the reply can name the same id. */
  id: string
  /** What they added. Null when they only wanted to leave an address. */
  note: string | null
  /** A reply-to address added after the fact. Null when they only wanted to add a note. */
  email: string | null
  /** How many amendments this report now carries, so a third one reads as a thread, not a repeat. */
  amendmentCount: number
}

interface ErrorReportAmendmentParams {
  amendment: ErrorReportAmendmentRow
  to: string
  resendApiKey: string
}

/**
 * Mail one amendment: a note or an address someone added after their bundle was already sent.
 *
 * It goes to the same inbox as the report itself, because an afterthought is worth as much as the
 * first thought and it arrives while the report is still open. The original bundle is not re-linked
 * here: the id is enough to find it (`report:{id}` in KV), and a second presigned link per note
 * would multiply live download URLs for one bundle.
 */
export async function sendErrorReportAmendmentEmail(params: ErrorReportAmendmentParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const { id, note, email, amendmentCount } = params.amendment
  const subject = `Cmdr: someone added to error report ${id}`

  const countLine =
    amendmentCount > 1 ? ` &middot; amendment ${escapeHtml(amendmentCount.toString())} on this report` : ''

  await sendViaResend(
    resend,
    {
      from: 'Cmdr Error Reports <noreply@getcmdr.com>',
      to: params.to,
      subject,
      ...(email ? { replyTo: email } : {}),
      html: notificationPage(
        subject,
        `
    <div style="${CARD_STYLE}">
        <div style="${CARD_HEADER_STYLE}">${escapeHtml(id)}${countLine}</div>
        ${noteBlock(note, 'No note this time, only the address.')}
        <div style="${CARD_FOOTER_STYLE}">${replyToLine(email)}</div>
    </div>`,
        'The Cmdr API server sends this when someone adds to an error report they already sent.',
      ),
    },
    'error report amendment',
  )
}

interface ErrorReportsSuppressedParams {
  /** Reports mailed per UTC day before suppression starts. */
  cap: number
  /** The UTC day (`yyyy-mm-dd`) whose allowance ran out. */
  date: string
  to: string
  resendApiKey: string
}

/**
 * The one line the inbox gets when the day's allowance runs out, so it stops hearing about reports
 * for a reason it can read rather than going quiet.
 */
export async function sendErrorReportsSuppressedEmail(params: ErrorReportsSuppressedParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const subject = `Cmdr: error report emails are suppressed for the rest of ${params.date}`

  await sendViaResend(
    resend,
    {
      from: 'Cmdr Error Reports <noreply@getcmdr.com>',
      to: params.to,
      subject,
      html: notificationPage(
        subject,
        `
    <div style="${CARD_STYLE}">
        <div style="${CARD_PROSE_STYLE}">That's ${escapeHtml(params.cap.toString())} error report emails sent for ${escapeHtml(params.date)}, counting hand-written reports and the amendments people add to them, which is far past the usual rate. The rest of today's stay out of your inbox. Nothing is lost: every bundle is in R2, pinged to Discord, and listed by GET /admin/error-reports, and amendments are in the bundle's sidecar. Emails resume tomorrow.</div>
    </div>`,
        'The Cmdr API server sends this once a day at most.',
      ),
    },
    'error report suppression notice',
  )
}
