#!/usr/bin/env node
/**
 * One-shot orchestrator for the i18n screenshot-capture loop.
 *
 * Mirrors the manual single-spec recipe (see `test/e2e-playwright/DETAILS.md`)
 * but wraps the whole lifecycle so capture is a single command:
 *   1. refuse to run if any Cmdr is already up (we never kill a foreign instance).
 *   2. (optional `--build`) compile the capture binary: the `playwright-e2e`
 *      feature PLUS `CMDR_I18N_CAPTURE_BUILD=1`, which bakes the capture
 *      instrumentation into the frontend (see `messages.svelte.ts`).
 *   3. create a fresh fixture tree.
 *   4. launch the binary (E2E mode, unique socket) and wait for its socket.
 *   5. run ONLY `i18n-capture.spec.ts` (via the `i18n-capture` shard kind),
 *      which drives the surfaces, records keys, and writes the screenshots +
 *      `screenshots/capture-report.json`.
 *   6. stop ONLY the app WE launched (its pid), always, even on failure.
 *
 * Then run `pnpm i18n:couple` to write the `@key.screenshot` couplings.
 *
 * Usage:
 *   pnpm i18n:shots              # the full re-run: this with --build, then couple
 *   pnpm i18n:capture --build    # build the capture binary, then capture
 *   pnpm i18n:capture            # reuse a binary from a PRIOR --build run
 *
 * `pnpm i18n:shots` is the single entry point for a fresh end-to-end refresh
 * (capture with `--build`, then `i18n:couple`); reach for it after a UI change.
 *
 * ALWAYS use `--build` unless a previous `--build` already produced a capture
 * binary: the capture API is absent from a binary built by the normal E2E lane
 * (that lane doesn't set `CMDR_I18N_CAPTURE_BUILD`).
 *
 * Extending to more surfaces: add a staging block to `i18n-capture.spec.ts`
 * (stage → setSurface → rerender → screenshot → dump) and re-run this. No change
 * here is needed.
 */

import { spawn, spawnSync, execSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import net from 'node:net'

const here = dirname(fileURLToPath(import.meta.url))
const desktopDir = join(here, '..')
// The Cargo workspace root is the REPO root, so the built binary lands in
// `<repo-root>/target/<triple>/release/Cmdr`, NOT under `apps/desktop/src-tauri`.
// This matches `desktop-svelte-e2e-playwright.go`'s binary resolution.
const repoRoot = join(desktopDir, '..', '..')
const wantBuild = process.argv.includes('--build')
// A per-run unique socket (not the shared default) so a parallel dev/E2E session
// in another worktree can never collide on the socket path.
const SOCKET = process.env.CMDR_PLAYWRIGHT_SOCKET ?? `/tmp/tauri-playwright-i18n-${String(process.pid)}.sock`

/**
 * @param {string} cmd
 * @param {string[]} args
 * @param {import('node:child_process').SpawnSyncOptions} [opts]
 */
function run(cmd, args, opts = {}) {
  const res = spawnSync(cmd, args, { cwd: desktopDir, stdio: 'inherit', ...opts })
  if (res.status !== 0) {
    throw new Error(`${cmd} ${args.join(' ')} exited ${String(res.status)}`)
  }
}

/**
 * Resolves the host target triple (matches the build target).
 * @returns {string}
 */
function hostTriple() {
  const line = execSync('rustc -vV', { encoding: 'utf8' })
    .split('\n')
    .find((l) => l.startsWith('host:'))
  if (line === undefined) throw new Error('could not parse host triple from `rustc -vV`')
  return line.replace('host:', '').trim()
}

/**
 * Polls a Unix socket until connectable or the deadline passes.
 * @param {string} path
 * @param {number} timeoutMs
 * @returns {Promise<void>}
 */
async function waitForSocket(path, timeoutMs) {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    const ok = await new Promise((resolve) => {
      const c = net.connect(path, () => {
        c.end()
        resolve(true)
      })
      c.on('error', () => resolve(false))
    })
    if (ok) return
    if (Date.now() > deadline) throw new Error(`tauri-playwright socket ${path} never became ready`)
    await new Promise((r) => setTimeout(r, 150))
  }
}

/** @type {import('node:child_process').ChildProcess | null} */
let appProc = null
// Stop ONLY the app process THIS script launched, never a broad
// `pkill -f 'target.*Cmdr'` — that pattern matches any worktree's running Cmdr
// (dev or E2E) and would clobber a parallel session. We spawn the binary
// ourselves, so `appProc.pid` is the exact process to signal. Best-effort and
// idempotent (SIGTERM a gone pid throws ESRCH, which we swallow).
function killApp() {
  if (appProc?.pid == null) return
  try {
    process.kill(appProc.pid, 'SIGTERM')
  } catch {
    /* already gone */
  }
  appProc = null
}
process.on('exit', killApp)
process.on('SIGINT', () => {
  killApp()
  process.exit(130)
})

