/**
 * The one door out to SMTP2Go, plus the daily probe that proves the door still opens.
 *
 * Every sender in `src/email/` goes through {@link sendEmail}. Nothing else calls SMTP2Go.
 */

import type { Bindings } from '../types'

/**
 * Where the report digest lands (every crash, error report, amendment, and feedback message).
 * `FEEDBACK_NOTIFICATION_EMAIL` when it's set, else the crash recipient, so the digest needs
 * a new secret to ship. `undefined` means nothing is configured and the caller stays quiet.
 */
export function humanReportRecipient(
  env: Pick<Bindings, 'FEEDBACK_NOTIFICATION_EMAIL' | 'CRASH_NOTIFICATION_EMAIL'>,
): string | undefined {
  return env.FEEDBACK_NOTIFICATION_EMAIL ?? env.CRASH_NOTIFICATION_EMAIL
}

/**
 * SMTP2Go's EU region, so mail is processed in the EU. The regionless host may route anywhere.
 * Endpoints: https://developers.smtp2go.com/docs/endpoints.md
 */
export const SMTP2GO_API_BASE = 'https://eu-api.smtp2go.com/v3'

/** One email, in our own words. `sendEmail` maps it onto SMTP2Go's fields. */
export interface EmailMessage {
  /** `Name <address>`, always on our verified `getcmdr.com` domain. */
  from: string
  to: string
  subject: string
  html: string
  /** A plain-text alternative, for the mails whose reader must copy something out. */
  text?: string
  replyTo?: string
}

/** The `data` object SMTP2Go wraps every answer in, success or not. */
type Smtp2GoData = Record<string, unknown>

/**
 * POST to one SMTP2Go endpoint and return its `data`, or throw.
 *
 * SMTP2Go puts an error in the body as `data.error` / `data.error_code`, sometimes on a 200, so a
 * 2xx alone proves nothing. The thrown message names what we were doing (`what`) and SMTP2Go's
 * own words, and ❌ never the key: the message lands in ops events, logs, and the digest.
 */
async function callSmtp2Go(apiKey: string, path: string, body: unknown, what: string): Promise<Smtp2GoData> {
  let response: Response
  try {
    response = await fetch(`${SMTP2GO_API_BASE}${path}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Smtp2go-Api-Key': apiKey },
      body: JSON.stringify(body),
    })
  } catch (error) {
    throw new Error(
      `SMTP2Go couldn't be reached for ${what}: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    )
  }

  const raw = await response.text()
  let data: Smtp2GoData | undefined
  try {
    const parsed = JSON.parse(raw) as { data?: unknown }
    if (parsed.data && typeof parsed.data === 'object') data = parsed.data as Smtp2GoData
  } catch {
    // Not JSON (a proxy's error page, say). Judged below like any body without `data`.
  }

  const reason = data ? describeError(data) : undefined
  if (!response.ok || reason !== undefined || !data) {
    const detail = redact(reason ?? 'no readable answer', apiKey)
    throw new Error(`SMTP2Go refused ${what} (HTTP ${String(response.status)}): ${detail}`)
  }
  return data
}

function describeError(data: Smtp2GoData): string | undefined {
  const message = typeof data.error === 'string' ? data.error : undefined
  const code = typeof data.error_code === 'string' ? data.error_code : undefined
  if (message && code) return `${message} (${code})`
  return message ?? code
}

/** Belt and braces: SMTP2Go has no reason to echo the key, but an error string is not ours to trust. */
function redact(text: string, apiKey: string): string {
  return apiKey ? text.split(apiKey).join('[key]') : text
}

/**
 * Send one email through SMTP2Go, turning every failed send into a thrown error.
 *
 * SMTP2Go answers 200 OK even when the send failed, putting the verdict in `data.succeeded` /
 * `data.failed` / `data.failures`, so a caller that only checks the status reads a failure as a
 * success. That is how a license email can vanish while the purchase looks fulfilled: the buyer
 * pays and gets nothing. Hence one door, and a throw for a network failure, a non-2xx, an error in
 * the body, and anything short of exactly one success. `fastaccept: false` keeps the verdict in the
 * answer; with it on, SMTP2Go accepts first and judges later, where nothing here would see it.
 */
export async function sendEmail(apiKey: string, message: EmailMessage, label: string): Promise<void> {
  const what = `the ${label} email`
  const data = await callSmtp2Go(
    apiKey,
    '/email/send',
    {
      sender: message.from,
      to: [message.to],
      subject: message.subject,
      html_body: message.html,
      ...(message.text !== undefined ? { text_body: message.text } : {}),
      ...(message.replyTo ? { custom_headers: [{ header: 'Reply-To', value: message.replyTo }] } : {}),
      fastaccept: false,
    },
    what,
  )
  const failed = typeof data.failed === 'number' ? data.failed : 0
  if (data.succeeded !== 1 || failed > 0) {
    const failures = Array.isArray(data.failures) ? data.failures.map(String).join('; ') : ''
    const detail = redact(failures || `succeeded ${String(data.succeeded)}, failed ${String(failed)}`, apiKey)
    throw new Error(`SMTP2Go didn't send ${what}: ${detail}`)
  }
}

/**
 * Below this share of the cycle's quota left, the probe alarms. The getcmdr.com account's 1,000
 * emails a cycle are shared with mailcow and Listmonk, so a newsletter can eat the room a buyer's
 * license email needs.
 */
export const EMAIL_QUOTA_ALARM_FRACTION = 0.15

/**
 * Prove the SMTP2Go path is alive, without waiting for something that matters to need it.
 *
 * Real sends are sporadic (a handful a month), so a revoked key or a spent quota would otherwise
 * stay invisible until a report digest or a buyer's license key hit it. SMTP2Go has no simulator
 * address and every real send costs quota, so this reads `/stats/email_cycle` with the same key
 * instead: a dead or revoked key throws, and so does a quota below
 * {@link EMAIL_QUOTA_ALARM_FRACTION}. The Worker's key is allowed that endpoint and `/email/send`
 * only, so this exercises the real credential without widening it.
 *
 * Known gap: it proves the key is live and the quota has room, not that a send would succeed.
 * SMTP2Go can block sending on its own (a suspended account, an unverified sender domain) while
 * stats still answer. Accepted, since the alternative costs a real email a day.
 *
 * Throws, which is what raises the cron alarm (healthchecks.io plus an ops event).
 */
export async function checkEmailPath(apiKey: string): Promise<void> {
  const data = await callSmtp2Go(apiKey, '/stats/email_cycle', {}, 'the email path probe')
  const { cycle_max: max, cycle_remaining: remaining, cycle_end: end } = data
  if (typeof max !== 'number' || typeof remaining !== 'number') {
    throw new Error("SMTP2Go's email cycle answer has no quota numbers, so the probe can't judge the room left")
  }
  if (remaining < max * EMAIL_QUOTA_ALARM_FRACTION) {
    const fmt = (n: number): string => n.toLocaleString('en-US')
    const until = typeof end === 'string' ? ` until the cycle ends (${end})` : ''
    throw new Error(
      `Only ${fmt(remaining)} of ${fmt(max)} SMTP2Go emails remain this cycle${until}, so license emails are at risk once it runs out.`,
    )
  }
}
