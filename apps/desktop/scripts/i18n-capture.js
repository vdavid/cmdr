#!/usr/bin/env node
/**
 * One-shot orchestrator for the i18n screenshot-capture loop.
 *
 * Mirrors the manual single-spec recipe (see `test/e2e-playwright/DETAILS.md`)
 * but wraps the whole lifecycle so capture is a single command:
 *   1. (optional `--build`) compile the Tauri binary with the `playwright-e2e`
 *      feature, the same build the E2E suite uses.
 *   2. create a fresh fixture tree.
 *   3. launch the binary (E2E mode, capture-friendly) and wait for its
 *      playwright socket.
 *   4. run ONLY `i18n-capture.spec.ts` (via the `i18n-capture` shard kind),
 *      which drives the surfaces, records keys, and writes the screenshots +
 *      `screenshots/capture-report.json`.
 *   5. kill the app (always, even on failure).
 *
 * Then run `pnpm i18n:couple` to write the `@key.screenshot` couplings.
 *
 * Usage:
 *   pnpm i18n:capture            # reuse the existing build
 *   pnpm i18n:capture --build    # rebuild the E2E binary first
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
const SOCKET = process.env.CMDR_PLAYWRIGHT_SOCKET ?? '/tmp/tauri-playwright.sock'

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

let appProc = null
function killApp() {
  try {
    spawnSync('pkill', ['-f', 'target.*Cmdr'])
  } catch {
    /* best effort */
  }
}
process.on('exit', killApp)
process.on('SIGINT', () => {
  killApp()
  process.exit(130)
})

async function main() {
  if (wantBuild) {
    console.log('[i18n-capture] building E2E binary…')
    run('node', [
      'scripts/tauri-wrapper.js',
      'build',
      '--no-bundle',
      '--target',
      hostTriple(),
      '--',
      '--features',
      'playwright-e2e',
    ])
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

  // Make sure no stale app holds the socket.
  killApp()

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
  run('npx', ['playwright', 'test', '--config', 'test/e2e-playwright/playwright.config.ts'], {
    env: { ...process.env, CMDR_E2E_START_PATH: startPath, CMDR_E2E_SHARD_KIND: 'i18n-capture' },
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
