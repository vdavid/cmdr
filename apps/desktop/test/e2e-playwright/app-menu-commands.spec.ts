/**
 * E2E for the app menu's commands per OS, read off the running app's own capability answer
 * (`appMenu`), so the same spec pins macOS and Linux with no platform check:
 *
 * - Where an app menu owns Quit (macOS), the palette has no Quit row: AppKit's item runs it.
 * - Without one (Linux, and Windows once it runs), Quit is Cmdr's own command, in the palette.
 * - Toggle full screen is a palette row on every OS that offers full screen.
 *
 * **Why no spec runs Quit.** The suite shares one app process across specs, so a real quit takes
 * every later spec down (`quit-gate.spec.ts` says the same). The road Quit takes is pinned below
 * the webview: the handler calls one backend command (`command-dispatch.characterization.test.ts`),
 * which is the same function closing the main window calls, held while the gate holds
 * (`src-tauri/src/quit/tests.rs`).
 *
 * **Why no spec presses F11.** The Linux lane's Xvfb has no window manager, and GTK's full screen
 * is a request the window manager grants, so the window never changes and nothing could be read
 * back. The toggle is pinned in `src/routes/(main)/full-screen.test.ts`.
 *
 * Requires `--features playwright-e2e`.
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { ensureAppReady, escapeOverlayUntilGone } from './helpers.js'
import { CTRL_OR_META, type PageLike } from './helpers/core.js'

const PALETTE = '.palette-overlay'
const PALETTE_INPUT = '.palette-overlay input.text-field-control'

async function openCommandPalette(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'p', ctrlKey: ${String(CTRL_OR_META === 'Control')}, metaKey: ${String(CTRL_OR_META === 'Meta')}, shiftKey: true, bubbles: true
    }))`)
  await tauriPage.waitForSelector(PALETTE, 3000)
}

/** The palette rows' text for `query`, once the list has caught up with it. */
async function paletteRowsFor(tauriPage: PageLike, query: string): Promise<string[]> {
  await tauriPage.fill(PALETTE_INPUT, query)
  // Let the filter run before reading: the list renders on the next frame.
  await expect
    .poll(() => tauriPage.evaluate(`document.querySelector('${PALETTE_INPUT}').value`), { timeout: waitBudget(3000) })
    .toBe(query)
  const rows = await tauriPage.evaluate(`JSON.stringify(Array.from(
    document.querySelectorAll('${PALETTE} .result-item'), function (row) { return row.textContent.trim() }))`)
  return JSON.parse(rows as string) as string[]
}

async function hasAppMenu(tauriPage: PageLike): Promise<boolean> {
  return (await tauriPage.evaluate(`window.__CMDR_CAPABILITIES__.appMenu === true`)) === true
}

test.describe('App menu commands', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
  })

  test('Quit is a palette row exactly where no app menu owns it', async ({ tauriPage }) => {
    const appMenu = await hasAppMenu(tauriPage)
    await openCommandPalette(tauriPage)
    try {
      await expect
        .poll(async () => (await paletteRowsFor(tauriPage, 'quit cmdr')).some((row) => row.includes('Quit Cmdr')), {
          timeout: waitBudget(3000),
        })
        .toBe(!appMenu)
    } finally {
      await escapeOverlayUntilGone(tauriPage, PALETTE)
    }
  })

  test('Toggle full screen is a palette row', async ({ tauriPage }) => {
    await openCommandPalette(tauriPage)
    try {
      await expect
        .poll(async () => (await paletteRowsFor(tauriPage, 'full screen')).some((row) => row.includes('full screen')), {
          timeout: waitBudget(3000),
        })
        .toBe(true)
    } finally {
      await escapeOverlayUntilGone(tauriPage, PALETTE)
    }
  })
})
