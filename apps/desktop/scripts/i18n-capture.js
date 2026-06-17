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
 *   pnpm i18n:overflow           # pseudolocale OVERFLOW pass (= --build --locale en-XA)
 *
 * The `--locale <tag>` axis (default `en`) switches the capture to an OVERFLOW
 * pass: it generates the locale (en-XA), the driver switches the app to it, the
 * screenshots land in a SEPARATE `screenshots/overflow/` dir, and a DOM clip scan
 * writes `overflow/overflow-report.md`. An overflow pass never touches the
 * coupling artifacts (`capture-report.json` / `@key.screenshot`) and runs only
 * the main capture pass. See `docs/guides/i18n.md` § Pseudolocale.
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
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
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

/**
 * `--locale <tag>` axis. Default `en` is the normal coupling capture (writes
 * `@key.screenshot` via `i18n:couple`). Any other tag (e.g. `en-XA`, the
 * pseudolocale) is an OVERFLOW pass: the driver switches the app to that locale,
 * screenshots land in a SEPARATE `screenshots/overflow/` dir, and a DOM clip scan
 * writes `overflow/overflow-report.md`. An overflow pass never touches coupling
 * artifacts and runs only the `main` capture pass (the mock-license/FDA passes
 * are coupling-only). `pnpm i18n:overflow` is just this with `--locale en-XA
 * --build`.
 */
const localeIdx = process.argv.indexOf('--locale')
const captureLocale = localeIdx >= 0 ? process.argv[localeIdx + 1] : 'en'
if (localeIdx >= 0 && (captureLocale === undefined || captureLocale.startsWith('--'))) {
  throw new Error('`--locale` needs a tag, e.g. `--locale en-XA`')
}
const isOverflow = captureLocale !== 'en'
// An explicit socket override (rare); otherwise each pass derives its own unique
// per-launch socket in `launchAndCapture` so a parallel dev/E2E session in
// another worktree can never collide and relaunches don't race a stale bind.
const SOCKET_OVERRIDE = process.env.CMDR_PLAYWRIGHT_SOCKET

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

// A fresh, isolated data dir for the capture run. The app resolves its
// tauri-plugin-store (settings, license, favorites) and all other persisted
// state from `CMDR_DATA_DIR` (see `src-tauri/src/settings/loader.rs` /
// `config.rs`); with no override it falls back to the OS default, which is the
// DEVELOPER'S REAL PROD STORE. Captures must NOT depend on personal settings:
// a translator screenshot is canonical only from DEFAULT settings, and a
// settings-gated surface (the Quick Look hint, suppressed in David's real
// store) must render. So every launch gets the same fresh mktemp dir, removed
// on exit. Created lazily on first launch; reused across the multi-launch
// passes so they share one default-settings baseline.
/** @type {string | null} */
let captureDataDir = null
function ensureCaptureDataDir() {
  if (captureDataDir == null) {
    captureDataDir = mkdtempSync(join(tmpdir(), 'cmdr-i18n-capture-data-'))
    console.log(`[i18n-capture] isolated data dir at ${captureDataDir} (fresh default settings)`)
  }
  return captureDataDir
}
function cleanupCaptureDataDir() {
  if (captureDataDir == null) return
  try {
    rmSync(captureDataDir, { recursive: true, force: true })
  } catch {
    /* best-effort; /tmp self-prunes */
  }
  captureDataDir = null
}
process.on('exit', cleanupCaptureDataDir)

/** @type {import('node:child_process').ChildProcess | null} */
let appProc = null
// Stop ONLY the app process THIS script launched, never a broad
// `pkill -f 'target.*Cmdr'`: that pattern matches any worktree's running Cmdr
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
 * another app is actively foreground, so for clean shots the screen should be
 * idle during a run. We surface the foreign instance rather than hard-failing.
 */
function warnIfForeignCmdr() {
  const res = spawnSync('pgrep', ['-fl', 'target.*Cmdr'], { encoding: 'utf8' })
  // pgrep exits 0 with matches, 1 with none.
  if (res.status === 0 && res.stdout.trim() !== '') {
    console.warn(
      `[i18n-capture] WARNING: another Cmdr is running, so separate-window shots may capture stale frames ` +
        `if the screen isn't idle:\n${res.stdout.trim()}`,
    )
  }
}

