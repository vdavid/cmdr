/**
 * Linux: a dialog gets the `Alt+<letter>` keys the menu bar's mnemonics would take.
 *
 * GTK takes a bar title's `Alt+<letter>` (English: F, E, S, V, G, T, H) before WebKit sees the
 * keydown, so `preventDefault` can't help. While a dialog, an overlay, or the palette is up, the main
 * window's mnemonic modifier is a mask nobody presses (`src-tauri/src/menu/mnemonic_mask.rs`), and
 * Alt again once the last one closes.
 *
 * Real X key events only (`helpers/x11-keys.ts`): `pressKey`'s synthetic DOM event never reaches
 * GTK, so it would pass with or without the mask. An open GTK menu is a new visible top-level X
 * window, which is how the spec sees it. One language proves it: the mask doesn't read labels.
 */
import os from 'node:os'
import { test, expect } from './fixtures.js'
import { ensureAppReady, pressKey } from './helpers.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import { focusMainWindow, pressRealKey, visibleTopLevelWindows } from './helpers/x11-keys.js'
import { closeSearchDialog, getActiveMode, openSearchDialog, pollActiveMode, SEARCH_INPUT } from './search-helpers.js'
import { waitBudget } from './wait-budget.js'

/** The X windows that appeared since `before`: an open menu is one. */
function newWindowsSince(before: readonly string[]): string[] {
  return visibleTopLevelWindows().filter((id) => !before.includes(id))
}

test.describe('Menu bar mnemonics while a dialog is up (Linux)', () => {
  test.skip(os.platform() !== 'linux', 'Only the Linux bar has mnemonics')

  test('a real Alt+F reaches the search dialog, and opens the File menu once it closes', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const atStart = visibleTopLevelWindows()
    try {
      await openSearchDialog(tauriPage)
      await tauriPage.evaluate(`(function(){
          var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
          if (el) el.focus();
      })()`)
      await tauriPage.waitForFunction(
        `document.activeElement === document.querySelector(${JSON.stringify(SEARCH_INPUT)})`,
        waitBudget(3000),
      )
      // Start from Regex, so Filename mode can only come from the real Alt+F.
      await pressKey(tauriPage, 'Alt+r', 'KeyR')
      expect(await pollActiveMode(tauriPage, 'regex')).toBe(true)

      focusMainWindow()
      pressRealKey('alt+f')
      const switched = await pollActiveMode(tauriPage, 'filename', waitBudget(3000))
      expect({ switched, menusOpened: newWindowsSince(atStart) }).toEqual({ switched: true, menusOpened: [] })

      await closeSearchDialog(tauriPage)
      expect(await getActiveMode(tauriPage)).toBeNull()

      focusMainWindow()
      pressRealKey('alt+f')
      await expect.poll(() => newWindowsSince(atStart).length, { timeout: waitBudget(3000) }).toBeGreaterThan(0)

      pressRealKey('Escape')
      await expect.poll(() => newWindowsSince(atStart), { timeout: waitBudget(3000) }).toEqual([])
    } finally {
      // A failed run can leave the File menu open (it grabs the keyboard) or the dialog up.
      if (newWindowsSince(atStart).length > 0) pressRealKey('Escape')
      if ((await getActiveMode(tauriPage)) !== null) await closeSearchDialog(tauriPage)
    }
  })
})
