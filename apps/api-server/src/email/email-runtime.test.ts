import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest'
import { createTestHarness } from 'wrangler'
import { createSmtp2GoStub, type WireEmail } from './smtp2go-test-helpers'

/**
 * Every sender in `src/email/` leaving the REAL Worker: wrangler builds `src/index.ts` and runs it
 * in workerd under the production `wrangler.toml`, so a Node API anywhere between a route (or a
 * cron job) and the SMTP2Go call throws here exactly as it throws in production.
 *
 * `webhook-runtime.test.ts` already covers `sendLicenseEmail` over the purchase path; this file is
 * the rest. The reason they need a runtime lane at all, when `eslint.config.js` already
 * bans Node globals in our own source, is the layer lint structurally can't see: a DEPENDENCY
 * reaching for `Buffer` or `process` on a path we exercise. The senders and the layout helpers are
 * only proven safe by being run.
 *
 * ❌ Never give this harness its own compatibility settings, and never reach for `nodejs_compat`.
 *
 * **SMTP2Go and Paddle are stubbed at the socket, not in the source.** The harness points the
 * Worker's global `fetch` at this process, so replacing `globalThis.fetch` here intercepts every
 * outbound request with the route, the sender, and the runtime untouched. See
 * `../../DETAILS.md` § Test runtimes for the two gotchas in reading an intercepted body.
 *
 * `GITHUB_ISSUES_TOKEN` is deliberately left unset: the board jobs share these ticks, and
 * leaving them off keeps the assertion "what reached the wire is the email" exact.
 */

const crashRecipient = 'ops@example.com'
const paddleApiKey = 'test-paddle-key'

const server = createTestHarness({
  workers: [
    {
      configPath: new URL('../../wrangler.toml', import.meta.url),
      secrets: {
        SMTP2GO_API_KEY: 'test-smtp2go-key',
        CRASH_NOTIFICATION_EMAIL: crashRecipient,
        PADDLE_API_KEY_LIVE: paddleApiKey,
      },
      // A small cap, so the size alert's ballast stays at 200 MB rather than the plan's gigabytes.
      vars: { D1_MAX_DB_MB: 400 },
    },
  ],
})

interface HarnessEnv {
  TELEMETRY_DB: D1Database
  ERROR_REPORT_META: KVNamespace
  LICENSE_CODES: KVNamespace
}

/** What left the Worker for SMTP2Go, read off the wire. */
const smtp2go = createSmtp2GoStub()

/** The Paddle transaction `/validate` resolves, and the customer the device alert looks up. */
const paddleTransaction = { id: 'txn_devicealert', customer_id: 'ctm_01hv8x', custom_data: null }
const paddleCustomer = { email: 'fleet@example.com', name: 'Robin' }

/** Any host we didn't stub. Asserted empty after every test: the Worker talks to two services. */
const unexpectedOutbound: string[] = []

const realFetch = globalThis.fetch

beforeAll(async () => {
  globalThis.fetch = stubbedFetch
  await server.listen()
  await server.getWorker<HarnessEnv>().applyD1Migrations('TELEMETRY_DB')
}, 60_000)

afterAll(async () => {
  globalThis.fetch = realFetch
  await server.close()
})

beforeEach(() => {
  smtp2go.reset()
})

afterEach(() => {
  expect(unexpectedOutbound).toEqual([])
})

async function stubbedFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url
  if (smtp2go.handles(url)) return await smtp2go.answer(url, input, init)
  if (url.startsWith('https://api.paddle.com/transactions/')) {
    return Response.json({ data: paddleTransaction })
  }
  if (url.startsWith('https://api.paddle.com/customers/')) {
    return Response.json({ data: paddleCustomer })
  }
  if (url.startsWith('https://api.github.com/repos/getcmdr/cmdr/')) return greenGitHub(url)
  unexpectedOutbound.push(url)
  return Response.json({ error: 'This host is not stubbed' }, { status: 502 })
}

/**
 * The public CI repo `ci-health.ts` reads on every tick (`CI_HEALTH_REPO`), all green, so the job
 * runs in workerd on each tick here and adds nothing to the digest the tests assert on.
 */
function greenGitHub(url: string): Response {
  const head = 'e81d02881c00239b4df238425ec89addb3ca0888'
  const now = new Date().toISOString()
  if (url.includes('/commits/main')) return Response.json({ sha: head, commit: { committer: { date: now } } })
  const workflow = /\/actions\/workflows\/([^/]+)\/runs/.exec(url)?.[1]
  if (!workflow) return Response.json({ message: 'Not Found' }, { status: 404 })
  return Response.json({
    workflow_runs: [
      {
        id: 1,
        html_url: 'https://github.com/getcmdr/cmdr/actions/runs/1',
        head_sha: head,
        event: new URL(url).searchParams.get('event') ?? 'schedule',
        status: 'completed',
        conclusion: 'success',
        created_at: now,
      },
    ],
  })
}

/** The runtime's own logs, for a failure message: a throw inside workerd leaves nothing in the body. */
function workerLogs(): string {
  return server
    .getLogs()
    .map((log) => JSON.stringify(log))
    .join('\n')
}

