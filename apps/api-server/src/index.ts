import { Hono } from 'hono'
import type { Bindings } from './types'
import { licensing } from './licensing/licensing'
import { admin } from './admin/admin'
import { funnel } from './admin/funnel'
import { telemetry } from './telemetry/telemetry'
import { errorReport } from './telemetry/error-report'
import { errorReportAmend } from './telemetry/error-report-amend'
import { feedback } from './telemetry/feedback'
import { likes } from './website/likes'
import { betaSignup } from './website/beta-signup'
import { linkCodes } from './website/link-codes'
import {
  handleCrashNotifications,
  handleFeedbackNotifications,
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  handleSyntheticHeartbeatSweep,
} from './scheduled'
import { postCronFailureNotification } from './discord'
import { pingCronHealth } from './cron-health'

const app = new Hono<{ Bindings: Bindings }>()

// Health check
app.get('/', (c) => {
  return c.json({ status: 'ok', service: 'cmdr-api-server' })
})

// Mount route modules
app.route('/', licensing)
app.route('/', admin)
app.route('/', funnel)
app.route('/', telemetry)
app.route('/', likes)
app.route('/', errorReport)
app.route('/', errorReportAmend)
app.route('/', betaSignup)
app.route('/', feedback)
app.route('/', linkCodes)

export { app }

/**
 * Run one cron job, keeping its failure off the other jobs and onto a channel someone watches.
 *
 * `console.error` alone is where cron failures used to go, and Workers logs weren't retained, so a
 * job could throw every three hours forever with nothing to show for it. The Discord post is the
 * alarm; it deliberately uses `DISCORD_WEBHOOK_URL` (the #error-reports channel) rather than an
 * email, because email is one of the things that can be broken here.
 *
 * Returns whether the job finished, so the caller can report the tick to the dead-man's switch.
 */
async function runCronJob(env: Bindings, job: string, when: string, run: () => Promise<void>): Promise<boolean> {
  try {
    await run()
    return true
  } catch (e) {
    console.error(`${job} failed:`, e)
    if (env.DISCORD_WEBHOOK_URL) {
      // `postCronFailureNotification` swallows its own failures, so a dead webhook costs us the
      // alert and nothing else.
      await postCronFailureNotification(env.DISCORD_WEBHOOK_URL, { job, when, detail: String(e) })
    }
    return false
  }
}

export default {
  fetch: app.fetch.bind(app),
  async scheduled(event: ScheduledEvent, env: Bindings) {
    const when = new Date(event.scheduledTime).toISOString()
    const failedJobs: string[] = []

    const run = async (job: string, handler: () => Promise<void>): Promise<void> => {
      if (!(await runCronJob(env, job, when, handler))) failedJobs.push(job)
    }

    await run('Crash notifications', () => handleCrashNotifications(env))
    await run('Feedback notifications', () => handleFeedbackNotifications(env))

    // Daily jobs: only run on the 00:00 UTC invocation
    if (new Date(event.scheduledTime).getUTCHours() === 0) {
      await run('Daily aggregation', () => handleDailyAggregation(env))
      await run('DB size check', () => handleDbSizeCheck(env))
      await run('Daily eviction sweep', () => handleDailyEvictionSweep(env))
      await run('Retention sweep', () => handleRetentionSweep(env))
      await run('Synthetic heartbeat sweep', () => handleSyntheticHeartbeatSweep(env))
    }

    // Last, so the ping reports the whole tick. A tick that never reaches this line is exactly what
    // the dead-man's switch is for: healthchecks.io alerts on the ping that didn't arrive.
    if (env.HEALTHCHECKS_PING_URL) {
      await pingCronHealth(env.HEALTHCHECKS_PING_URL, failedJobs)
    }
  },
}

// Export handler functions for testing
export {
  handleCrashNotifications,
  handleFeedbackNotifications,
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  handleSyntheticHeartbeatSweep,
}
