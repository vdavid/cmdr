import { Resend, type CreateEmailOptions } from 'resend'
import type { LicenseType } from './licensing/license'
import { formatBytes, type Bindings } from './types'

/**
 * Where mail a person wrote lands: the in-app feedback digest and hand-written error reports.
 * `FEEDBACK_NOTIFICATION_EMAIL` when it's set, else the crash recipient, so neither channel needs
 * a new secret to ship. `undefined` means nothing is configured and the caller stays quiet.
 */
export function humanReportRecipient(
  env: Pick<Bindings, 'FEEDBACK_NOTIFICATION_EMAIL' | 'CRASH_NOTIFICATION_EMAIL'>,
): string | undefined {
  return env.FEEDBACK_NOTIFICATION_EMAIL ?? env.CRASH_NOTIFICATION_EMAIL
}

/** A card's shared chrome: the rounded border and the white ground the header and footer sit on. */
const CARD_STYLE = 'border: 1px solid #e5e7eb; border-radius: 8px; margin: 0 0 20px; background: #ffffff;'

/** The muted strip at the top of a card, carrying the machine facts. */
const CARD_HEADER_STYLE =
  'padding: 10px 16px; background: #f9fafb; border-bottom: 1px solid #e5e7eb; border-radius: 8px 8px 0 0; font-size: 13px; color: #6b7280;'

/** The strip at the bottom of a card, carrying the follow-up action. */
const CARD_FOOTER_STYLE = 'padding: 10px 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;'

/** Prose a person wrote: a readable measure, and their line breaks kept. */
const CARD_PROSE_STYLE =
  'padding: 16px; max-width: 600px; font-size: 15px; line-height: 1.6; color: #1f2937; white-space: pre-wrap; word-break: break-word;'

/** The `<body>` chrome every notification email shares. */
const BODY_STYLE =
  "font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #1f2937; max-width: 680px; margin: 0 auto; padding: 20px; background: #ffffff;"

/** The closing line under the cards, explaining who sent this and why. */
const SIGNOFF_STYLE =
  'margin-top: 24px; padding-top: 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;'

/**
 * Send through Resend, turning a rejected send into a thrown error.
 *
 * The SDK reports failures in its RESPONSE (`{ data, error }`) rather than throwing, network
 * failures included, so a bare `await resend.emails.send(...)` reads every failure as a success.
 * That is how a license email can vanish while the purchase looks fulfilled.
 */
async function sendViaResend(resend: Resend, payload: CreateEmailOptions, label: string): Promise<void> {
  const { error } = await resend.emails.send(payload)
  if (error) {
    throw new Error(`Resend rejected the ${label} email: ${error.message}`)
  }
}

/** The fate column's rendered values. `'?'` is the honest answer, never a guessed `'crashed'`. */
export type CrashFate = 'crashed' | 'kept running' | '?'

/** Text color per fate, in the email's existing language: red for a crash, amber for a survived panic, gray for unknown. */
const fateColors: Record<CrashFate, string> = { crashed: '#dc2626', 'kept running': '#d97706', '?': '#9ca3af' }

