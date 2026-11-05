/**
 * The three-hour digest: zero or one email per cron tick, covering every crash, error report,
 * amendment, and feedback message that arrived since the last one.
 *
 * Its job is "is anything urgent?", so the SUBJECT carries the counts (a phone notification shows
 * nothing else) and the body is there for the details. Where a report waits to be dealt with is the
 * board, so every entry links its card.
 */

import { sendEmail } from './send'
import { renderCrashTable, type CrashEmailRow } from './crash'
import {
  renderAmendmentCard,
  renderErrorReportCard,
  type ErrorReportAmendmentRow,
  type ErrorReportEmailRow,
} from './error-report'
import { renderFeedbackCard, type FeedbackEmailRow } from './feedback'
import {
  CARD_BODY_STYLE,
  CARD_HEADER_STYLE,
  CARD_PROSE_STYLE,
  CARD_STYLE,
  documentShell,
  escapeHtml,
  signoffParagraph,
} from './layout'

/** One operational notice (`ops-events.ts`): a cron job that threw, eviction, a CSP violation, and so on. */
export interface OpsAlertRow {
  when: string
  kind: string
  message: string
}

/** Everything one digest covers. Each list is newest first. */
export interface DigestContent {
  crashes: CrashEmailRow[]
  errorReports: ErrorReportEmailRow[]
  amendments: ErrorReportAmendmentRow[]
  feedback: FeedbackEmailRow[]
  opsAlerts: OpsAlertRow[]
}

/** How many entries the digest holds. Zero means no email at all. */
export function digestSize(content: DigestContent): number {
  return (
    content.crashes.length +
    content.errorReports.length +
    content.amendments.length +
    content.feedback.length +
    content.opsAlerts.length
  )
}

function renderOpsAlert(alert: OpsAlertRow): string {
  return `
    <div style="${CARD_STYLE}">
        <div style="${CARD_HEADER_STYLE}">${escapeHtml(alert.when)} UTC &middot; ${escapeHtml(alert.kind)}</div>
        <div style="${CARD_PROSE_STYLE}">${escapeHtml(alert.message)}</div>
    </div>`
}

function counted(count: number, singular: string, plural: string): string | null {
  if (count === 0) return null
  return `${String(count)} ${count === 1 ? singular : plural}`
}

/**
 * `Cmdr digest: 1 crash, 2 errors, 4 feedback in the past 3h`. Only the kinds that arrived are
 * named. A survived panic is lower severity than a crash, so the mix is named too: the subject is
 * all a phone notification shows.
 */
export function digestSubject(content: DigestContent): string {
  const keptRunning = content.crashes.filter((crash) => crash.fate === 'kept running').length
  let crashes = counted(content.crashes.length, 'crash', 'crashes')
  if (crashes && keptRunning > 0) {
    crashes += keptRunning === content.crashes.length ? ', app kept running' : ` (${String(keptRunning)} kept running)`
  }
  const parts = [
    crashes,
    counted(content.errorReports.length, 'error', 'errors'),
    counted(content.amendments.length, 'note added', 'notes added'),
    counted(content.feedback.length, 'feedback', 'feedback'),
    counted(content.opsAlerts.length, 'ops alert', 'ops alerts'),
  ].filter((part): part is string => part !== null)
  return `Cmdr digest: ${parts.join(', ')} in the past 3h`
}

/**
 * The one address to set as `Reply-To`, so answering is a plain reply. Only when exactly one person
 * in the whole digest left one: with several there's no single right answer, and the per-entry
 * `mailto:` links carry them instead.
 */
function soleReplyTo(content: DigestContent): string | undefined {
  const addresses = new Set(
    [...content.crashes, ...content.errorReports, ...content.amendments, ...content.feedback]
      .map((entry) => entry.email)
      .filter((email): email is string => Boolean(email)),
  )
  return addresses.size === 1 ? [...addresses][0] : undefined
}

function section(title: string, html: string): string {
  return `
    <h3 style="color: #111827; margin: 28px 0 12px;">${escapeHtml(title)}</h3>
    ${html}`
}

function renderBody(subject: string, content: DigestContent): string {
  const sections: string[] = []
  if (content.crashes.length) sections.push(section('Crashes', renderCrashTable(content.crashes)))
  if (content.errorReports.length) {
    sections.push(section('Error reports', content.errorReports.map(renderErrorReportCard).join('\n')))
  }
  if (content.amendments.length) {
    sections.push(section('Notes added to earlier reports', content.amendments.map(renderAmendmentCard).join('\n')))
  }
  if (content.feedback.length) sections.push(section('Feedback', content.feedback.map(renderFeedbackCard).join('\n')))
  if (content.opsAlerts.length) sections.push(section('Ops alerts', content.opsAlerts.map(renderOpsAlert).join('\n')))

  return documentShell(
    `    <h2 style="color: #111827;">${escapeHtml(subject)}</h2>
${sections.join('\n')}

    ${signoffParagraph('The Cmdr API server sends this every three hours when something new arrived. Every entry is also on the reports board.')}`,
    CARD_BODY_STYLE,
  )
}

interface DigestParams {
  content: DigestContent
  to: string
  smtp2goApiKey: string
}

/** Send one digest. The caller skips the call when {@link digestSize} is zero. */
export async function sendReportDigestEmail(params: DigestParams): Promise<void> {
  const subject = digestSubject(params.content)
  const replyTo = soleReplyTo(params.content)

  await sendEmail(
    params.smtp2goApiKey,
    {
      from: 'Cmdr Digest <noreply@getcmdr.com>',
      to: params.to,
      subject,
      ...(replyTo ? { replyTo } : {}),
      html: renderBody(subject, params.content),
    },
    'report digest',
  )
}
