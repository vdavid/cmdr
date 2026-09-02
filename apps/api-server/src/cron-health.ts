/**
 * Dead-man's switch for the cron handler, backed by healthchecks.io.
 *
 * The Discord alert in `index.ts` covers a tick that RAN and threw. It structurally cannot cover a
 * tick that never ran: no code executed, so nothing posted, and silence looks exactly like health.
 * This closes that half. healthchecks.io expects a ping on a schedule and alerts when one doesn't
 * arrive, so a removed cron trigger, a failed deploy, or a Worker dying before the first job all
 * surface within the grace period.
 *
 * It's also a second channel for a tick that DID throw, and an independent one: healthchecks.io
 * sends its alerts through its own infrastructure, not Resend. When the thing that broke is our
 * email path, an alarm that mails through that same path can't tell us. This one can.
 */

/** What the ping's body says when the tick was clean; the dashboard shows it against the check. */
const CLEAN_TICK_BODY = 'All cron jobs finished.'

/**
 * Signal the outcome of one cron tick.
 *
 * `failedJobs` empty means a clean tick, which pings the check's success endpoint. Any entry pings
 * `/fail`, which trips the check immediately rather than waiting out the grace period, and puts the
 * job names in the ping body so the alert email says which one went.
 *
 * Never throws and never rejects: this runs last in the handler, and an alarm that can take down
 * the thing it watches is worse than no alarm.
 */
export async function pingCronHealth(pingUrl: string, failedJobs: string[]): Promise<void> {
  const base = pingUrl.replace(/\/+$/, '')
  const url = failedJobs.length > 0 ? `${base}/fail` : base
  const body = failedJobs.length > 0 ? `These cron jobs threw: ${failedJobs.join(', ')}.` : CLEAN_TICK_BODY

  try {
    const res = await fetch(url, { method: 'POST', body })
    if (!res.ok) {
      console.error(`Cron healthcheck ping was rejected: HTTP ${res.status.toString()}`)
    }
  } catch (e) {
    console.error('Cron healthcheck ping threw:', e)
  }
}
