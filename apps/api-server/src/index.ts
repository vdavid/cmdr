import { Hono } from 'hono'
import type { Bindings } from './types'
import { licensing } from './licensing/licensing'
import { admin } from './admin/admin'
import { funnel } from './admin/funnel'
import { telemetry } from './telemetry/telemetry'
import { errorReport } from './telemetry/error-report'
import { feedback } from './telemetry/feedback'
import { likes } from './website/likes'
import { betaSignup } from './website/beta-signup'
import { linkCodes } from './website/link-codes'
import {
  handleCrashNotifications,
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  handleSyntheticHeartbeatSweep,
} from './scheduled'

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
app.route('/', betaSignup)
app.route('/', feedback)
app.route('/', linkCodes)

export { app }

export default {
  fetch: app.fetch.bind(app),
  async scheduled(event: ScheduledEvent, env: Bindings) {
    try {
      await handleCrashNotifications(env)
    } catch (e) {
      console.error('Crash notifications failed:', e)
    }

    // Daily jobs: only run on the 00:00 UTC invocation
    const hour = new Date(event.scheduledTime).getUTCHours()
    if (hour === 0) {
      try {
        await handleDailyAggregation(env)
      } catch (e) {
        console.error('Daily aggregation failed:', e)
      }

      try {
        await handleDbSizeCheck(env)
      } catch (e) {
        console.error('DB size check failed:', e)
      }

      try {
        await handleDailyEvictionSweep(env)
      } catch (e) {
        console.error('Daily eviction sweep failed:', e)
      }

      try {
        await handleRetentionSweep(env)
      } catch (e) {
        console.error('Retention sweep failed:', e)
      }

      try {
        await handleSyntheticHeartbeatSweep(env)
      } catch (e) {
        console.error('Synthetic heartbeat sweep failed:', e)
      }
    }
  },
}

// Export handler functions for testing
export {
  handleCrashNotifications,
  handleDailyAggregation,
  handleDbSizeCheck,
  handleDailyEvictionSweep,
  handleRetentionSweep,
  handleSyntheticHeartbeatSweep,
}
