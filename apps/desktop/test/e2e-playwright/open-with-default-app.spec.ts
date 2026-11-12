/**
 * E2E for handing a file to its default app: Enter and ⌘↓ each launch it ONCE.
 *
 * The count is the whole point, and only a real keypress can pin it. Both combos
 * travel two roads at once — the pane's own keydown handler, and the document-level
 * dispatcher that `nav.open` sits in — so a pane handler that forgets to stop the
 * event opens the file twice. That stayed invisible for months, because a second
 * `open` on a `.txt` just re-focuses the TextEdit window already showing it; a
 * Google Drive file is what exposed it, with two browser tabs for one Enter.
 *
 * The `playwright-e2e` build records the request instead of launching anything
 * (`crate::open_mock`), so the spec reads the count back through `e2e_opened_paths`
 * and nothing lands on the desktop.
 *
 * Each test opens a SECOND file and waits for it, which is what makes "exactly one"
 * assertable without a timer: the second keypress crosses the socket as its own
 * task, so anything the first press still had coming has already landed by then.
 *
 * Fixture (at $CMDR_E2E_START_PATH):
 *   left/                   <- left pane starts here
 *     file-a.txt, file-b.txt, sub-dir/, ...
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { ensureMcpClient, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import {
  clearOpenedPaths,
  dismissAllToasts,
  ensureAppReady,
  ensureExplorerFocused,
  getFixtureRoot,
  getOpenedPaths,
  moveCursorToFile,
  settleFocusedPaneOnLeft,
} from './helpers.js'

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { defaultKey } from './helpers/registry-keys.js'

type PageLike = TauriPage | BrowserPageAdapter

/** Put the cursor on `name`, press `combo`, and wait until that file's open lands. */
async function openFileWith(tauriPage: PageLike, name: string, combo: string): Promise<void> {
  expect(await moveCursorToFile(tauriPage, name)).toBe(true)
  await ensureExplorerFocused(tauriPage)

  await tauriPage.keyboard.press(combo)

  await expect
    .poll(async () => (await getOpenedPaths(tauriPage)).some((p) => p.endsWith(`/${name}`)), {
      timeout: waitBudget(5000),
    })
    .toBeTruthy()
}

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

test.afterEach(() => {
  restoreFixtureTree(getFixtureRoot())
})

test.describe('Open with the default app', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await mcpNavToPath('left', `${getFixtureRoot()}/left`)
    await settleFocusedPaneOnLeft(tauriPage, `${getFixtureRoot()}/left`)
    await clearOpenedPaths(tauriPage)
  })

  test('Enter hands each file to its default app exactly once', async ({ tauriPage }) => {
    await openFileWith(tauriPage, 'file-a.txt', 'Enter')
    await openFileWith(tauriPage, 'file-b.txt', 'Enter')

    // Pre-fix this recorded each path twice: the pane opened the file, then the
    // document dispatcher ran `nav.open`, whose handler re-sent Enter into the pane.
    expect(await getOpenedPaths(tauriPage)).toEqual([
      `${getFixtureRoot()}/left/file-a.txt`,
      `${getFixtureRoot()}/left/file-b.txt`,
    ])
    // Each open shows an "Opening <file>…" toast, which the leak guard counts.
    await dismissAllToasts(tauriPage)
  })

  test("Open's second key hands each file to its default app exactly once", async ({ tauriPage }) => {
    // `nav.open`'s second default: ⌘↓ on macOS, Total Commander's `Ctrl+PageDown` elsewhere.
    const openKey = defaultKey('nav.open', 1)
    await openFileWith(tauriPage, 'file-a.txt', openKey)
    await openFileWith(tauriPage, 'file-b.txt', openKey)

    expect(await getOpenedPaths(tauriPage)).toEqual([
      `${getFixtureRoot()}/left/file-a.txt`,
      `${getFixtureRoot()}/left/file-b.txt`,
    ])
    await dismissAllToasts(tauriPage)
  })
})
