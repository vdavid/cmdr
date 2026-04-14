import { Hono } from 'hono'
import type { Bindings } from './types'
import { licensing } from './licensing'
import { admin } from './admin'
import { telemetry } from './telemetry'
import { likes } from './likes'
import { handleCrashNotifications, handleDailyAggregation, handleDbSizeCheck } from './scheduled'

const app = new Hono<{ Bindings: Bindings }>()

// Health check
app.get('/', (c) => {
  return c.json({ status: 'ok', service: 'cmdr-api-server' })
})

// Mount route modules
app.route('/', licensing)
app.route('/', admin)
app.route('/', telemetry)
app.route('/', likes)

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
    }
  },
}

// Export handler functions for testing
export { handleCrashNotifications, handleDailyAggregation, handleDbSizeCheck }
