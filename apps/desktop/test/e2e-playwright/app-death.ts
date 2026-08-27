/**
 * The dead-app circuit breaker.
 *
 * One Tauri instance backs every test on a shard, so when it stops answering there is
 * nothing left for any later test to do except find out slowly: each one spends its full
 * 15 s timeout inside `ensureAppReady` waiting on a socket nobody is listening to, and the
 * CI retry spends it again. A wedge 48 s into a run turned 223 tests into two hours of CI
 * that reported nothing about any of them.
 *
 * So the first test to meet a silent app records that, and every test after it fails
 * immediately with a message that names where the app went and says the cascade is noise.
 *
 * The record is a FILE, not a module variable: Playwright discards a worker process after a
 * failure, and the very first thing the breaker sees IS a failure, so in-process state
 * would be gone by the test that needs to read it. The marker sits next to the shard's
 * socket, which is what makes it per-shard: parallel shards each own an instance and must
 * never read each other's verdict.
 *
 * ❗ This deliberately does NOT try to revive anything. `fixtures.ts`'s `breakTheCascade`
 * handles the recoverable wedge (an overlay the app is merely unwilling to close). A shard
 * that reaches here is past that: the webview is not answering at all, and continuing to
 * ask is what cost the two hours.
 */

import fs from 'fs'
import net from 'net'

/**
 * The deadline for a single liveness probe. Generous on purpose: the probe is one
 * `document.querySelector` and answers in about a millisecond on a healthy app, so the only
 * thing this number buys is protection against a false positive on a loaded Docker VM,
 * where a wrong verdict would abandon a whole shard. It costs the run 10 s exactly once,
 * on the test that discovers the death; every test after that is instant.
 */
export const APP_PROBE_DEADLINE_MS = 10_000

export type AppDeath = {
  /** Human phrase for when the app went silent, e.g. `during "Archive browsing › …"`. */
  where: string
  /** What the probe saw: a deadline with no answer, or the rejection it got instead. */
  detail: string
}

/** The minimum of a Playwright page this module needs, so tests can hand it a stub. */
export type EvaluatablePage = {
  evaluate: {
    (js: string): Promise<unknown>
    <T>(js: string): Promise<T>
  }
}

/**
 * Read per call rather than at import: it keeps the module honest when the socket is
 * repointed (the Go check runner sets `CMDR_PLAYWRIGHT_SOCKET` per shard), and it's what
 * lets the unit tests aim the marker at a temp dir.
 */
function socketPath(): string {
  return process.env.CMDR_PLAYWRIGHT_SOCKET ?? '/tmp/tauri-playwright.sock'
}

function markerPath(): string {
  return `${socketPath()}.dead`
}

/**
 * Forget any previous run's verdict. Called from `global-setup.ts`: the marker outlives the
 * process that wrote it, and a stale one would fail an entire healthy run before its first
 * test got to touch the app.
 */
export function clearAppDeathMarker(): void {
  fs.rmSync(markerPath(), { force: true })
}

/** The recorded death for this shard, or null while the app is presumed alive. */
export function readAppDeath(): AppDeath | null {
  try {
    return JSON.parse(fs.readFileSync(markerPath(), 'utf8')) as AppDeath
  } catch {
    // No marker (the normal case), or one we can't parse. Either way: no verdict.
    return null
  }
}

/** Record the death so every later test, in this worker or the next one, fails fast. */
function recordAppDeath(death: AppDeath): void {
  try {
    fs.writeFileSync(markerPath(), JSON.stringify(death))
  } catch {
    // A marker we can't write costs the run its fast path, not its correctness. Never let
    // the breaker itself be the reason a test dies.
  }
}

const TIMED_OUT = Symbol('timed-out')

/**
 * `Promise.race` with a timer that gets cleaned up. Both branches are subscribed, so a late
 * rejection from `work` lands on a handler rather than on the process.
 */
async function withDeadline<T>(work: Promise<T>, ms: number): Promise<T | typeof TIMED_OUT> {
  let timer: ReturnType<typeof setTimeout> | undefined
  const deadline = new Promise<typeof TIMED_OUT>((resolve) => {
    timer = setTimeout(() => resolve(TIMED_OUT), ms)
  })
  try {
    return await Promise.race([work, deadline])
  } finally {
    clearTimeout(timer)
  }
}

/**
 * Ask the app the cheapest question there is. Returns null when it answers, or what went
 * wrong when it doesn't.
 *
 * The ANSWER is the signal, never its value: a spec that destroyed the focused window mid-
 * test gets null back from `evaluate` and is perfectly healthy. Only silence counts.
 */
export async function probeAppAlive(
  page: EvaluatablePage,
  deadlineMs: number = APP_PROBE_DEADLINE_MS,
): Promise<string | null> {
  const answered = page.evaluate(`document.querySelector('.dual-pane-explorer') !== null`).then(
    () => null,
    (err: unknown) => `the liveness probe was rejected: ${String(err)}`,
  )
  const verdict = await withDeadline(answered, deadlineMs)
  if (verdict === TIMED_OUT) return `no answer to the liveness probe in ${deadlineMs} ms`
  return verdict
}

/**
 * Ping the plugin's socket directly, on our own connection, under a deadline.
 *
 * ❗ This exists because `tauriPage`'s setup does the same handshake with NO deadline:
 * `PluginClient.connect()` then `send({ type: 'ping' })`, and `send` waits on a line that a
 * wedged app never writes (`@srsholmes/tauri-playwright`, verified against the vendored
 * `dist/index.js`, 2026-08-27). Against a live-but-unresponsive app that alone burns the
 * whole 15 s test timeout, which is why the breaker cannot be built on `tauriPage` and has
 * to speak the two-line protocol itself.
 *
 * Returns null when a line comes back, or what went wrong when one doesn't. As with the
 * webview probe, the ANSWER is the signal: a `{ ok: false }` reply is still an app that is
 * awake and talking.
 */