/**
 * Wait for the mail that rides behind a 200 (`waitUntil`) or behind a cron job.
 *
 * A send that never reaches the wire IS the failure this file exists to catch, and both the route
 * and `runCronJob` swallow it, so the only trace is in the runtime's own logs. They're carried into
 * the assertion message rather than left for someone to go find.
 */
async function awaitEmails(count = 1): Promise<WireEmail[]> {
  try {
    await expect.poll(() => smtp2go.sent.length).toBeGreaterThanOrEqual(count)
  } catch {
    expect.fail(
      `Expected ${String(count)} email(s), got ${String(smtp2go.sent.length)}.\nWorker logs:\n${workerLogs()}`,
    )
  }
  return smtp2go.sent
}

/** The one message the path under test sent. */
async function awaitEmail(): Promise<WireEmail> {
  return (await awaitEmails())[0]
}

/** What `server.fetch` answers with: workerd's `Response`, not this realm's. */
type HarnessResponse = Awaited<ReturnType<typeof server.fetch>>

/** Fire one cron tick at the given UTC hour. `0` is the one that runs the daily jobs too. */
async function tick(utcHour: number): Promise<void> {
  const scheduledTime = new Date(Date.UTC(2026, 8, 16, utcHour, 0, 0))
  const result = await server.getWorker<HarnessEnv>().scheduled({ scheduledTime, cron: '0 */3 * * *' })
  expect(result.outcome, `Worker logs:\n${workerLogs()}`).toBe('ok')
}

describe('mailing the report digest from the Worker runtime', () => {
  /** Upload one report the way the desktop client does, and keep the amend credential it hands back. */
  async function upload(meta: Record<string, unknown>): Promise<{ id: string; amendKey: string }> {
    const form = new FormData()
    form.set('bundle', new File([new Uint8Array([0x50, 0x4b, 0x05, 0x06])], 'bundle.zip'), 'bundle.zip')
    form.set('meta', JSON.stringify(meta))

    // Serialize the multipart body here rather than handing `FormData` to the harness: the
    // boundary lives in the `content-type` header, and a body re-encoded without it parses as
    // garbage inside the Worker.
    const packed = new Response(form)
    const response = await server.fetch('http://api.getcmdr.com/error-report', {
      method: 'POST',
      headers: { 'Content-Type': packed.headers.get('content-type') ?? '' },
      body: await packed.arrayBuffer(),
    })
    const text = await response.text()
    expect(response.status, `Body: ${text}\nWorker logs:\n${workerLogs()}`).toBe(200)
    return JSON.parse(text) as { id: string; amendKey: string }
  }

  /** The harness answers with workerd's own `Response`, which is not this realm's `Response`. */
  async function amend(id: string, body: Record<string, unknown>): Promise<HarnessResponse> {
    return await server.fetch(`http://api.getcmdr.com/error-report/${id}/amend`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
  }

  it('mails ONE digest for a report, its amendment, a crash, and feedback, then stamps them all', async () => {
    const uploaded = await upload({
      id: 'ERR-2345A',
      kind: 'user',
      appVersion: '0.9.1',
      osVersion: 'macOS 26.0',
      arch: 'aarch64',
      userNote: 'Copying to the NAS stalls at 99%',
      email: 'reporter@example.com',
      generatedAt: new Date().toISOString(),
    })
    const amended = await amend(uploaded.id, {
      amendKey: uploaded.amendKey,
      note: 'It also stalls on a local disk',
      email: 'reporter@example.com',
    })
    expect(amended.status, `Worker logs:\n${workerLogs()}`).toBe(200)

    const env = await server.getWorker<HarnessEnv>().getEnv()
    await env.TELEMETRY_DB.prepare(
      `INSERT INTO crash_reports
         (hashed_ip, backtrace, app_version, os_version, arch, signal, top_function, build_mode,
          short_id, panic_message, app_fate)
       VALUES ('', 'cmdr_fs::copy::stream', '0.9.1', 'macOS 26.0', 'aarch64', 'SIGSEGV',
               'cmdr_fs::copy::stream', 'release', 'CRASH-7KQ2M',
               'assertion failed: chunk.len() > 0', 'ended')`,
    ).run()
    await env.TELEMETRY_DB.prepare(
      `INSERT INTO feedback (feedback, app_version, os_version, build_mode)
       VALUES ('The dual pane finally makes sense to me', '0.9.1', 'macOS 26.0', 'release')`,
    ).run()

    // Intake mails nothing on its own: the digest is the one email.
    expect(smtp2go.sent).toHaveLength(0)

    await tick(3)
    const email = await awaitEmail()

    expect(smtp2go.sent).toHaveLength(1)
    expect(email.to).toBe(crashRecipient)
    expect(email.subject).toBe(
      'Cmdr digest: 1 crash, 1 error, 1 note added, 1 feedback in the past 3h (2026-09-16 03:00 UTC)',
    )
    // One person left an address (twice), so a plain reply reaches them.
    expect(email.replyTo).toBe('reporter@example.com')
    expect(email.html).toContain('Copying to the NAS stalls at 99%')
    expect(email.html).toContain('It also stalls on a local disk')
    expect(email.html).toContain('assertion failed: chunk.len() &gt; 0')
    expect(email.html).toContain('The dual pane finally makes sense to me')
    // CI health read GitHub on the same tick in workerd, found it green, and said nothing.
    const ciStates = await env.TELEMETRY_DB.prepare(`SELECT state FROM ci_health WHERE signal = 'main'`).first()
    expect(ciStates, `Worker logs:\n${workerLogs()}`).toEqual({ state: 'green' })

    for (const table of ['crash_reports', 'error_reports', 'error_report_amendments', 'feedback']) {
      const pending = await env.TELEMETRY_DB.prepare(
        `SELECT COUNT(*) AS n FROM ${table} WHERE notified_at IS NULL`,
      ).first<{ n: number }>()
      expect(pending?.n, table).toBe(0)
    }
  })

  it('runs the daily path probe against the email cycle, spending no email on it', async () => {
    await tick(0)

    expect(smtp2go.cycleChecks, `Worker logs:\n${workerLogs()}`).toBe(1)
    expect(smtp2go.sent).toHaveLength(0)
  })

  it('leaves the digest unstamped when SMTP2Go answers 200 but reports the send failed', async () => {
    const env = await server.getWorker<HarnessEnv>().getEnv()
    await env.TELEMETRY_DB.prepare(
      `INSERT INTO feedback (feedback, app_version, os_version, build_mode)
       VALUES ('The queue could use a pause button', '0.9.1', 'macOS 26.0', 'release')`,
    ).run()
    const pending = async (): Promise<number | undefined> =>
      (
        await env.TELEMETRY_DB.prepare('SELECT COUNT(*) AS n FROM feedback WHERE notified_at IS NULL').first<{
          n: number
        }>()
      )?.n
    smtp2go.failAll = true

    await tick(3)
    await awaitEmail()
    expect(await pending(), `Worker logs:\n${workerLogs()}`).toBe(1)

    // The next tick retries and stamps, which also keeps the row out of the tests below.
    smtp2go.reset()
    await tick(3)
    await awaitEmail()
    expect(await pending()).toBe(0)
  })
})

describe('mailing the device count alert from the Worker runtime', () => {
  it('alerts on the seat that crossed the threshold, naming the customer Paddle knows', async () => {
    // One device short of the alert threshold, so this validation is the one that crosses it.
    const env = await server.getWorker<HarnessEnv>().getEnv()
    const seen = new Date().toISOString()
    const devices = Object.fromEntries(Array.from({ length: 5 }, (_, i) => [`device-${String(i)}`, seen]))
    await env.LICENSE_CODES.put(`devices:${paddleTransaction.id}`, JSON.stringify({ devices }))

    const response = await server.fetch('http://api.getcmdr.com/validate', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ transactionId: paddleTransaction.id, deviceId: 'device-the-sixth' }),
    })
    expect(response.status, `Worker logs:\n${workerLogs()}`).toBe(200)

    const email = await awaitEmail()

    expect(email.to).toBe('legal@getcmdr.com')
    expect(email.subject).toBe(`Device count alert: ${paddleTransaction.id} (6 devices)`)
    expect(email.html).toContain(paddleCustomer.email)
    expect(email.html).toContain('vendors.paddle.com')
  })
})

