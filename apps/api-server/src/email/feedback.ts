/**
 * The in-app feedback digest: every message that hasn't been mailed yet, one card each.
 *
 * Cards rather than the crash digest's table, because feedback is prose: a table column shreds a
 * paragraph into a ribbon.
 */

import { Resend } from 'resend'
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
  type EmailEnv,
} from './layout'

/**
 * One in-app feedback message, as the digest email renders it. Every field here is either written
 * by a person or copied from their machine, so every one of them is escaped at render time.
 */
export interface FeedbackEmailRow {
  /** `created_at`, in the SQLite `datetime('now')` shape (`YYYY-MM-DD HH:MM:SS`, UTC). */
  when: string
  /** Friendly env (`'prod'` for release, `'dev'` for debug, `'?'` for unknown). */
  env: EmailEnv
  /** `app_version`. */
  version: string
  /** `os_version`. */
  osVersion: string
  /** What the person wrote, verbatim. Untrusted text; line breaks are theirs and are preserved. */
  message: string
  /** The reply-to address the sender chose to attach, or `null` when they attached none. */
  email: string | null
}

/** The subject line, which is the whole email for anyone who doesn't open it. */
function feedbackSubject(count: number): string {
  return `Cmdr: ${String(count)} new feedback message${count === 1 ? '' : 's'}`
}

/** One card per message, stacked. */
function renderFeedbackCard(entry: FeedbackEmailRow): string {
  const replyLine = replyToLine(entry.email)

  return `
    <div style="${CARD_STYLE}">
        <div style="${CARD_HEADER_STYLE}">
            ${escapeHtml(entry.when)} UTC &middot; app ${escapeHtml(entry.version)} &middot; OS ${escapeHtml(entry.osVersion)}
            ${envChip(entry.env)}
        </div>
        <div style="${CARD_PROSE_STYLE}">${escapeHtml(entry.message)}</div>
        <div style="${CARD_FOOTER_STYLE}">${replyLine}</div>
    </div>`
}

interface FeedbackNotificationParams {
  entries: FeedbackEmailRow[]
  to: string
  resendApiKey: string
}

/**
 * The feedback digest, newest first.
 *
 * When exactly one message in the batch carries a reply-to address, that address becomes the
 * email's `replyTo`, so answering the person is a plain reply. With none or several there's no
 * single right answer, so the header stays off and the per-card `mailto:` links carry it instead.
 */
export async function sendFeedbackNotificationEmail(params: FeedbackNotificationParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const subject = feedbackSubject(params.entries.length)

  const addresses = params.entries.map((entry) => entry.email).filter((email): email is string => email !== null)
  const soleReplyTo = addresses.length === 1 ? addresses[0] : undefined

  const cards = params.entries.map(renderFeedbackCard).join('\n')

  await sendViaResend(
    resend,
    {
      from: 'Cmdr Feedback <noreply@getcmdr.com>',
      to: params.to,
      subject,
      ...(soleReplyTo ? { replyTo: soleReplyTo } : {}),
      html: notificationPage(
        subject,
        cards,
        'The Cmdr API server sends this digest whenever new in-app feedback arrives.',
      ),
    },
    'feedback notification',
  )
}
