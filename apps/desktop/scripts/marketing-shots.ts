/**
 * Reshoots the brand masters in `brand/screenshots/` and re-measures the website
 * hero's pane rectangles, in one command.
 *
 * ```bash
 * pnpm marketing:shots            # reshoot, using the warm shots data dir
 * pnpm marketing:shots --build    # rebuild the Playwright binary first
 * pnpm marketing:shots --out /tmp/shots   # write somewhere else, leave brand/ alone
 * ```
 *
 * ❗ Leave the machine alone while it runs. macOS draws the wide window shadow only for
 * the KEY window, so every shot takes the front position first, and clicking into
 * another app mid-run costs retries. Unlike `pnpm i18n:capture`, this does NOT refuse to
 * start behind another app: it claims the front through System Events, which works
 * across apps, and then proves it in the pixels.
 *
 * How it differs from `i18n-capture.ts`, and why:
 *
 * - **`CMDR_E2E_MODE` stays unset.** That variable paints the blue `E2E MODE` title
 *   bar AND sets `ActivationPolicy::Prohibited`, which makes the app permanently
 *   unable to become the key window. A marketing master needs both the prod-looking
 *   title bar and the key-window shadow, so an E2E-mode run could not produce one even
 *   in principle. The Playwright plugin is gated on the cargo FEATURE, not the mode
 *   (`src-tauri/src/lib.rs:285-292`), so the socket still works.
 * - **`CMDR_E2E_START_PATH` stays unset**, and the shard skips the fixture machinery.
 *   The suite's post-test guard deletes anything not in the fixture manifest, and this
 *   run photographs REAL folders. See `docs/specs/marketing-screenshot-pipeline-plan.md`
 *   § "Real data comes from the data dir".
 * - **The data dir persists.** Its index stays warm, so the whole-drive reconcile that
 *   makes every size cell an hourglass is a one-time cost rather than a 20-minute tax
 *   on every round.
 */

