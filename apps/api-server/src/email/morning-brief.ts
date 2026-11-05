/**
 * The morning brief: one email a day, around 06:30 in Stockholm, with real numbers. Who is waiting
 * on a reply, which cards nobody has looked at yet, what arrived in the last day, and the tripwires
 * that say a report didn't make it somewhere.
 *
 * It always sends, quiet days included: the point is a number in front of David every morning, and
 * a missing brief is itself the signal that something broke.
 */

import { sendEmail } from './send'
import { CARD_BODY_STYLE, documentShell, escapeHtml, signoffParagraph } from './layout'

/** One card in a brief list. */
export interface BriefCard {
  number: number
  title: string
  url: string
  /** `YYYY-MM-DD`. */
  openedOn: string
}

/** What arrived in the last 24 hours, release builds only. */
export interface BriefArrivals {
  crashes: number
  handWrittenErrors: number
  autoSentErrors: number
  notesAdded: number
  feedback: number
  betaSignups: number
  newsletterSignups: number
  subprocessorUpdatesSignups: number
  opsAlerts: number
}

export interface BriefContent {
  /** Null when the board couldn't be read; the brief says so instead of showing an empty list. */
  needsReply: BriefCard[] | null
  unprocessed: BriefCard[] | null
  arrivals: BriefArrivals
  /** Plain sentences, each one a reason to look closer. Empty on a healthy morning. */
  tripwires: string[]
  /** The one CI line (`ci-brief.ts`), or null when CI health is off. */
  ci: string | null
}

function counted(count: number, singular: string, plural: string): string {
  return `${String(count)} ${count === 1 ? singular : plural}`
}

function errorCount(arrivals: BriefArrivals): number {
  return arrivals.handWrittenErrors + arrivals.autoSentErrors
}

/**
 * `Cmdr morning: 2 waiting on a reply, 1 unprocessed; in the last day 1 crash, 2 errors`. The subject
 * is all a phone notification shows, so it carries the two numbers that need a person, then the
 * day's arrivals when there were any.
 */
export function briefSubject(content: BriefContent): string {
  let waiting: string
  if (content.needsReply === null || content.unprocessed === null) {
    waiting = "couldn't read the board"
  } else {
    const parts: string[] = []
    if (content.needsReply.length) parts.push(`${String(content.needsReply.length)} waiting on a reply`)
    if (content.unprocessed.length) parts.push(`${String(content.unprocessed.length)} unprocessed`)
    waiting = parts.length ? parts.join(', ') : 'nothing waiting'
  }

  const { arrivals } = content
  const day = [
    arrivals.crashes ? counted(arrivals.crashes, 'crash', 'crashes') : null,
    errorCount(arrivals) ? counted(errorCount(arrivals), 'error', 'errors') : null,
    arrivals.feedback ? counted(arrivals.feedback, 'feedback', 'feedback') : null,
  ].filter((part): part is string => part !== null)

  if (day.length) return `Cmdr morning: ${waiting}; in the last day ${day.join(', ')}`
  if (waiting === 'nothing waiting') return 'Cmdr morning: nothing waiting, and a quiet day'
  return `Cmdr morning: ${waiting}`
}

function cardList(title: string, cards: BriefCard[] | null, emptyLine: string): string {
  const heading = `<h3 style="color: #111827; margin: 24px 0 8px;">${escapeHtml(title)}${cards?.length ? ` (${String(cards.length)})` : ''}</h3>`
  if (cards === null) return `${heading}<p>Couldn't read the board this morning; see the tripwires below.</p>`
  if (cards.length === 0) return `${heading}<p style="color: #6b7280;">${escapeHtml(emptyLine)}</p>`
  const items = cards
    .map(
      (card) =>
        `<li><a href="${escapeHtml(card.url)}" style="color: #2563eb;">#${escapeHtml(String(card.number))}</a> ${escapeHtml(card.title)} <span style="color: #6b7280;">(opened ${escapeHtml(card.openedOn)})</span></li>`,
    )
    .join('\n')
  return `${heading}<ul>${items}</ul>`
}

function arrivalsList(arrivals: BriefArrivals): string {
  const errors = errorCount(arrivals)
  const lines = [
    counted(arrivals.crashes, 'crash', 'crashes'),
    `${counted(errors, 'error report', 'error reports')}${errors ? ` (${String(arrivals.handWrittenErrors)} hand-written, ${String(arrivals.autoSentErrors)} auto-sent)` : ''}`,
    counted(arrivals.notesAdded, 'note added to an earlier report', 'notes added to earlier reports'),
    counted(arrivals.feedback, 'feedback message', 'feedback messages'),
    counted(arrivals.betaSignups, 'beta signup', 'beta signups'),
    counted(arrivals.newsletterSignups, 'newsletter signup', 'newsletter signups'),
    counted(arrivals.subprocessorUpdatesSignups, 'subprocessor updates signup', 'subprocessor updates signups'),
    counted(arrivals.opsAlerts, 'ops alert', 'ops alerts'),
  ]
  return `<h3 style="color: #111827; margin: 24px 0 8px;">In the last day</h3><ul>${lines.map((line) => `<li>${escapeHtml(line)}</li>`).join('')}</ul>`
}

function renderBody(subject: string, content: BriefContent): string {
  const tripwires = content.tripwires.length
    ? `<h3 style="color: #b91c1c; margin: 24px 0 8px;">Look closer</h3><ul>${content.tripwires.map((line) => `<li>${escapeHtml(line)}</li>`).join('')}</ul>`
    : ''
  return documentShell(
    `    <h2 style="color: #111827;">${escapeHtml(subject)}</h2>
    ${cardList('Waiting on a reply', content.needsReply, 'Nobody is waiting on an answer.')}
    ${cardList('Not looked at yet', content.unprocessed, 'The triage agent got to every card.')}
    ${arrivalsList(content.arrivals)}
    ${content.ci ? `<p>${escapeHtml(content.ci)}</p>` : ''}
    ${tripwires}

    ${signoffParagraph('The Cmdr API server sends this every morning at 06:30 Stockholm time, after the triage agent has gone through the board.')}`,
    CARD_BODY_STYLE,
  )
}

export async function sendMorningBriefEmail(params: {
  content: BriefContent
  to: string
  smtp2goApiKey: string
}): Promise<void> {
  const subject = briefSubject(params.content)
  await sendEmail(
    params.smtp2goApiKey,
    {
      from: 'Cmdr Morning Brief <noreply@getcmdr.com>',
      to: params.to,
      subject,
      html: renderBody(subject, params.content),
    },
    'morning brief',
  )
}
