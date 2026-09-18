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
  /**
   * `image_base`: where the main executable was loaded in the crashed process. Rendered only for a
   * report with no panic message, which is exactly the signal crash whose raw addresses need it:
   * paired with the release binary it feeds `atos -o <binary> -l <imageBase> <frame…>`. `null` for
   * panic reports and for clients older than the field.
   */
  imageBase: string | null
  /**
   * `os_exception`: macOS's own one-line verdict, like
   * `EXC_BAD_ACCESS (SIGSEGV), KERN_INVALID_ADDRESS at 0x10`. It leads the detail cell whenever we
   * have it, because a fault address of `0x10` says more than any other single field in the row.
   */
  osException: string | null
  /**
   * `os_frames` already parsed: the faulting thread's symbolicated frames. Only the top few are
   * rendered; the rest are a click away in the row's D1 record, and a 35-frame stack per report
   * would make the digest unreadable.
   */
  osFrames: string[]
}

/** Frames of macOS's stack shown per report. Enough to name the crash, short enough to scan past. */
const OS_FRAMES_SHOWN = 6

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

/**
 * The full-width detail cell under a report's fact columns.
 *
 * Ordered by what actually tells you what broke. A panic message is the whole story when there is
 * one. Otherwise macOS's exception line leads (a fault address names a null dereference outright),
 * followed by the top of its symbolicated stack, which is the only stack with names on it when the
 * crash is in WebKit or AppKit. The load base comes last and only when nothing better exists: it is
 * a tool for resolving addresses by hand, not a finding. A report with none of these keeps the em
 * dash it always had.
 */
function renderDetailCell(entry: CrashEmailRow): string {
  const lines: string[] = []
  if (entry.message) {
    lines.push(`<span style="color: #b91c1c;">${escapeHtml(entry.message)}</span>`)
  }
  if (entry.osException) {
    lines.push(`<span style="color: #b91c1c;">${escapeHtml(entry.osException)}</span>`)
  }
  for (const frame of entry.osFrames.slice(0, OS_FRAMES_SHOWN)) {
    lines.push(`<span style="color: #4b5563;">&nbsp;&nbsp;${escapeHtml(frame)}</span>`)
  }
  if (entry.osFrames.length > OS_FRAMES_SHOWN) {
    const rest = entry.osFrames.length - OS_FRAMES_SHOWN
    lines.push(`<span style="color: #9ca3af;">&nbsp;&nbsp;+ ${String(rest)} more</span>`)
  }
  if (lines.length === 0 && entry.imageBase) {
    lines.push(`<span style="color: #6b7280;">image base ${escapeHtml(entry.imageBase)}</span>`)
  }
  if (lines.length === 0) return '<span style="color: #9ca3af; font-family: inherit;">—</span>'
  return lines.join('<br>')
}

/** Two `<tr>`s per report: the fact columns, then the detail line across the full width. */
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
            <td colspan="8" style="padding: 6px 12px 12px; border: 1px solid #e5e7eb; border-top: 0; font-family: monospace; font-size: 12px; word-break: break-word;">${renderDetailCell(entry)}</td>
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