async function main() {
  // Coexisting with a running Cmdr is safe (PID-scoped teardown, window-ID-scoped
  // capture); just warn, since a busy screen can spoil separate-window shots.
  warnIfForeignCmdr()

  if (isOverflow) {
    // Generate the target locale BEFORE the build: the frontend's catalog glob
    // (`messages/*/*.json`) is eager and resolved at BUILD time, so the locale dir
    // must exist on disk before the capture binary is compiled or the runtime
    // can't switch to it. Only `en-XA` (the pseudolocale) is generable today.
    if (captureLocale === 'en-XA') {
      console.log(`[i18n-capture] overflow pass: generating ${captureLocale} catalog…`)
      run('node', ['scripts/gen-pseudolocale.js'])
    } else {
      console.log(
        `[i18n-capture] overflow pass in ${captureLocale}: assuming its catalog is already on disk ` +
          `(only en-XA is auto-generated). Build with --build so the glob includes it.`,
      )
    }
  }

  if (wantBuild) {
    console.log('[i18n-capture] building capture binary…')
    // `CMDR_I18N_CAPTURE_BUILD=1` flips the `__CMDR_I18N_CAPTURE__` Vite define so
    // the frontend bundle BAKES IN the capture instrumentation. Only THIS build
    // sets it, so a binary built by the normal E2E lane has no capture API:
    // `pnpm i18n:capture` must always go through `--build`. The env propagates
    // through tauri-wrapper → Tauri → the vite build.
    //
    // The capture build carries EVERY mock/feature at once (the visual UI is
    // identical between them, only the cfg gates flip), so one build reaches all
    // the special surfaces:
    //  - `playwright-e2e`: the capture sink + E2E IPC (always needed).
    //  - `virtual-mtp`: the fake MTP device, so the MTP browse surface + connected
    //    toast are reachable without real hardware.
    //  - `--config profile.release.debug-assertions=true`: turns ON
    //    `#[cfg(debug_assertions)]` for the RELEASE profile, so the
    //    `CMDR_MOCK_LICENSE` / `CMDR_MOCK_FDA` mocks (debug-only) take effect.
    //    A clean, scoped Cargo override that touches only this one build, with no
    //    committed `Cargo.toml` change. Everything after the tauri `--` separator
    //    is forwarded to `cargo`.
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
        'playwright-e2e,virtual-mtp',
        '--config',
        'profile.release.debug-assertions=true',
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

  // The MAIN launch captures every default-launch surface and writes the report
  // fresh. `CMDR_MOCK_FDA=granted` opens the FDA gate so the download teaching
  // toast surfaces (its event bridge bails when the gate is pending); the
  // debug-assertions capture build honors the mock. The virtual MTP device
  // auto-registers under E2E mode (no env needed beyond the feature).
  // `CMDR_I18N_OVERFLOW_LOCALE` (overflow pass only) tells the spec to switch the
  // app to the pseudolocale, redirect screenshots to `overflow/`, and run the
  // clip scan; empty/absent → the normal English coupling capture.
  /** @type {Record<string, string>} */
  const mainEnv = { CMDR_MOCK_FDA: 'granted' }
  if (isOverflow) mainEnv.CMDR_I18N_OVERFLOW_LOCALE = captureLocale
  await launchAndCapture(binary, startPath, mainEnv, 'main')

  // The mock-license / FDA-variant passes are coupling-only (they capture extra
  // surfaces for `@key.screenshot`). An overflow pass only needs the main-pass
  // surfaces rendered in the pseudolocale, so skip them: stop after the main pass.
  if (isOverflow) {
    console.log('[i18n-capture] overflow pass done. See `screenshots/overflow/overflow-report.md` for clip findings.')
    return
  }

  // PER-LAUNCH mock passes. Each carries an env the app reads once at startup,
  // and the spec (keyed by `CMDR_I18N_CAPTURE_PASS`) captures only that pass's
  // surface and MERGES into the report the main pass wrote. The
  // debug-assertions capture build is what makes `CMDR_MOCK_LICENSE` /
  // `CMDR_MOCK_FDA` (both `#[cfg(debug_assertions)]`) take effect in a release
  // binary. `CMDR_MOCK_LICENSE` values per `app_status.rs::get_mock_status`.
  /** @type {{ env: Record<string, string>, label: string }[]} */
  const passes = [
    // License states (paid About, perpetual About, commercial reminder, expired).
    { env: { CMDR_MOCK_LICENSE: 'commercial' }, label: 'license:commercial' },
    { env: { CMDR_MOCK_LICENSE: 'perpetual' }, label: 'license:perpetual' },
    { env: { CMDR_MOCK_LICENSE: 'personal_reminder' }, label: 'license:reminder' },
    { env: { CMDR_MOCK_LICENSE: 'expired' }, label: 'license:expired' },
    // FDA-variant onboarding step 1 (the default macOS launch already grants FDA,
    // so these drive the not-yet-granted / denied banner copy).
    { env: { CMDR_MOCK_FDA: 'notgranted' }, label: 'fda:notgranted' },
    { env: { CMDR_MOCK_FDA: 'denied' }, label: 'fda:denied' },
  ]
  // Run every pass even if one fails: a single broken pass must not abort the
  // others (each pass MERGES into the report, so partial progress is kept). A
  // failed Playwright run throws out of `launchAndCapture`; catch it, record the
  // pass, and continue. We surface the failures (non-zero exit) at the very end.
  const failedPasses = []
  for (const { env: passEnv, label } of passes) {
    try {
      await launchAndCapture(binary, startPath, { ...passEnv, CMDR_I18N_CAPTURE_PASS: label }, label)
    } catch (e) {
      failedPasses.push(label)
      console.warn(`[i18n-capture] pass '${label}' FAILED: ${e instanceof Error ? e.message : String(e)}`)
    }
  }

  if (failedPasses.length > 0) {
    throw new Error(`capture passes failed: ${failedPasses.join(', ')}`)
  }

  console.log('[i18n-capture] done. Next: `pnpm i18n:couple` to write @key.screenshot couplings.')
}

/**
 * Launches the capture binary (with `extraEnv` merged in), waits for its unique
 * socket, runs ONLY the capture spec against it, then stops that app. One launch
 * per pass so a `CMDR_MOCK_LICENSE` state takes effect (it's read once at launch).
 * @param {string} binary
 * @param {string} startPath
 * @param {Record<string, string>} extraEnv
 * @param {string} passLabel
 * @returns {Promise<void>}
 */
async function launchAndCapture(binary, startPath, extraEnv, passLabel) {
  // A per-pass unique socket: the prior app is stopped, but a fresh socket path
  // avoids any stale-bind races across relaunches. An explicit override wins.
  const socket =
    SOCKET_OVERRIDE ??
    `/tmp/tauri-playwright-i18n-${String(process.pid)}-${passLabel.replace(/[^a-z0-9]+/gi, '-')}.sock`

  console.log(`[i18n-capture] launching app (${passLabel})…`)
  appProc = spawn(binary, [], {
    cwd: desktopDir,
    stdio: 'inherit',
    env: {
      ...process.env,
      CMDR_E2E_MODE: '1',
      CMDR_E2E_START_PATH: startPath,
      CMDR_PLAYWRIGHT_SOCKET: socket,
      // Isolated, fresh data dir → default settings, reproducible, never the
      // developer's real prod store. See `ensureCaptureDataDir`.
      CMDR_DATA_DIR: ensureCaptureDataDir(),
      ...extraEnv,
    },
  })
  appProc.on('exit', (code) => {
    if (code != null && code !== 0) console.warn(`[i18n-capture] app (${passLabel}) exited with code ${String(code)}`)
  })

  await waitForSocket(socket, 60000)
  console.log(`[i18n-capture] socket ready (${passLabel}); running capture spec…`)

  // Don't pass `--project tauri` AND a positional spec path: Playwright treats
  // the positional as a project filter when `--project` is set, failing with
  // "Project(s) ... not found". The `i18n-capture` shard's `testMatch` already
  // restricts the run to the capture spec, and the config has only the `tauri`
  // project, so it runs by default. (See the suite CLAUDE.md note on this clash.)
  // Pass the SAME unique socket to Playwright: `fixtures.ts` reads
  // `CMDR_PLAYWRIGHT_SOCKET` to know which socket to connect to. Without this,
  // Playwright connects to the default `/tmp/tauri-playwright.sock` while the app
  // listens on our unique one, and the first `evaluate` hangs to timeout.
  try {
    run('npx', ['playwright', 'test', '--config', 'test/e2e-playwright/playwright.config.ts'], {
      env: {
        ...process.env,
        CMDR_E2E_START_PATH: startPath,
        CMDR_E2E_SHARD_KIND: 'i18n-capture',
        CMDR_PLAYWRIGHT_SOCKET: socket,
        ...extraEnv,
      },
    })
  } finally {
    // Always stop THIS pass's app before the next launch (or before exit).
    killApp()
  }
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
