/**
 * Launch primitives shared by the two capture orchestrators: `i18n-capture.ts`
 * (translator screenshots) and `marketing-shots.ts` (the brand masters).
 *
 * Both spawn the same Playwright-enabled binary, wait on the same kind of socket,
 * pin an MCP port the same way, and depend on the same macOS fact: a raw binary
 * cannot take the front position from an app that holds it, so a run started behind
 * another app photographs stale frames. That fact was learned once, expensively
 * (a run shipped 31 blank images), and it belongs in one place.
 *
 * Stdlib only, like `instance-id.ts`, so either orchestrator can import it with no
 * build step.
 */

import { execSync, spawnSync } from 'node:child_process'
import net from 'node:net'

/** Resolves the host target triple, which is where the built binary lands. */
export function hostTriple(): string {
  const line = execSync('rustc -vV', { encoding: 'utf8' })
    .split('\n')
    .find((l) => l.startsWith('host:'))
  if (line === undefined) throw new Error('could not parse host triple from `rustc -vV`')
  return line.replace('host:', '').trim()
}

/** Polls a Unix socket until connectable or the deadline passes. */
export async function waitForSocket(path: string, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    const ok = await new Promise<boolean>((resolve) => {
      const c = net.connect(path, () => {
        c.end()
        resolve(true)
      })
      c.on('error', () => {
        resolve(false)
      })
    })
    if (ok) return
    if (Date.now() > deadline) throw new Error(`tauri-playwright socket ${path} never became ready`)
    await new Promise<void>((r) => {
      setTimeout(r, 150)
    })
  }
}

/**
 * Reserves a free high port by binding ephemeral and reading the assigned port, then
 * releasing it. Used to pin `CMDR_MCP_PORT` so the app and the spec's MCP-client
 * helpers agree on where `/mcp` lives; without a pinned port the helper's discovery
 * comes back unusable. A tiny bind/release race window is acceptable (single local
 * process, no contention), and the OS ephemeral range satisfies the no-standard-ports
 * rule.
 */
export function reserveFreePort(): Promise<number> {
  return new Promise<number>((resolve, reject) => {
    const srv = net.createServer()
    srv.on('error', reject)
    srv.listen(0, '127.0.0.1', () => {
      const addr = srv.address()
      const port = addr && typeof addr === 'object' ? addr.port : 0
      srv.close(() => {
        if (port > 0) resolve(port)
        else reject(new Error('could not reserve a free MCP port'))
      })
    })
  })
}

/**
 * Warns when another Cmdr is already running.
 *
 * A warning, not a refusal: a parallel worktree's dev instance is normal here, and it
 * only matters if it holds the front position. ❌ Never answer this with
 * `pkill -f 'target.*Cmdr'` — that pattern matches every worktree's Cmdr and has
 * killed a parallel session's app mid-shoot.
 */
export function warnIfForeignCmdr(logPrefix: string): void {
  const res = spawnSync('pgrep', ['-fl', 'target.*Cmdr'], { encoding: 'utf8' })
  // pgrep exits 0 with matches, 1 with none.
  if (res.status === 0 && res.stdout.trim() !== '') {
    console.warn(
      `${logPrefix} WARNING: another Cmdr is running, so separate-window shots may capture stale frames ` +
        `if the screen isn't idle:\n${res.stdout.trim()}`,
    )
  }
}

/**
 * The apps whose holding the front position doesn't stop a capture. Finder and the
 * window server's own agents own the front whenever a user has simply hidden
 * everything, which is the state we ASK for, so refusing on them would make the check
 * impossible to satisfy.
 */
const FRONT_POSITION_OK = new Set(['Finder', 'Cmdr', 'loginwindow', 'WindowManager', 'Dock'])

/** How long to wait for a real app to give up the front, and how often to look. */
const FRONT_WAIT_MS = 30000
const FRONT_POLL_MS = 1000

/** The frontmost app's display name, or null when macOS won't say (or we're not on macOS). */
export function frontmostApp(): string | null {
  if (process.platform !== 'darwin') return null
  const asn = spawnSync('lsappinfo', ['front'], { encoding: 'utf8' })
  if (asn.status !== 0 || asn.stdout.trim() === '') return null
  const info = spawnSync('lsappinfo', ['info', '-only', 'name', asn.stdout.trim()], { encoding: 'utf8' })
  if (info.status !== 0) return null
  return /"LSDisplayName"="([^"]*)"/.exec(info.stdout)?.[1] ?? null
}

/**
 * Waits for the front position to be free before any window is photographed, and
 * refuses to start if it never is.
 *
 * ❗ An idle machine is NOT enough. The binary is spawned raw rather than through
 * LaunchServices, so macOS cooperative activation won't let it take the front from an
 * app that already holds it: the in-app `set_focus` remedy no-ops and the capture
 * reads stale frames. Measured with Chrome frontmost and nobody touching the laptop, a
 * run captured 13-14 of ~71 surfaces.
 *
 * It WAITS rather than checking once, because whoever starts the run is usually typing
 * in the very app that has to be hidden. Two seconds spent refusing beats a full run of
 * blanks that reads like a harness bug.
 */
export async function waitForFrontPositionToClear(logPrefix: string): Promise<void> {
  const started = frontmostApp()
  if (started === null || FRONT_POSITION_OK.has(started)) return

  console.warn(
    `${logPrefix} '${started}' holds the front position. Hide or quit it (⌘H is enough); ` +
      `waiting up to ${String(FRONT_WAIT_MS / 1000)}s. A run started behind another app captures blanks.`,
  )
  const deadline = Date.now() + FRONT_WAIT_MS
  for (;;) {
    const now = frontmostApp()
    if (now === null || FRONT_POSITION_OK.has(now) || now !== started) {
      console.log(`${logPrefix} front position clear (now '${now ?? '<none>'}'); starting.`)
      return
    }
    if (Date.now() >= deadline) {
      throw new Error(
        `'${now}' still holds the front position after ${String(FRONT_WAIT_MS / 1000)}s. Hide or quit it and ` +
          're-run: macOS will not let this binary take the front from it, so every shot would read a stale frame.',
      )
    }
    await new Promise((resolve) => setTimeout(resolve, FRONT_POLL_MS))
  }
}
