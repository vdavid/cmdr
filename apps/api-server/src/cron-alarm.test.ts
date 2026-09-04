/**
 * The cron handler's alarm wiring: a job that throws has to reach a channel a person actually
 * watches. Before this existed, every cron failure went to `console.error` and nowhere else, and a
 * dead `RESEND_API_KEY` would have dropped crash alerts silently for as long as it took someone to
 * notice the quiet.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import worker from './index'
import { createBaseEnv } from './cron-test-helpers'

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- mock stands in for Resend's send; a precise signature adds no test value
const mockSend = vi.fn<any>(() => Promise.resolve({ id: 'test-email-id' }))
vi.mock('resend', () => ({
  Resend: class {
    emails = { send: mockSend }
  },
}))

/** A D1 stand-in that throws the moment a job touches it, so every job on the tick fails. */
function brokenD1(): D1Database {
  return {
    prepare: () => {
      throw new Error('D1_ERROR: no such table: crash_reports')
    },
  } as unknown as D1Database
}

/** Every outbound POST, so a test can tell the Discord alert from the healthchecks ping. */
let posts: { url: string; body: string }[] = []

beforeEach(() => {
  posts = []
  vi.spyOn(console, 'error').mockImplementation(() => undefined)
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string | URL, init?: RequestInit) => {
      posts.push({ url: String(url), body: typeof init?.body === 'string' ? init.body : '' })
      return Promise.resolve(new Response('OK'))
    }),
  )
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

/** A non-midnight tick, so only the two every-invocation jobs run. */
const threeAm = { scheduledTime: Date.parse('2026-09-02T03:00:00.000Z') } as ScheduledEvent

function discordPosts() {
  return posts.filter((p) => p.url.startsWith('https://discord.test/'))
}

function healthPings() {
  return posts.filter((p) => p.url.startsWith('https://hc-ping.com/'))
}

describe('scheduled alarm wiring', () => {
  it('posts one Discord alert per job that threw, naming the job and the cause', async () => {
    const env = createBaseEnv({
      TELEMETRY_DB: brokenD1(),
      DISCORD_WEBHOOK_URL: 'https://discord.test/hook',
    })

    await worker.scheduled(threeAm, env as never)

    const alerts = discordPosts()
    expect(alerts).toHaveLength(2)
    const content = alerts.map((a) => (JSON.parse(a.body) as { content: string }).content)
    expect(content[0]).toContain('Crash notifications')
    expect(content[0]).toContain('no such table: crash_reports')
    expect(content[0]).toContain('2026-09-02T03:00:00.000Z')
    expect(content[1]).toContain('Feedback notifications')
  })

  it('keeps running the later jobs after an earlier one throws', async () => {
    const env = createBaseEnv({
      TELEMETRY_DB: brokenD1(),
      DISCORD_WEBHOOK_URL: 'https://discord.test/hook',
    })

    await worker.scheduled(threeAm, env as never)

    // The second alert only exists if the second job ran at all.
    expect(discordPosts()).toHaveLength(2)
  })

  it('fails the healthcheck and names the jobs when a job threw', async () => {
    const env = createBaseEnv({
      TELEMETRY_DB: brokenD1(),
      HEALTHCHECKS_PING_URL: 'https://hc-ping.com/abc',
    })

    await worker.scheduled(threeAm, env as never)

    const pings = healthPings()
    expect(pings).toHaveLength(1)
    expect(pings[0]?.url).toBe('https://hc-ping.com/abc/fail')
    expect(pings[0]?.body).toContain('Crash notifications')
  })

  it('pings the healthcheck clean when every job finished', async () => {
    const env = createBaseEnv({
      // No recipient configured, so both jobs return early instead of touching D1.
      CRASH_NOTIFICATION_EMAIL: undefined,
      HEALTHCHECKS_PING_URL: 'https://hc-ping.com/abc',
    })

    await worker.scheduled(threeAm, env as never)

    expect(healthPings()).toEqual([{ url: 'https://hc-ping.com/abc', body: 'All cron jobs finished.' }])
  })

  it('runs the cron unchanged when neither alarm is configured', async () => {
    const env = createBaseEnv({ TELEMETRY_DB: brokenD1() })

    await expect(worker.scheduled(threeAm, env as never)).resolves.toBeUndefined()

    expect(posts).toEqual([])
  })

  it('never lets a broken alarm channel take the cron down', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.reject(new Error('webhook host is down'))),
    )
    const env = createBaseEnv({
      TELEMETRY_DB: brokenD1(),
      DISCORD_WEBHOOK_URL: 'https://discord.test/hook',
      HEALTHCHECKS_PING_URL: 'https://hc-ping.com/abc',
    })

    await expect(worker.scheduled(threeAm, env as never)).resolves.toBeUndefined()
  })
})