/**
 * Last, and deliberately so: the only way to the DB size alert is a telemetry DB actually past
 * the first alert step (200 MB is 50% of the harness's 400 MB `D1_MAX_DB_MB`), and the harness's D1
 * keeps that weight for the rest of the run. Writing it takes a few seconds and lives in the
 * harness's temp directory, which goes away with `server.close()`. The migrations give it an empty
 * `db_size_samples`, so the first tick counts as crossing the step.
 */
describe('mailing the DB size alert from the Worker runtime', () => {
  it('alerts with the per-table row counts that say where the size went', async () => {
    const env = await server.getWorker<HarnessEnv>().getEnv()
    await env.TELEMETRY_DB.prepare('CREATE TABLE ballast (blob TEXT)').run()
    const chunk = 'x'.repeat(500_000)
    for (let i = 0; i < 480; i++) {
      const written = await env.TELEMETRY_DB.prepare('INSERT INTO ballast (blob) VALUES (?)').bind(chunk).run()
      if (written.meta.size_after > 200 * 1024 * 1024) break
    }

    await tick(0)

    // Other daily jobs ride the same tick, so the alert is found by subject rather than by position.
    const sent = await awaitEmails(1)
    const email = sent.find((mail) => mail.subject.startsWith('Cmdr: telemetry DB is'))

    expect(email, `Sent: ${sent.map((mail) => mail.subject).join(', ')}`).toBeDefined()
    expect(email?.to).toBe(crashRecipient)
    expect(email?.html).toContain('crash_reports')
    expect(email?.html).toContain('daily_active_users')
  }, 60_000)
})
