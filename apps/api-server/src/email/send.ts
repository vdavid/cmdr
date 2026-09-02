/**
 * The one door out to Resend, plus the daily probe that proves the door still opens.
 *
 * Every sender in `src/email/` goes through {@link sendViaResend}. Nothing calls
 * `resend.emails.send` directly.
 */

import { Resend, type CreateEmailOptions } from 'resend'
import type { Bindings } from '../types'

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

/**
 * Send through Resend, turning a rejected send into a thrown error.
 *
 * The SDK reports failures in its RESPONSE (`{ data, error }`) rather than throwing, network
 * failures included, so a bare `await resend.emails.send(...)` reads every failure as a success.
 * That is how a license email can vanish while the purchase looks fulfilled.
 */
export async function sendViaResend(resend: Resend, payload: CreateEmailOptions, label: string): Promise<void> {
  const { error } = await resend.emails.send(payload)
  if (error) {
    throw new Error(`Resend rejected the ${label} email: ${error.message}`)
  }
}

/**
 * Resend's simulator address. It accepts the message and marks it delivered without sending it to
 * a person, so the daily probe below costs an inbox nothing.
 */
export const EMAIL_PROBE_RECIPIENT = 'delivered@resend.dev'

/**
 * Prove the Resend send path still works, without waiting for something that matters to need it.
 *
 * Real sends are sporadic (a handful a month), so a rotated or revoked key would otherwise stay
 * invisible until a crash alert, a feedback digest, or a buyer's license key hit the dead
 * credential. This is a REAL send through the real key: the key is scoped to sending only, so
 * every read endpoint (`/domains`, `/api-keys`, `/emails`) returns 401 no matter how healthy the
 * key is, which makes them useless as probes (verified against the live key, 2026-09-02). Widening
 * the key's scope to make them work would test the wrong capability AND hand a leaked key the
 * power to delete our sending domain.
 *
 * Throws through `sendViaResend` when Resend rejects, which is what the cron alarm turns into a
 * Discord message.
 */
export async function sendEmailPathProbe(params: { resendApiKey: string }): Promise<void> {
  const resend = new Resend(params.resendApiKey)
  await sendViaResend(
    resend,
    {
      from: 'Cmdr <noreply@getcmdr.com>',
      to: EMAIL_PROBE_RECIPIENT,
      subject: 'Cmdr email path probe',
      text: 'Automated daily check that the Resend send path still works. Nobody receives this.',
    },
    'email path probe',
  )
}