import { spawn, spawnSync } from 'node:child_process'
import type { ChildProcess, SpawnSyncOptions } from 'node:child_process'
import { existsSync, mkdirSync, statSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { frontmostApp, hostTriple, reserveFreePort, waitForSocket, warnIfForeignCmdr } from './capture-runtime.ts'

const LOG = '[marketing-shots]'

const here = dirname(fileURLToPath(import.meta.url))
const desktopDir = join(here, '..')
// The Cargo workspace root is the REPO root, so the binary lands in
// `<repo-root>/target/<triple>/release/Cmdr`, not under `apps/desktop/src-tauri`.
const repoRoot = join(desktopDir, '..', '..')

const wantBuild = process.argv.includes('--build')
const outIdx = process.argv.indexOf('--out')
const outDir = outIdx >= 0 ? (process.argv.at(outIdx + 1) ?? '') : join(repoRoot, 'brand', 'screenshots')

/**
 * The shots instance's own data dir, deliberately OUTSIDE the repo and deliberately
 * not throwaway.
 *
 * Its own path so it can never collide with prod, plain dev, a `--worktree` dev
 * instance, or an E2E shard; and persistent so its file index stays warm. A cold dir
 * costs one whole-drive reconcile, during which every size cell shows an hourglass and
 * every folder size reads `≥` — which is what a whole round of unusable masters looks
 * like.
 */
const dataDir = join(homedir(), 'Library', 'Application Support', 'com.veszelovszki.cmdr-shots')

let appProc: ChildProcess | null = null

function run(cmd: string, args: string[], opts: SpawnSyncOptions = {}) {
  const res = spawnSync(cmd, args, { cwd: desktopDir, stdio: 'inherit', ...opts })
  if (res.status !== 0) throw new Error(`${cmd} ${args.join(' ')} exited ${String(res.status)}`)
}

/**
 * Stops ONLY the app this script launched.
 *
 * ❌ Never `pkill -f 'target.*Cmdr'`: that pattern matches every worktree's Cmdr, and
 * it has killed a parallel session's app mid-shoot twice. We spawned the process, so
 * its pid is the exact thing to signal.
 */
function killApp() {
  const pid = appProc?.pid
  if (pid === undefined) return
  try {
    process.kill(pid, 'SIGTERM')
  } catch {
    // Already gone; ESRCH is the normal way this ends.
  }
  appProc = null
}

function binaryPath(): string {
  const triple = hostTriple()
  const binary = join(repoRoot, 'target', triple, 'release', 'Cmdr')
  if (!existsSync(binary)) {
    throw new Error(`No Playwright binary at ${binary}. Run with --build, or \`pnpm test:e2e:playwright:build\`.`)
  }
  return binary
}

/**
 * Warns when the binary predates the source it's supposed to photograph.
 *
 * A warning rather than a rebuild: rebuilding takes minutes and the caller may know the
 * change was frontend-only. Silence would be worse than either, because a master shot
 * from a stale binary looks completely fine.
 */
function warnIfBinaryIsStale(binary: string): void {
  const builtAt = statSync(binary).mtimeMs
  const sources = [join(desktopDir, 'src'), join(desktopDir, 'src-tauri', 'src')]
  const newest = Math.max(...sources.filter(existsSync).map((dir) => statSync(dir).mtimeMs))
  if (newest > builtAt) {
    console.warn(`${LOG} WARNING: ${binary} is older than the sources. Re-run with --build to photograph the change.`)
  }
}

function build(): void {
  console.log(`${LOG} building the Playwright binary…`)
  // Deliberately the SAME command (and therefore the same cargo config) as
  // `pnpm test:e2e:playwright:build`, so the two share a cache. ❌ Don't add
  // `--config profile.release.debug-assertions=true` the way `i18n-capture.ts` does:
  // that flips `CMDR_MOCK_LICENSE` on, which changes visible About/licensing copy.
  run('pnpm', ['test:e2e:playwright:build'])
}

/**
 * Writes the shots instance's settings on first run, and leaves them alone after.
 *
 * Every entry here is something a prod-mode launch would otherwise put ON TOP of a
 * master, or send somewhere it shouldn't. `isE2eRun()` is false by design in this run
 * (see the file header), and these are the suppressions that come back with it:
 *
 * - `analytics.enabled` off, because a release build points at the PRODUCTION PostHog
 *   project and a fresh data dir mints a new install id, i.e. a phantom user.
 * - `updates.autoCheck` off, because its poll loop can raise a "Restart to update"
 *   toast over a shot.
 * - `whatsNew.showOnUpdate` off and the upgrade nudge marked shown: on a fresh data dir
 *   both fire once, which would be precisely the run that proves the pipeline.
 * - The cosmetics are the look the masters are supposed to have, set explicitly rather
 *   than inherited from whatever a previous round left behind.
 *
 * Not written after the first run, so David can adjust the instance by hand (pane
 * paths, favorites, tabs) and keep it.
 */
function seedSettingsIfNew(): void {
  const settingsPath = join(dataDir, 'settings.json')
  if (existsSync(settingsPath)) return
  console.log(`${LOG} first run: seeding ${settingsPath}`)
  writeFileSync(
    settingsPath,
    `${JSON.stringify(
      {
        isOnboarded: true,
        'analytics.enabled': false,
        'updates.autoCheck': false,
        'whatsNew.showOnUpdate': false,
        'onboarding.upgradeNudgeShown': true,
        'appearance.appColor': 'cmdr-gold',
        'appearance.fileSizeFormat': 'binary',
        'appearance.showFunctionKeyBar': true,
        'mediaIndex.enabled': false,
      },
      null,
      2,
    )}\n`,
  )
}

async function main(): Promise<void> {
  if (process.platform !== 'darwin') {
    throw new Error('The marketing masters are macOS window shots (traffic lights, a real system shadow), macOS only.')
  }
  if (outIdx >= 0 && outDir === '') throw new Error('--out needs a directory')

  warnIfForeignCmdr(LOG)
  // ❗ Deliberately NOT `waitForFrontPositionToClear` (which `i18n-capture.ts` uses and
  // needs). This pipeline claims the front position per shot through System Events,
  // which DOES work against another app, so refusing to start behind one would refuse
  // every run: the person starting it is by definition in a terminal that holds the
  // front. Instead the front grab is attempted and then VERIFIED in the pixels — an
  // unfocused window's shadow is 68/52 rather than 112/76, and `shootWithShadow`
  // rejects it by name. A warning is still worth it, because clicking into another app
  // mid-run is a real way to spend three retries.
  const front = frontmostApp()
  if (front !== null && front !== 'Cmdr') {
    console.warn(`${LOG} '${front}' holds the front position. Leave the machine alone; each shot takes it back.`)
  }

  if (wantBuild) build()
  const binary = binaryPath()
  warnIfBinaryIsStale(binary)

  mkdirSync(dataDir, { recursive: true })
  mkdirSync(outDir, { recursive: true })
  seedSettingsIfNew()

  const socket = `/tmp/tauri-playwright-shots-${String(process.pid)}.sock`
  const mcpPort = await reserveFreePort()
  const sharedEnv = {
    CMDR_PLAYWRIGHT_SOCKET: socket,
    CMDR_MCP_PORT: String(mcpPort),
    CMDR_MCP_ENABLED: '1',
    CMDR_SHOTS_OUT_DIR: outDir,
  }

  console.log(`${LOG} launching the app; data dir ${dataDir}, MCP port ${String(mcpPort)}…`)
  appProc = spawn(binary, [], {
    cwd: desktopDir,
    stdio: 'inherit',
    env: {
      ...process.env,
      ...sharedEnv,
      CMDR_DATA_DIR: dataDir,
      // Keeps a Keychain approval dialog from landing over a shot. Without E2E mode
      // the app would otherwise talk to the REAL macOS Keychain
      // (`secrets/mod.rs:103-120`), and this override is checked before that branch.
      CMDR_SECRET_STORE: 'file',
      // The chat shot runs off a seeded thread, so no provider is ever called. This
      // keeps the composer from rendering its "provider off" hint over the shot.
      CMDR_E2E_ASK_CMDR_FAKE: '1',
      // Suppresses analytics in a RELEASE build (`analytics/mod.rs:88-90`), which
      // otherwise points at the production PostHog project and would register this
      // data dir as a phantom install. The seeded `analytics.enabled: false` is the
      // belt to this suspenders: the env var is a coincidence of the suppression
      // list, not a contract.
      CI: '1',
      // ❗ Deliberately absent: CMDR_E2E_MODE (would make the window permanently
      // unfocusable and paint the blue title bar) and CMDR_E2E_START_PATH (would arm
      // the fixture guard against real folders). See this file's header.
    },
  })
  appProc.on('exit', (code) => {
    if (code != null && code !== 0) console.warn(`${LOG} app exited with code ${String(code)}`)
  })

  await waitForSocket(socket, 60000)
  console.log(`${LOG} socket ready; staging and shooting…`)

  try {
    // No `--project`: Playwright treats a positional as a project filter when
    // `--project` is set, and the shard's own `testMatch` already restricts the run.
    run('npx', ['playwright', 'test', '--config', 'test/e2e-playwright/playwright.config.ts'], {
      env: {
        ...process.env,
        ...sharedEnv,
        CMDR_E2E_SHARD_KIND: 'marketing-shots',
        CMDR_SHOTS_PID: String(appProc.pid ?? ''),
      },
    })
  } finally {
    killApp()
  }

  console.log(`${LOG} masters written to ${outDir}.`)
  console.log(`${LOG} next: apps/website/scripts/regenerate-hero.sh, then refresh the listings in brand/listings/.`)
}

main()
  .then(() => {
    killApp()
    process.exit(0)
  })
  .catch((e: unknown) => {
    console.error(`${LOG} ${e instanceof Error ? e.message : String(e)}`)
    killApp()
    process.exit(1)
  })
