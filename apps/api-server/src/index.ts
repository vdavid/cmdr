import { Hono } from 'hono'
import type { Bindings } from './types'
import { licensing } from './licensing/licensing'
import { admin } from './admin/admin'
import { funnel } from './admin/funnel'
import { funnelCountries } from './admin/funnel-countries'
import { featureUsage } from './admin/feature-usage'
import { telemetry } from './telemetry/telemetry'
import { heartbeat } from './telemetry/heartbeat'
import { errorReport } from './telemetry/error-report'
import { errorReportAmend } from './telemetry/error-report-amend'
import { feedback } from './telemetry/feedback'
import { likes } from './website/likes'
import { betaSignup } from './website/beta-signup'
import { newsletterSignup } from './website/newsletter-signup'
import { subprocessorUpdatesSignup } from './website/subprocessor-updates-signup'
import { linkCodes } from './website/link-codes'
import { cspReport } from './website/csp-report'
import { s3Prices } from './s3-prices/s3-prices'
import { webhookGitHub } from './webhook-github'
import {
  handleEmailPathProbe,
  handleLicenseBackup,
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  handlePersonalCommentSweep,
  handleSyntheticHeartbeatSweep,
} from './scheduled'
import { cronFailureMessage, recordOpsEvent } from './ops-events'
import { pingCronHealth } from './cron-health'
import { EVERY_TICK_ALERT_AFTER, clearCronStreak, recordCronFailure } from './cron-streaks'
import { fileCrashCards, fileErrorReportCards, fileFeedbackCards, postAmendmentsToBoard } from './report-outbox'
import { handleReportDigest } from './report-digest'
import { applyCardRatings } from './report-ratings'
import { MORNING_CRON, handleMorningBrief, isMorningBriefTick } from './morning-brief'
import { handleCiHealth } from './ci-health'

const app = new Hono<{ Bindings: Bindings }>()

// Health check
app.get('/', (c) => {
  return c.json({ status: 'ok', service: 'cmdr-api-server' })
})

// Mount route modules
app.route('/', licensing)
app.route('/', admin)
app.route('/', funnel)
app.route('/', funnelCountries)
app.route('/', featureUsage)
app.route('/', telemetry)
app.route('/', heartbeat)
app.route('/', likes)
app.route('/', errorReport)
app.route('/', errorReportAmend)
app.route('/', betaSignup)
app.route('/', newsletterSignup)
app.route('/', subprocessorUpdatesSignup)
app.route('/', feedback)
app.route('/', linkCodes)
app.route('/', cspReport)
app.route('/', s3Prices)
app.route('/', webhookGitHub)

export { app }

/**
 * Run one cron job, keeping its failure off the other jobs and onto a channel someone watches.
 *
 * `console.error` alone is where cron failures used to go, and Workers logs weren't retained, so a
 * job could throw every three hours forever with nothing to show for it. Two channels carry the
 * alarm now, and they fail independently: the caller reports the failed job to healthchecks.io,
 * which emails through its OWN infrastructure (so a dead SMTP2Go key or a broken D1 still alerts),
 * and an ops event puts the cause in the next report digest.
 *
 * A job with grace (`alertAfter` > 1) alerts only once it has thrown on that many ticks in a row
 * (`cron-streaks.ts`); until then its failure is `held-back`, logged but kept off both channels.
 *
 * Returns the outcome, so the caller can report the tick to the dead-man's switch.
 */
async function runCronJob(
  env: Bindings,
  job: string,
  when: string,
  alertAfter: number,
  run: () => Promise<void>,
): Promise<'finished' | 'alerted' | 'held-back'> {
  try {
    await run()
  } catch (e) {
    console.error(`${job} failed:`, e)
    const streak = await recordCronFailure(env.TELEMETRY_DB, job, when, alertAfter)
    if (!streak.alert) return 'held-back'
    // Never throws, so a broken D1 costs this half of the alarm and nothing else.
    await recordOpsEvent(
      env.TELEMETRY_DB,
      'cron-failure',
      cronFailureMessage({ job, when, detail: String(e), ...streak }),
    )
    return 'alerted'
  }
  if (alertAfter > 1) await clearCronStreak(env.TELEMETRY_DB, job)
  return 'finished'
}

export default {
  fetch: app.fetch.bind(app),
  async scheduled(event: ScheduledEvent, env: Bindings) {
    const when = new Date(event.scheduledTime).toISOString()
    const failedJobs: string[] = []
    const heldBack: string[] = []

    /**
     * The every-tick jobs get a tick of grace: their next run is three hours away and redoes their
     * work. The daily jobs and the morning brief alert on the first failure, since a second would
     * be a day later.
     */
    const run = async (job: string, handler: () => Promise<void>, alertAfter = 1): Promise<void> => {
      const outcome = await runCronJob(env, job, when, alertAfter, handler)
      if (outcome === 'alerted') failedJobs.push(job)
      else if (outcome === 'held-back') heldBack.push(job)
    }
    const runEveryTick = (job: string, handler: () => Promise<void>) => run(job, handler, EVERY_TICK_ALERT_AFTER)

    // The morning trigger runs the brief and nothing else. Its healthcheck report is failure-only:
    // the check's period is the three-hour tick, and a success ping here would only blur that.
    if (event.cron === MORNING_CRON) {
      if (!isMorningBriefTick(new Date(event.scheduledTime))) return
      await run('Morning brief', () => handleMorningBrief(env, new Date(event.scheduledTime)))
      if (env.HEALTHCHECKS_PING_URL && failedJobs.length) await pingCronHealth(env.HEALTHCHECKS_PING_URL, failedJobs)
      return
    }

    // Cards first, so the emails below can link them. Amendments last: they comment on the cards
    // the report jobs just filed.
    await runEveryTick('Crash cards', () => fileCrashCards(env))
    await runEveryTick('Error report cards', () => fileErrorReportCards(env))
    await runEveryTick('Feedback cards', () => fileFeedbackCards(env))
    await runEveryTick('Amendments on the board', () => postAmendmentsToBoard(env))
    // Before the digest, so a rating that failed on this tick is in it.
    await runEveryTick('Card ratings', () => applyCardRatings(env))
    // Also before the digest, so a CI change seen on this tick is in it. Never throws on GitHub.
    await runEveryTick('CI health', () => handleCiHealth(env))
    await runEveryTick('Report digest', () => handleReportDigest(env, new Date(event.scheduledTime)))

    // Daily jobs: only run on the 00:00 UTC invocation
    if (new Date(event.scheduledTime).getUTCHours() === 0) {
      await run('License backup', () => handleLicenseBackup(env))
      await run('Daily aggregation', () => handleDailyAggregation(env))
      await run('DB size check', () => handleDbSizeCheck(env))
      await run('Daily eviction sweep', () => handleDailyEvictionSweep(env))
      await run('Retention sweep', () => handleRetentionSweep(env))
      await run('Personal comment sweep', () => handlePersonalCommentSweep(env))
      await run('Synthetic heartbeat sweep', () => handleSyntheticHeartbeatSweep(env))
      await run('Email path probe', () => handleEmailPathProbe(env))
    }

    // Last, so the ping reports the whole tick. A tick that never reaches this line is exactly what
    // the dead-man's switch is for: healthchecks.io alerts on the ping that didn't arrive.
    if (env.HEALTHCHECKS_PING_URL) {
      await pingCronHealth(env.HEALTHCHECKS_PING_URL, failedJobs, { heldBack })
    }
  },
}

// Export handler functions for testing
export {
  handleEmailPathProbe,
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  handlePersonalCommentSweep,
  handleSyntheticHeartbeatSweep,
}
