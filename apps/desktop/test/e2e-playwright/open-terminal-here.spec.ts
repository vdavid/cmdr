/**
 * E2E for "Open terminal here" (`file.openTerminalHere`).
 *
 * The whole point of the command is a side effect outside the app, so the
 * `playwright-e2e` build records the folder instead of launching a terminal
 * (`crate::open_mock`, the same store `open_path` and `open_in_editor` use) and
 * the spec reads it back through `e2e_opened_paths`. Nothing launches, so a suite
 * run leaves no terminal windows behind.
 *
 * What's pinned here is the folder RESOLUTION end to end, which is the part a
 * unit test can't reach: the cursor sitting on a folder, on a file, and on `..`
 * each answer differently, and only a real pane can say which.
 *
 * Fixture (at $CMDR_E2E_START_PATH):
 *   left/                   <- left pane starts here
 *     file-a.txt, file-b.txt, sub-dir/, bulk/, ...
 *   right/                  <- empty
 */

import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { ensureMcpClient, mcpCall, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import {
  clearOpenedPaths,
  dispatchMenuCommand,
  ensureAppReady,
  getFixtureRoot,
  getOpenedPaths,
  moveCursorToFile,
  settleFocusedPaneOnLeft,
} from './helpers.js'

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

/**
 * Spend the one-time hint flag before every test. Whether it would fire depends
 * on which terminals happen to be installed on the machine running the suite, so
 * leaving it unspent would make these tests pass or fail by luck. The picker's
 * own decision table is pinned in `src/lib/open-terminal/first-use-pick.test.ts`.
 */
async function silenceFirstUseHint(): Promise<void> {
  await mcpCall('set_setting', { id: 'behavior.openTerminalHereToastSeen', value: true })
}

/** The folder the command handed to the launcher, once one lands. */
async function openedFolder(tauriPage: PageLike): Promise<string | undefined> {
  const paths = await getOpenedPaths(tauriPage)
  return paths[paths.length - 1]
}

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

test.afterEach(() => {
  restoreFixtureTree(getFixtureRoot())
})

test.describe('Open terminal here', () => {
  // macOS only: `open_terminal_here` and `list_terminal_apps` are
  // `#[cfg(target_os = "macos")]` and aren't registered at all on Linux
  // (`src-tauri/src/ipc.rs`), so every dispatch here would sit until it timed out.
  test.skip(process.platform !== 'darwin', 'Open terminal here is implemented on macOS only.')

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await silenceFirstUseHint()
    await mcpNavToPath('left', `${getFixtureRoot()}/left`)
    await settleFocusedPaneOnLeft(tauriPage, `${getFixtureRoot()}/left`)
    await clearOpenedPaths(tauriPage)
  })

  test('opens the folder under the cursor', async ({ tauriPage }) => {
    expect(await moveCursorToFile(tauriPage, 'sub-dir')).toBe(true)

    await dispatchMenuCommand(tauriPage, 'file.openTerminalHere')

    await expect.poll(async () => openedFolder(tauriPage), { timeout: 5000 }).toBe(`${getFixtureRoot()}/left/sub-dir`)
  })

  test("opens the pane's own folder when the cursor sits on a file", async ({ tauriPage }) => {
    expect(await moveCursorToFile(tauriPage, 'file-a.txt')).toBe(true)

    await dispatchMenuCommand(tauriPage, 'file.openTerminalHere')

    await expect.poll(async () => openedFolder(tauriPage), { timeout: 5000 }).toBe(`${getFixtureRoot()}/left`)
  })

  test("opens the pane's own folder from the `..` row, not the parent", async ({ tauriPage }) => {
    // Standing on `..` means "I'm looking at this folder", the same reading
    // Copy path takes. The parent would be a surprise.
    await mcpCall('move_cursor', { pane: 'left', index: 0 })

    await dispatchMenuCommand(tauriPage, 'file.openTerminalHere')

    await expect.poll(async () => openedFolder(tauriPage), { timeout: 5000 }).toBe(`${getFixtureRoot()}/left`)
  })
})