/**
 * One row in the crash notification email. The email lists every crash report (no
 * grouping by `top_function` like the previous incarnation) so each row maps to a
 * single D1 row, with the short id letting the user trace it back.
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

interface CrashNotificationParams {
  crashes: CrashEmailRow[]
  totalCount: number
  to: string
  resendApiKey: string
}

export async function sendCrashNotificationEmail(params: CrashNotificationParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const subject = crashSubject(params.totalCount, params.crashes.filter((c) => c.fate === 'kept running').length)

  const tableRows = params.crashes
    .map(
      (entry) => `
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-size: 13px; white-space: nowrap;">${escapeHtml(entry.when)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-size: 13px; text-align: center;">${escapeHtml(entry.env)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-size: 13px; white-space: nowrap; color: ${fateColors[entry.fate]};">${escapeHtml(entry.fate)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-family: monospace; font-size: 13px;">${escapeHtml(entry.id)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-family: monospace; font-size: 13px;">${escapeHtml(entry.site)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-size: 13px;">${escapeHtml(entry.signal)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-size: 13px;">${escapeHtml(entry.version)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-size: 13px;">${
              entry.email
                ? `<a href="mailto:${escapeHtml(entry.email)}" style="color: #2563eb;">${escapeHtml(entry.email)}</a>`
                : '<span style="color: #9ca3af;">—</span>'
            }</td>
        </tr>
        <tr>
            <td colspan="8" style="padding: 6px 12px 12px; border: 1px solid #e5e7eb; border-top: 0; font-family: monospace; font-size: 12px; color: #b91c1c; word-break: break-word;">${
              entry.message ? escapeHtml(entry.message) : '<span style="color: #9ca3af; font-family: inherit;">—</span>'
            }</td>
        </tr>`,
    )
    .join('\n')

  await sendViaResend(
    resend,
    {
      from: 'Cmdr Crash Alerts <noreply@getcmdr.com>',
      to: params.to,
      subject,
      html: `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 720px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #dc2626;">${escapeHtml(subject)}</h2>

    <table style="border-collapse: collapse; width: 100%; margin: 16px 0;">
        <thead>
            <tr>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">When</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: center; background: #f9fafb;">Env</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Fate</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">ID</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Site</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Signal</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Version</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Reply to</th>
            </tr>
        </thead>
        <tbody>
            ${tableRows}
        </tbody>
    </table>

    <p style="margin-top: 24px; padding-top: 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;">
        This alert was generated automatically by the Cmdr API server.
    </p>
</body>
</html>
        `.trim(),
    },
    'crash notification',
  )
}

/** The friendly build-mode label a card shows, same vocabulary as the crash email's env column. */
export type EmailEnv = 'prod' | 'dev' | '?'

/** Chip colors per env: green for a shipped build, amber for a local one, gray for a row that says nothing. */
const envChipColors: Record<EmailEnv, { background: string; text: string }> = {
  prod: { background: '#ecfdf5', text: '#047857' },
  dev: { background: '#fff7ed', text: '#c2410c' },
  '?': { background: '#f3f4f6', text: '#6b7280' },
}

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

/** The `prod` / `dev` pill that tells shipped traffic from a local build at a glance. */
function envChip(env: EmailEnv): string {
  const chip = envChipColors[env]
  return `<span style="display: inline-block; margin-left: 6px; padding: 1px 8px; border-radius: 10px; font-size: 12px; background: ${chip.background}; color: ${chip.text};">${escapeHtml(env)}</span>`
}

/**
 * The page every card-shaped notification email shares: the subject as a heading, the cards, and
 * one line saying what sent it. Light-only with explicit hex, because a mail client is not a
 * browser and `prefers-color-scheme` support is a coin flip.
 */
function notificationPage(subject: string, cards: string, signoff: string): string {
  return `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
</head>
<body style="${BODY_STYLE}">
    <h2 style="color: #111827;">${escapeHtml(subject)}</h2>

    ${cards}

    <p style="${SIGNOFF_STYLE}">
        ${escapeHtml(signoff)}
    </p>
</body>
</html>
  `.trim()
}

/**
 * One card per message, stacked. Feedback is prose, so it gets a readable measure and its own
 * block rather than a table cell: a table column shreds a paragraph into a ribbon.
 */
