/**
 * Playwright test fixtures for Cmdr E2E testing.
 *
 * Uses tauri-playwright in Tauri mode: the test runner communicates with
 * the real Tauri app via a Unix socket, and commands are injected directly
 * into the webview via `webview.eval()`. No WebDriver, no HTTP server.
 *
 * Fixture lifecycle:
 * - globalSetup: creates the fixture directory tree (~170 MB)
 * - beforeEach: recreates small text files (keeps bulk .dat files)
 * - globalTeardown: deletes the fixture directory
 *
 * Window-title decoration:
 * - `beforeEach` sets the main window's OS title to "<base> (Running: <test>)"
 * - `afterEach` updates it to "<base> (Running: <test>) (FINISHED)"
 *   so you can glance at the dock / Cmd-Tab / Linux title bar to see which
 *   spec is in flight (or stuck) without tailing the log.
 */

import { createTauriTest } from '@srsholmes/tauri-playwright'
import type { TestInfo } from '@playwright/test'

// Each parallel E2E shard spawns its own Tauri instance bound to a distinct
// Unix socket. The Go check runner sets CMDR_PLAYWRIGHT_SOCKET per shard.
const socketPath = process.env.CMDR_PLAYWRIGHT_SOCKET ?? '/tmp/tauri-playwright.sock'

export const { test, expect } = createTauriTest({
  // No devUrl: in Tauri mode, the app is already running with its built
  // frontend. Setting devUrl would redirect the webview to a nonexistent
  // dev server. devUrl is only used in browser mode (not applicable here).
  devUrl: '',

  // Tauri mode config
  mcpSocket: socketPath,
})

// Captured once per worker on the first beforeEach so suffixes don't accumulate
// across tests. Each shard owns its own Tauri instance + its own worker process,
// so this lives correctly per-shard.
let baseTitle: string | null = null

type EvaluatablePage = { evaluate: (js: string) => Promise<unknown> }

/** Joins describe blocks + test title into "Section > test name" style. */
function formatTestName(info: TestInfo): string {
  const parts = info.titlePath
  const fileIdx = parts.findIndex((p) => /\.spec\.[tj]s$/.test(p))
  const tail = fileIdx >= 0 ? parts.slice(fileIdx + 1) : [info.title]
  return tail.filter((p) => p.length > 0).join(' › ')
}

async function readMainTitle(tauriPage: EvaluatablePage): Promise<string> {
  const result = await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|title', { label: 'main' })`)
  return typeof result === 'string' ? result : ''
}

async function setMainTitle(tauriPage: EvaluatablePage, title: string): Promise<void> {
  await tauriPage.evaluate(
    `window.__TAURI_INTERNALS__.invoke('plugin:window|set_title', { label: 'main', value: ${JSON.stringify(title)} })`,
  )
}

test.beforeEach(async ({ tauriPage }, testInfo) => {
  try {
    if (baseTitle === null) baseTitle = await readMainTitle(tauriPage)
    await setMainTitle(tauriPage, `${baseTitle} (Running: ${formatTestName(testInfo)})`)
  } catch {
    // Title decoration is purely for human eyeballs — never block a test on it.
  }
})

test.afterEach(async ({ tauriPage }, testInfo) => {
  try {
    if (baseTitle === null) baseTitle = await readMainTitle(tauriPage)
    await setMainTitle(tauriPage, `${baseTitle} (Running: ${formatTestName(testInfo)}) (FINISHED)`)
  } catch {
    // See beforeEach.
  }
})
