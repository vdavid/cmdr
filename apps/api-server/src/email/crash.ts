/**
 * The crash digest: one email per cron tick listing every crash report that hasn't been mailed yet.
 *
 * A table rather than the cards the human-written channels use, because these rows are machine
 * facts read by scanning down a column, not prose.
 */

import { Resend } from 'resend'
import { sendViaResend } from './send'
import {
  CELL_STYLE,
  TABLE_STYLE,
  bodyStyle,
  documentShell,
  escapeHtml,
  headCellStyle,
  signoffParagraph,
} from './layout'

/** The fate column's rendered values. `'?'` is the honest answer, never a guessed `'crashed'`. */
export type CrashFate = 'crashed' | 'kept running' | '?'

/** Text color per fate, in the email's existing language: red for a crash, amber for a survived panic, gray for unknown. */
const fateColors: Record<CrashFate, string> = { crashed: '#dc2626', 'kept running': '#d97706', '?': '#9ca3af' }

/**
 * One row in the crash notification email. The email lists every crash report (no grouping by
 * `top_function`) so each row maps to a single D1 row, with the short id letting the user trace it
 * back.
 */
export interface CrashEmailRow {
  /** `created_at` in ISO 8601. */
  when: string
  /** Friendly env (`'prod'` for release, `'dev'` for debug, `'?'` for unknown). */
  env: 'prod' | 'dev' | '?'
  /**
   * What the app did after the report was written: `'crashed'` (it went down), `'kept running'`
   * (a background panic it survived), or `'?'` for a row whose `app_fate` claims nothing. This is
   * the severity ranking; two rows can otherwise read identically.
   */
  fate: CrashFate
  /** `CRASH-XXXXX`, or `'?'` for rows from older clients. */
  id: string
  /** `top_function`. */
  site: string
  signal: string
  version: string
  /** Contact email the tester voluntarily attached at send time, or `null` if none. */
  email: string | null
  /**
   * `panic_message`: the panic payload, already redacted and capped by the client. `null` for
   * signal crashes (no payload) and for rows written before the column existed.
   */
  message: string | null
}

/**
 * The subject line, which is the whole email for anyone who doesn't open it. A survived panic is
 * a lower-severity thing than a crash, so it is named there rather than only in the table; when
 * nothing survived, the line is the plain count it has always been. Only survivors are counted:
 * a NULL `app_fate` claims nothing, so it is never tallied as a crash.
 */
function crashSubject(totalCount: number, keptRunningCount: number): string {
  const base = `Cmdr: ${String(totalCount)} new crash report${totalCount === 1 ? '' : 's'}`
  if (keptRunningCount === 0) return base
  if (keptRunningCount === totalCount) return `${base}, the app kept running`
  return `${base} (${String(keptRunningCount)} kept running)`
}

/** Two `<tr>`s per report: the fact columns, then the panic payload across the full width. */
function renderCrashRow(entry: CrashEmailRow): string {
  const nowrapCell = `${CELL_STYLE} font-size: 13px; white-space: nowrap;`
  const plainCell = `${CELL_STYLE} font-size: 13px;`
  const monoCell = `${CELL_STYLE} font-family: monospace; font-size: 13px;`

  return `
        <tr>
            <td style="${nowrapCell}">${escapeHtml(entry.when)}</td>
            <td style="${CELL_STYLE} font-size: 13px; text-align: center;">${escapeHtml(entry.env)}</td>
            <td style="${CELL_STYLE} font-size: 13px; white-space: nowrap; color: ${fateColors[entry.fate]};">${escapeHtml(entry.fate)}</td>
            <td style="${monoCell}">${escapeHtml(entry.id)}</td>
            <td style="${monoCell}">${escapeHtml(entry.site)}</td>
            <td style="${plainCell}">${escapeHtml(entry.signal)}</td>
            <td style="${plainCell}">${escapeHtml(entry.version)}</td>
            <td style="${plainCell}">${
              entry.email
                ? `<a href="mailto:${escapeHtml(entry.email)}" style="color: #2563eb;">${escapeHtml(entry.email)}</a>`
                : '<span style="color: #9ca3af;">—</span>'
            }</td>
        </tr>
        <tr>
            <td colspan="8" style="padding: 6px 12px 12px; border: 1px solid #e5e7eb; border-top: 0; font-family: monospace; font-size: 12px; color: #b91c1c; word-break: break-word;">${
              entry.message ? escapeHtml(entry.message) : '<span style="color: #9ca3af; font-family: inherit;">—</span>'
            }</td>
        </tr>`
}

interface CrashNotificationParams {
  crashes: CrashEmailRow[]
  totalCount: number
  to: string
  resendApiKey: string
}

export async function sendCrashNotificationEmail(params: CrashNotificationParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const subject = crashSubject(params.totalCount, params.crashes.filter((c) => c.fate === 'kept running').length)

  const tableRows = params.crashes.map(renderCrashRow).join('\n')
  const columns: [string, 'left' | 'center'][] = [
    ['When', 'left'],
    ['Env', 'center'],
    ['Fate', 'left'],
    ['ID', 'left'],
    ['Site', 'left'],
    ['Signal', 'left'],
    ['Version', 'left'],
    ['Reply to', 'left'],
  ]
  const headerCells = columns
    .map(([label, align]) => `                <th style="${headCellStyle(align)}">${label}</th>`)
    .join('\n')

  await sendViaResend(
    resend,
    {
      from: 'Cmdr Crash Alerts <noreply@getcmdr.com>',
      to: params.to,
      subject,
      html: documentShell(
        `    <h2 style="color: #dc2626;">${escapeHtml(subject)}</h2>

    <table style="${TABLE_STYLE}">
        <thead>
            <tr>
${headerCells}
            </tr>
        </thead>
        <tbody>
            ${tableRows}
        </tbody>
    </table>

    ${signoffParagraph('This alert was generated automatically by the Cmdr API server.')}`,
        bodyStyle({ maxWidthPx: 720, color: '#333' }),
      ),
    },
    'crash notification',
  )
}