function renderFeedbackCard(entry: FeedbackEmailRow): string {
  const replyLine = entry.email
    ? `Reply to <a href="mailto:${escapeHtml(entry.email)}" style="color: #2563eb;">${escapeHtml(entry.email)}</a>`
    : 'No reply-to address'

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
 * The feedback digest: every message that hasn't been mailed yet, newest first.
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

/** The card body: the note, or a plain line saying there wasn't one. */
function renderErrorReportCard(report: ErrorReportEmailRow): string {
  const note = report.userNote?.trim()
  const noteBlock = note
    ? `<div style="${CARD_PROSE_STYLE}">${escapeHtml(note)}</div>`
    : `<div style="${CARD_PROSE_STYLE} color: #6b7280;">No note came with this one.</div>`

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
        ${noteBlock}
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
      html: notificationPage(
        subject,
        renderErrorReportCard(params.report),
        'The Cmdr API server sends this whenever someone writes an error report by hand. Auto-sent reports go to Discord only.',
      ),
    },
    'error report notification',
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
        <div style="${CARD_PROSE_STYLE}">That's ${escapeHtml(params.cap.toString())} hand-written error reports mailed for ${escapeHtml(params.date)}, which is far past the usual rate, so the rest of today's reports stay out of your inbox. Nothing is lost: every bundle is in R2, pinged to Discord, and listed by GET /admin/error-reports. Emails resume tomorrow.</div>
    </div>`,
        'The Cmdr API server sends this once a day at most.',
      ),
    },
    'error report suppression notice',
  )
}

interface DbSizeAlertParams {
  sizeMb: number
  tableCounts: Record<string, number>
  to: string
  resendApiKey: string
}

export async function sendDbSizeAlert(params: DbSizeAlertParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const subject = `Cmdr: telemetry DB is ${String(Math.round(params.sizeMb))} MB`

  const tableRows = Object.entries(params.tableCounts)
    .map(
      ([table, count]) => `
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-family: monospace;">${escapeHtml(table)}</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: right;">${String(count)}</td>
        </tr>`,
    )
    .join('\n')

  await sendViaResend(
    resend,
    {
      from: 'Cmdr Crash Alerts <noreply@getcmdr.com>',
      to: params.to,
      subject,
      html: `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #d97706;">${escapeHtml(subject)}</h2>

    <p>The telemetry D1 database has reached <strong>${String(Math.round(params.sizeMb))} MB</strong>. Consider reviewing and pruning old data.</p>

    <table style="border-collapse: collapse; width: 100%; margin: 16px 0;">
        <thead>
            <tr>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: left; background: #f9fafb;">Table</th>
                <th style="padding: 8px 12px; border: 1px solid #e5e7eb; text-align: right; background: #f9fafb;">Row count</th>
            </tr>
        </thead>
        <tbody>
            ${tableRows}
        </tbody>
    </table>

    <p style="margin-top: 24px; padding-top: 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;">
        This alert was generated automatically by the Cmdr API server. Threshold: 100 MB.
    </p>
</body>
</html>
        `.trim(),
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

export async function sendDeviceCountAlert(params: DeviceCountAlertParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const paddleDomain = params.paddleEnvironment === 'sandbox' ? 'sandbox-vendors.paddle.com' : 'vendors.paddle.com'
  const paddleUrl = `https://${paddleDomain}/transactions-v2/${params.baseTransactionId}`

  await sendViaResend(
    resend,
    {
      from: 'Cmdr License Alerts <noreply@getcmdr.com>',
      to: 'legal@getcmdr.com',
      subject: `Device count alert: ${params.seatTransactionId} (${String(params.deviceCount)} devices)`,
      html: `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #d97706;">Device count alert</h2>

    <table style="border-collapse: collapse; width: 100%; margin: 16px 0;">
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-weight: bold;">Seat transaction ID</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-family: monospace;">${escapeHtml(params.seatTransactionId)}</td>
        </tr>
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-weight: bold;">Base transaction</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-family: monospace;">
                <a href="${escapeHtml(paddleUrl)}" style="color: #2563eb;">${escapeHtml(params.baseTransactionId)}</a>
            </td>
        </tr>
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-weight: bold;">Device count</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb;"><strong style="color: #dc2626;">${String(params.deviceCount)}</strong></td>
        </tr>
        <tr>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb; font-weight: bold;">Customer email</td>
            <td style="padding: 8px 12px; border: 1px solid #e5e7eb;">${escapeHtml(params.customerEmail)}</td>
        </tr>
    </table>

    <h3>Next steps</h3>
    <ol>
        <li>Query Analytics Engine to check the pattern: is device count growing or did it spike once?</li>
        <li>Send a friendly email from <code style="background: #f3f4f6; padding: 2px 4px; border-radius: 3px;">support@getcmdr.com</code> asking if they need additional seats.</li>
        <li>If no response after two weeks, follow up once more.</li>
        <li>If still unresolved, consider suspending the subscription via Paddle (last resort).</li>
    </ol>

    <p style="margin-top: 24px; padding-top: 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;">
        This alert was generated automatically by the Cmdr API server. Re-alerts are suppressed for 30 days per seat.
    </p>
</body>
</html>
        `.trim(),
    },
    'device count alert',
  )
}

const htmlEscapeMap: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }

function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (char) => htmlEscapeMap[char])
}

interface EmailParams {
  to: string
  customerName: string
  licenseKeys: string[]
  productName: string
  supportEmail: string
  resendApiKey: string
  organizationName?: string
  licenseType?: LicenseType
}