/**
 * Warns (does not block) if another Cmdr is already running. Teardown only stops
 * the PID we launch and the native screenshot targets our own window IDs, so a
 * foreign instance (a dev session in another worktree) is safe to coexist with.
 * BUT separate-window captures (Settings, Viewer, Shortcuts, About) rely on
 * `set_focus` bringing an occluded window frontmost, which macOS won't honor if
 * another app is actively foreground — so for clean shots the screen should be
 * idle during a run. We surface the foreign instance rather than hard-failing.
 */
function warnIfForeignCmdr() {
  const res = spawnSync('pgrep', ['-fl', 'target.*Cmdr'], { encoding: 'utf8' })
  // pgrep exits 0 with matches, 1 with none.
  if (res.status === 0 && res.stdout.trim() !== '') {
    console.warn(
      `[i18n-capture] WARNING: another Cmdr is running — separate-window shots may capture stale frames ` +
        `if the screen isn't idle:\n${res.stdout.trim()}`,
    )
  }
}

async function main() {
  // Coexisting with a running Cmdr is safe (PID-scoped teardown, window-ID-scoped
  // capture); just warn, since a busy screen can spoil separate-window shots.
  warnIfForeignCmdr()

  if (wantBuild) {
    console.log('[i18n-capture] building capture binary…')
    // `CMDR_I18N_CAPTURE_BUILD=1` flips the `__CMDR_I18N_CAPTURE__` Vite define so
    // the frontend bundle BAKES IN the capture instrumentation. Only THIS build
    // sets it, so a binary built by the normal E2E lane has no capture API —
    // `pnpm i18n:capture` must always go through `--build`. The env propagates
    // through tauri-wrapper → Tauri → the vite build.
    run(
      'node',
      [
        'scripts/tauri-wrapper.js',
        'build',
        '--no-bundle',
        '--target',
        hostTriple(),
        '--',
        '--features',
        'playwright-e2e',
      ],
      { env: { ...process.env, CMDR_I18N_CAPTURE_BUILD: '1' } },
    )
  }

  const triple = hostTriple()
  const binary = join(repoRoot, 'target', triple, 'release', 'Cmdr')
  if (!existsSync(binary)) {
    throw new Error(`E2E binary not found at ${binary}.\nRun with --build first (\`pnpm i18n:capture --build\`).`)
  }

  // Fresh fixtures so the panes have predictable content for the screenshot.
  // This imports a `.ts` module, so the script runs under `tsx` (see the
  // `i18n:capture` package script), matching `check:type-drift`'s convention.
  const { createFixtures } = await import('../test/e2e-shared/fixtures.js')
  const startPath = createFixtures()
  console.log(`[i18n-capture] fixtures at ${startPath}`)

  console.log('[i18n-capture] launching app…')
  appProc = spawn(binary, [], {
    cwd: desktopDir,
    stdio: 'inherit',
    env: {
      ...process.env,
      CMDR_E2E_MODE: '1',
      CMDR_E2E_START_PATH: startPath,
      CMDR_PLAYWRIGHT_SOCKET: SOCKET,
    },
  })
  appProc.on('exit', (code) => {
    if (code != null && code !== 0) console.warn(`[i18n-capture] app exited with code ${String(code)}`)
  })

  await waitForSocket(SOCKET, 60000)
  console.log('[i18n-capture] socket ready; running capture spec…')

  // Don't pass `--project tauri` AND a positional spec path: Playwright treats
  // the positional as a project filter when `--project` is set, failing with
  // "Project(s) ... not found". The `i18n-capture` shard's `testMatch` already
  // restricts the run to the capture spec, and the config has only the `tauri`
  // project, so it runs by default. (See the suite CLAUDE.md note on this clash.)
  // Pass the SAME unique socket to Playwright: `fixtures.ts` reads
  // `CMDR_PLAYWRIGHT_SOCKET` to know which socket to connect to. Without this,
  // Playwright connects to the default `/tmp/tauri-playwright.sock` while the app
  // listens on our unique one, and the first `evaluate` hangs to timeout.
  run('npx', ['playwright', 'test', '--config', 'test/e2e-playwright/playwright.config.ts'], {
    env: {
      ...process.env,
      CMDR_E2E_START_PATH: startPath,
      CMDR_E2E_SHARD_KIND: 'i18n-capture',
      CMDR_PLAYWRIGHT_SOCKET: SOCKET,
    },
  })

  console.log('[i18n-capture] done. Next: `pnpm i18n:couple` to write @key.screenshot couplings.')
}

main()
  .then(() => {
    killApp()
    process.exit(0)
  })
  .catch((e) => {
    console.error(`[i18n-capture] ${e instanceof Error ? e.message : String(e)}`)
    killApp()
    process.exit(1)
  })
