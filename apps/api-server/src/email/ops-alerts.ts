/**
 * Alerts about the service itself rather than about a person: the telemetry DB outgrowing its
 * budget, and a license key showing up on more machines than one person plausibly owns.
 *
 * Nobody outside the team ever receives these, so they say what happened and what to do next
 * without any of the product voice the license mail carries.
 */

import { Resend } from 'resend'
import { sendViaResend } from './send'
import { paddleTransactionUrl } from './sale'
import {
  CELL_STYLE,
  TABLE_STYLE,
  bodyStyle,
  documentShell,
  escapeHtml,
  headCellStyle,
  signoffParagraph,
} from './layout'

interface DbSizeAlertParams {
  sizeMb: number
  /** D1's per-database cap on the account's plan. */
  capMb: number
  /** Where the next alert lands, or null past the last step. */
  nextStepMb: number | null
  /** Null when the size history is too short to project from. */
  growth: { mbPerMonth: number; overDays: number } | null
  tableCounts: Record<string, number>
  to: string
  resendApiKey: string
}

const SPELLED = ['zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine']

function monthsPhrase(months: number): string {
  if (months < 1) return 'less than a month'
  const whole = Math.round(months)
  const count = whole < 10 ? SPELLED[whole] : whole.toLocaleString('en-US')
  return `about ${count} ${whole === 1 ? 'month' : 'months'}`
}

function growthParagraph(params: DbSizeAlertParams): string {
  if (!params.growth) return "<p>There isn't enough size history yet to say how fast it grows.</p>"
  const { mbPerMonth, overDays } = params.growth
  const left = monthsPhrase((params.capMb - params.sizeMb) / mbPerMonth)
  return `<p>It grows <strong>about ${String(Math.round(mbPerMonth))} MB a month</strong> (over the past ${String(overDays)} days), so the cap is ${left} away at that pace.</p>`
}

/**
 * The telemetry D1 passing a step of its plan's size cap, sent once per step: how close it is, how
 * fast it grows, and the per-table row counts that say where the size went.
 */
export async function sendDbSizeAlert(params: DbSizeAlertParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const sizeMb = String(Math.round(params.sizeMb))
  const capMb = String(Math.round(params.capMb))
  const percent = String(Math.round((params.sizeMb / params.capMb) * 100))
  const subject = `Cmdr: telemetry DB is ${sizeMb} MB, ${percent}% of D1's ${capMb} MB cap`
  const next =
    params.nextStepMb === null
      ? 'This is the last step, so no more of these until the cap.'
      : `The next one comes at ${String(Math.round(params.nextStepMb))} MB.`

  const tableRows = Object.entries(params.tableCounts)
    .map(
      ([table, count]) => `
        <tr>
            <td style="${CELL_STYLE} font-family: monospace;">${escapeHtml(table)}</td>
            <td style="${CELL_STYLE} text-align: right;">${String(count)}</td>
        </tr>`,
    )
    .join('\n')

  await sendViaResend(
    resend,
    {
      from: 'Cmdr Crash Alerts <noreply@getcmdr.com>',
      to: params.to,
      subject,
      html: documentShell(
        `    <h2 style="color: #d97706;">${escapeHtml(subject)}</h2>

    <p>The telemetry D1 database has reached <strong>${sizeMb} MB</strong>, ${percent}% of the ${capMb} MB cap on the Workers Free plan. Past the cap, D1 refuses writes, so new telemetry stops landing.</p>

    ${growthParagraph(params)}

    <p>Most of it is <code>heartbeat</code> and <code>analytics_event</code>, kept 730 days for the dashboard's windows. Before the cap, either shorten that retention (and the privacy policy with it) or move to Workers Paid, which raises the cap to 10 GB.</p>

    <table style="${TABLE_STYLE}">
        <thead>
            <tr>
                <th style="${headCellStyle('left')}">Table</th>
                <th style="${headCellStyle('right')}">Row count</th>
            </tr>
        </thead>
        <tbody>
            ${tableRows}
        </tbody>
    </table>

    ${signoffParagraph(`The Cmdr API server sends this once per step of the cap (40%, 60%, 80%, 90%). ${next}`)}`,
        bodyStyle({ maxWidthPx: 600, color: '#333' }),
      ),
    },
    'database size alert',
  )
}

interface DeviceCountAlertParams {
  seatTransactionId: string
  baseTransactionId: string
  deviceCount: number
  customerEmail: string
  resendApiKey: string
  paddleEnvironment: 'sandbox' | 'live'
}

/**
 * One seat showing up on an implausible number of machines. Goes to `legal@`, and the next steps
 * are in the email because the answer is a conversation with the customer, not a lockout.
 */
export async function sendDeviceCountAlert(params: DeviceCountAlertParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const paddleUrl = paddleTransactionUrl(params.baseTransactionId, params.paddleEnvironment)
  const labelCell = `${CELL_STYLE} font-weight: bold;`

  await sendViaResend(
    resend,
    {
      from: 'Cmdr License Alerts <noreply@getcmdr.com>',
      to: 'legal@getcmdr.com',
      subject: `Device count alert: ${params.seatTransactionId} (${String(params.deviceCount)} devices)`,
      html: documentShell(
        `    <h2 style="color: #d97706;">Device count alert</h2>

    <table style="${TABLE_STYLE}">
        <tr>
            <td style="${labelCell}">Seat transaction ID</td>
            <td style="${CELL_STYLE} font-family: monospace;">${escapeHtml(params.seatTransactionId)}</td>
        </tr>
        <tr>
            <td style="${labelCell}">Base transaction</td>
            <td style="${CELL_STYLE} font-family: monospace;">
                <a href="${escapeHtml(paddleUrl)}" style="color: #2563eb;">${escapeHtml(params.baseTransactionId)}</a>
            </td>
        </tr>
        <tr>
            <td style="${labelCell}">Device count</td>
            <td style="${CELL_STYLE}"><strong style="color: #dc2626;">${String(params.deviceCount)}</strong></td>
        </tr>
        <tr>
            <td style="${labelCell}">Customer email</td>
            <td style="${CELL_STYLE}">${escapeHtml(params.customerEmail)}</td>
        </tr>
    </table>

    <h3>Next steps</h3>
    <ol>
        <li>Query Analytics Engine to check the pattern: is device count growing or did it spike once?</li>
        <li>Send a friendly email from <code style="background: #f3f4f6; padding: 2px 4px; border-radius: 3px;">support@getcmdr.com</code> asking if they need additional seats.</li>
        <li>If no response after two weeks, follow up once more.</li>
        <li>If still unresolved, consider suspending the subscription via Paddle (last resort).</li>
    </ol>

    ${signoffParagraph('This alert was generated automatically by the Cmdr API server. Re-alerts are suppressed for 30 days per seat.')}`,
        bodyStyle({ maxWidthPx: 600, color: '#333' }),
      ),
    },
    'device count alert',
  )
}
