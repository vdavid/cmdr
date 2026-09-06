/**
 * Shared helpers for the `archive-*.spec.ts` Playwright specs.
 *
 * `archive-browsing.spec.ts` (reading an archive: Enter policy, listing,
 * preview, extract-out, the read-only formats) and `archive-editing.spec.ts`
 * (rewriting a zip: mkdir, rename, delete, paste, move-out, conflicts) both
 * start from the same place — the focused pane settled on `left/` with the
 * fixture archives visible — and both press Enter on an archive, so the
 * settle-and-enter primitives live here instead of drifting apart in two files.
 */

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { expect } from './fixtures.js'
import {
  flushFileWatcher,
  getFixtureRoot,
  getFocusedPaneActiveTabPath,
  settleFocusedPaneOnLeft,
  moveCursorToFile,
  fileExistsInFocusedPane,
  getOpenedPaths,
  pollUntil,
} from './helpers.js'
import { mcpCall } from '../e2e-shared/mcp-client.js'

/** Union type for tauriPage (works in both Tauri and browser mode). */
export type PageLike = TauriPage | BrowserPageAdapter

/** The Enter-behavior popup (Browse | Open | Configure). */
export const ENTER_MENU = '.menu-content'

/** Navigate a pane to a path via the same `mcp-nav-to-path` event the MCP server uses. */
export async function navigatePaneTo(tauriPage: PageLike, pane: 'left' | 'right', targetPath: string): Promise<void> {
  await tauriPage.evaluate(`(function () {
        window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
            event: 'mcp-nav-to-path',
            payload: { pane: ${JSON.stringify(pane)}, path: ${JSON.stringify(targetPath)} }
        });
    })()`)
}

/**
 * Puts the focused pane back on `left/` and waits until `expectedEntry` is
 * actually listed. Every archive suite opens with this.
 *
 * A prior test may have left the pane browsing INSIDE an archive, and
 * `ensureAppReady` doesn't reliably back out of an archive volume, so each test
 * starts from a known directory.
 *
 * ❗ Flush FIRST rather than only polling. The top-level `beforeEach` wipes and
 * rewrites `left/`, so the path can already read `left/` (a prior test ended
 * here, making the nav a no-op) while the pane still shows the mid-refresh empty
 * view. And `restoreFixtureTree` rewrites `sample.zip` whenever the preceding
 * test mutated it: a remove/create of the same name inside one debounce window
 * is exactly the burst whose diff can land out of order, and the poll then
 * spends its whole budget on a listing that has already stopped moving.
 */
export async function settleOnFixtureLeft(tauriPage: PageLike, expectedEntry: string): Promise<void> {
  const leftDir = `${getFixtureRoot()}/left`
  await navigatePaneTo(tauriPage, 'left', leftDir)
  await settleFocusedPaneOnLeft(tauriPage, leftDir)
  await flushFileWatcher(tauriPage)
  await expect.poll(async () => fileExistsInFocusedPane(tauriPage, expectedEntry), { timeout: 5000 }).toBeTruthy()
}

/**
 * Sets the Enter behavior of one or more formats through the same MCP `set_setting`
 * path the UI uses — one call per format, since each is its own
 * `behavior.archiveEnter.<format>` setting. `set_setting` round-trips, so every
 * setting is live by the time this resolves.
 */
export async function setArchiveEnterBehavior(behavior: Record<string, string>): Promise<void> {
  for (const [format, action] of Object.entries(behavior)) {
    await mcpCall('set_setting', { id: `behavior.archiveEnter.${format}`, value: action })
  }
}

/**
 * Moves the cursor to `name` in the focused pane and presses Enter to open it.
 *
 * The Enter is effect-probed and retried (bounded): the suite's `beforeEach`
 * wipes and recreates `left/` on disk, and the file-watcher's remove/create
 * diffs can drain AFTER `moveCursorToFile` confirmed the cursor — briefly
 * emptying/replacing the listing (the same window `moveCursorToFile`'s own
 * doc describes). An Enter landing in that window is a silent no-op: no path
 * change, no overlay, no error — the long-standing "Enter on an archive did
 * nothing" flake. Each retry re-confirms the cursor is on the target before
 * pressing again, so this never masks a genuinely broken Enter: with the bug
 * present the probe exhausts its retries and fails loudly.
 */
export async function enterEntry(tauriPage: PageLike, name: string): Promise<void> {
  const startPath = await getFocusedPaneActiveTabPath()
  const startOpened = (await getOpenedPaths(tauriPage)).length
  for (let attempt = 0; attempt < 3; attempt++) {
    const found = await moveCursorToFile(tauriPage, name)
    expect(found, `entry "${name}" should be in the focused pane`).toBe(true)
    await tauriPage.keyboard.press('Enter')
    // Probe for ANY effect of the keystroke: an in-place navigation (path
    // change), an opened overlay/menu/dialog, or an external open recorded by
    // the mock (`getOpenedPaths` — the docx default-Open case). No effect
    // within the window ⇒ the keystroke hit the mid-refresh empty listing;
    // re-verify the cursor and press again.
    const hadEffect = await pollUntil(
      tauriPage,
      async () => {
        const path = await getFocusedPaneActiveTabPath()
        if (path !== startPath) return true
        if ((await getOpenedPaths(tauriPage)).length > startOpened) return true
        return tauriPage.evaluate<boolean>(`(function() {
              return !!document.querySelector('.menu-content, .modal-overlay, [role="dialog"], [role="alertdialog"]');
          })()`)
      },
      700,
    )
    if (hadEffect) return
  }
  throw new Error(
    `enterEntry: Enter on "${name}" produced no effect after 3 attempts (path stayed ${startPath ?? '<unknown>'})`,
  )
}