function getLicenseDescription(type: LicenseType | undefined, orgName?: string): string {
  switch (type) {
    case 'commercial_subscription':
      return orgName
        ? `Your commercial license for ${orgName} is valid for one year and will auto-renew.`
        : 'Your commercial license is valid for one year and will auto-renew.'
    case 'commercial_perpetual':
      return orgName
        ? `Your perpetual commercial license for ${orgName} is valid forever.`
        : 'Your perpetual commercial license is valid forever.'
    default:
      return 'This is an unknown license type. This is weird. Please contact support.'
  }
}

export async function sendLicenseEmail(params: EmailParams): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  const escapedCustomerName = escapeHtml(params.customerName)
  const escapedOrgName = params.organizationName ? escapeHtml(params.organizationName) : undefined
  const licenseDescriptionHtml = getLicenseDescription(params.licenseType, escapedOrgName)
  const licenseDescriptionText = getLicenseDescription(params.licenseType, params.organizationName)
  const orgLine = escapedOrgName ? `<p><strong>Licensed to:</strong> ${escapedOrgName}</p>` : ''
  const orgLineText = params.organizationName ? `Licensed to: ${params.organizationName}\n` : ''

  const count = params.licenseKeys.length
  const isMultiple = count > 1
  const keyWord = isMultiple ? 'keys' : 'key'
  const subject = `Your ${params.productName} license ${keyWord} 🎉`

  // HTML: render keys as numbered boxes if multiple, single box otherwise
  const licenseBoxesHtml = isMultiple
    ? params.licenseKeys
        .map(
          (key, i) => `
            <div class="license-box">
                <div class="license-number">License ${String(i + 1)} of ${String(count)}</div>
                ${key}
            </div>`,
        )
        .join('\n')
    : `<div class="license-box">${params.licenseKeys[0]}</div>`

  // Plain text: render keys with headers if multiple
  const licenseKeysText = isMultiple
    ? params.licenseKeys.map((key, i) => `License ${String(i + 1)} of ${String(count)}:\n${key}`).join('\n\n')
    : params.licenseKeys[0]

  const introText = isMultiple
    ? `Thanks for purchasing ${String(count)} licenses for ${params.productName}! Here are your license keys:`
    : `Thanks for purchasing ${params.productName}! Here's your license key:`

  await sendViaResend(
    resend,
    {
      from: `${params.productName} <noreply@getcmdr.com>`,
      to: params.to,
      subject,
      html: `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px; }
        .license-box { background: #f5f5f5; border-radius: 8px; padding: 20px; margin: 20px 0; font-family: monospace; font-size: 18px; text-align: center; letter-spacing: 2px; }
        .license-number { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 12px; color: #666; margin-bottom: 8px; letter-spacing: normal; }
        .footer { margin-top: 40px; padding-top: 20px; border-top: 1px solid #eee; font-size: 14px; color: #666; }
        .note { background: #e8f4f8; border-left: 4px solid #0ea5e9; padding: 12px 16px; margin: 20px 0; }
    </style>
</head>
<body>
    <h1>Welcome to ${params.productName}! 🚀</h1>

    <p>Hey ${escapedCustomerName},</p>

    <p>${introText}</p>

    ${licenseBoxesHtml}

    ${orgLine}

    <h3>How to activate:</h3>
    <ol>
        <li>Open ${params.productName}</li>
        <li>Go to <strong>Cmdr menu → Enter license key...</strong></li>
        <li>Paste a key and click Activate</li>
    </ol>

    <p>${licenseDescriptionHtml}</p>

    <div class="note">
        <strong>Multiple machines?</strong> Each license lets you run ${params.productName} on multiple machines (like a laptop and desktop for remote debugging) as long as you're the only one using that license.
    </div>

    <div class="footer">
        <p>Questions? Just reply to this email or contact <a href="mailto:${params.supportEmail}">${params.supportEmail}</a></p>
        <p>Happy file managing! ⌘</p>
    </div>
</body>
</html>
        `.trim(),
      text: `
Welcome to ${params.productName}!

Hey ${params.customerName},

${introText}

${licenseKeysText}

${orgLineText}
How to activate:
1. Open ${params.productName}
2. Go to Cmdr menu → Enter license key...
3. Paste a key and click Activate

${licenseDescriptionText}

Multiple machines? Each license lets you run ${params.productName} on multiple machines (like a laptop and desktop for remote debugging) as long as you're the one using that license.

Questions? Contact ${params.supportEmail}

Happy file managing! ⌘
        `.trim(),
    },
    'license',
  )
}
