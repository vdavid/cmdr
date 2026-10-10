/**
 * Acceptance test for the favorites menu (⌃D): open it on the focused pane, press a
 * number, and the pane is there. That's the whole promise of GitHub #91, and it's the one
 * path no unit test can prove — it crosses the command registry, the house `Menu`'s
 * accelerator matching, `resolve_path_to_volume` in Rust, and a real navigation.
 *
 * Everything else the menu does (the `0` row's three states, the swap keys, the switcher's
 * favorites section, reorder, rename, analytics) is pinned in
 * `src/lib/file-explorer/navigation/FavoritesMenu.svelte.test.ts` and its controller
 * sibling, which drive the same surfaces far more cheaply.
 *
 * ❗ The test adds its OWN favorite in a fixture directory and presses the digit THAT row
 * carries, rather than trusting the four platform defaults the store seeds on first launch:
 * those point at the machine's real home folders, so a fixed digit would mean a different
 * place (and a listing of somebody's Desktop) depending on where the suite runs. It's
 * removed again in the `finally`, so the store is left as it was found.
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import {
  CTRL_OR_META,
  dispatchMenuCommand,
  ensureAppReady,
  escapeOverlayUntilGone,
  getFixtureRoot,
  getFocusedPaneActiveTabPath,
  pressKey,
} from './helpers.js'
import { ensureMcpClient, mcpCall, mcpNavToPath, mcpReadResource } from '../e2e-shared/mcp-client.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const MENU = '[data-menu]'
const FAVORITE_NAME = 'E2E favorite'

/** The favorites as the backend holds them, in store order: the order the menu numbers. */
async function readFavorites(): Promise<{ id: string; name: string; path: string }[]> {
  const state = await mcpReadResource('cmdr://state?include=favorites')
  const rows: { id: string; name: string; path: string }[] = []
  // `  - id: <id>` then `    name: "..."` then `    path: "..."`, the shape
  // `mcp/resources/mod.rs` writes.
  const pattern = /^\s+- id: (\S+)\n\s+name: "([^"]*)"\n\s+path: "([^"]*)"$/gm
  for (const match of state.matchAll(pattern)) {
    rows.push({ id: match[1], name: match[2], path: match[3] })
  }
  return rows
}

/**
 * Opens the favorites menu the way a person does.
 *
 * ❗ ⌃D only on macOS. `toPlatformShortcut` folds both ⌘ and ⌃ onto `Ctrl` off macOS, so
 * on Linux Ctrl+D is Duplicate's combo too and the more specific scope wins it — pressing
 * it here would copy a fixture file and fail the dirty-tree guard. The command dispatch is
 * the same path ⌃D reaches, so Linux takes that half and macOS proves the key.
 */
async function openFavoritesMenu(tauriPage: PageLike): Promise<void> {
  if (CTRL_OR_META === 'Meta') await pressKey(tauriPage, 'Control+d')
  else await dispatchMenuCommand(tauriPage, 'favorites.open')
  await tauriPage.waitForSelector(`${MENU} [data-accelerator="1"]`, 5000)
}

/** The digit the row with this label carries, or null while no such row is listed. */
async function digitOfFavorite(tauriPage: PageLike, name: string): Promise<string | null> {
  return tauriPage.evaluate<string | null>(`(function(){
      var rows = document.querySelectorAll(${JSON.stringify(`${MENU} [data-menu-section="favorites"] [data-menu-row]`)});
      for (var i = 0; i < rows.length; i++) {
        var label = rows[i].querySelector('.favorite-label');
        if (label && label.textContent === ${JSON.stringify(name)}) return rows[i].getAttribute('data-accelerator');
      }
      return null;
  })()`)
}

test.describe('Favorites menu (⌃D)', () => {
  test('pressing a number key jumps the focused pane to that favorite', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    const fixtureRoot = getFixtureRoot()
    const startPath = `${fixtureRoot}/left`
    // A real directory in the fixture tree, and not where the pane already is: landing
    // there is the proof.
    const target = `${fixtureRoot}/left/sub-dir`
    let addedId: string | null = null

    try {
      await mcpCall('favorites', { action: 'add', path: target, name: FAVORITE_NAME })
      addedId = (await readFavorites()).find((favorite) => favorite.name === FAVORITE_NAME)?.id ?? null
      expect(addedId, 'the favorite the test adds should be in the store').not.toBeNull()

      await openFavoritesMenu(tauriPage)
      // Ask the MENU which digit its own row carries, rather than counting positions: the
      // store seeds four platform defaults, and how many of those survive (the volume list
      // drops a favorite whose path is gone) differs per machine.
      //
      // ❗ Polled, ❌ read once: the favorite being in the store is not the menu showing it.
      // The frontend volume store rebuilds off the `volumes-changed` the write emits, which
      // lands a beat later. The menu is reactive, so it catches up while it's open — a
      // straight read right after opening sees the list from before the add.
      await expect
        .poll(async () => digitOfFavorite(tauriPage, FAVORITE_NAME), { timeout: waitBudget(5000) })
        .toMatch(/^[1-9]$/)
      // The poll above already failed the test if there's no such row or no digit on it.
      const digit = (await digitOfFavorite(tauriPage, FAVORITE_NAME)) ?? ''

      // ❗ With the `code`: the accelerator matcher reads the PHYSICAL key, not `event.key`,
      // which is what makes the digits work on a layout where they need Shift.
      await pressKey(tauriPage, digit, `Digit${digit}`)

      // A pick closes the menu, and the pane lands in the favorite's folder — on the
      // volume that CONTAINS it, so the listing is a real one.
      await expect.poll(async () => tauriPage.count(MENU), { timeout: waitBudget(3000) }).toBe(0)
      await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: waitBudget(5000) }).toBe(target)
    } finally {
      // Leave the store and the panes as they were found: an overlay or a moved pane
      // fails the global leak guard, and a stray favorite would renumber the next run.
      if ((await tauriPage.count(MENU)) > 0) await escapeOverlayUntilGone(tauriPage, MENU)
      if (addedId !== null) await mcpCall('favorites', { action: 'remove', id: addedId }).catch(() => {})
      await mcpNavToPath('left', startPath).catch(() => {})
    }
  })
})