export function pingAppSocket(deadlineMs: number = APP_PROBE_DEADLINE_MS): Promise<string | null> {
  return new Promise((resolve) => {
    const socket = net.createConnection({ path: socketPath() })
    let settled = false
    const finish = (verdict: string | null): void => {
      if (settled) return
      settled = true
      clearTimeout(timer)
      socket.destroy()
      resolve(verdict)
    }
    const timer = setTimeout(() => finish(`no answer to the socket ping in ${deadlineMs} ms`), deadlineMs)

    socket.on('connect', () => socket.write('{"type":"ping"}\n'))
    socket.on('data', () => finish(null))
    socket.on('error', (err: Error) => finish(`the socket ping was rejected: ${err.message}`))
    // Ordering matters: `finish` has already fired on a good ping, so reaching here means
    // the app hung up mid-handshake. Without this the promise would wait out the deadline
    // for a socket that is already gone.
    socket.on('close', () => finish('the app closed the socket without answering the ping'))
  })
}

function cascadeMessage(death: AppDeath): string {
  return (
    `The shared Tauri app STOPPED ANSWERING ${death.where} (${death.detail}), so this test never ran. ` +
    `Every failure from that point down is this same wedge and none of them say anything about the ` +
    `test they're filed under: go to the last test that PASSED and look at what ran next. ` +
    `See apps/desktop/test/e2e-playwright/DETAILS.md § "The dead-app circuit breaker".`
  )
}

/**
 * The cheap half of the gate: throws when a previous test already recorded the death, and
 * does it with one synchronous file read and NO app contact at all.
 *
 * ❗ This has to run before the `tauriPage` fixture is instantiated, which is why it lives
 * on its own fixture that depends on nothing (`fixtures.ts`). `tauriPage`'s own setup is
 * what blocks against a wedged app: connecting to the socket succeeds, because the process
 * is alive and the kernel still accepts, and then nothing ever answers. Measured against a
 * SIGSTOPped app, that alone eats the full 15 s test timeout before a single line of this
 * file's code gets to run (macOS 15, 2026-08-27).
 */
export function failIfAppIsKnownDead(): void {
  const known = readAppDeath()
  if (known !== null) throw new Error(cascadeMessage(known))
}

/**
 * How long a test gets once the app is known dead. Throwing from the gate is NOT enough on
 * its own: Playwright records the fixture error and still runs the spec's `beforeEach`
 * hooks, which hang on `tauriPage` for the whole 15 s (measured, 22 tests, macOS 15,
 * 2026-08-27). So the gate shrinks the test's own budget on the way out, and this is what
 * a doomed test costs instead. Small enough to be nothing across 300 tests, large enough
 * that the teardown still gets to run and report.
 */
export const DEAD_APP_TEST_BUDGET_MS = 1000

/**
 * The gate that runs before `tauriPage` exists: the free marker check, then a socket ping
 * for the test that gets there first. Records what it finds, so the rest of the shard pays
 * the marker read and nothing else.
 *
 * Returns the message to fail the test with, or null when the app is reachable.
 */
export async function findAppDeath(
  testName: string,
  deadlineMs: number = APP_PROBE_DEADLINE_MS,
): Promise<string | null> {
  const known = readAppDeath()
  if (known !== null) return cascadeMessage(known)

  const detail = await pingAppSocket(deadlineMs)
  if (detail === null) return null

  const death: AppDeath = { where: `before "${testName}"`, detail }
  recordAppDeath(death)
  return cascadeMessage(death)
}

/**
 * The gate every test passes through. Throws when the app is dead, and once it has thrown
 * once it throws for free, without a probe and without touching the app at all.
 */
export async function assertAppAlive(
  page: EvaluatablePage,
  testName: string,
  deadlineMs: number = APP_PROBE_DEADLINE_MS,
): Promise<void> {
  // Repeats the cheap gate on purpose: this one runs after `tauriPage` exists, so it is the
  // only check standing if the fixture order ever changes underneath us.
  failIfAppIsKnownDead()

  const detail = await probeAppAlive(page, deadlineMs)
  if (detail === null) return

  const death: AppDeath = { where: `before "${testName}"`, detail }
  recordAppDeath(death)
  throw new Error(cascadeMessage(death))
}

/**
 * The teardown half: catches an app the test itself killed, so the NEXT test gets the
 * instant verdict instead of paying the probe deadline to rediscover it. Returns the
 * message to fail this test with, or null when the app is still answering.
 */
export async function checkAppSurvived(
  page: EvaluatablePage,
  testName: string,
  deadlineMs: number = APP_PROBE_DEADLINE_MS,
): Promise<string | null> {
  if (readAppDeath() !== null) return null // Already recorded upstream; don't pile on.

  const detail = await probeAppAlive(page, deadlineMs)
  if (detail === null) return null

  const death: AppDeath = { where: `during "${testName}"`, detail }
  recordAppDeath(death)
  return (
    `The shared Tauri app STOPPED ANSWERING during this test (${detail}). Every test after it on ` +
    `this shard fails without running, so THIS is the one to debug. ` +
    `See apps/desktop/test/e2e-playwright/DETAILS.md § "The dead-app circuit breaker".`
  )
}
