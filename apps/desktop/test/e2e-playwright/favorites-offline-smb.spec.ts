/**
 * Acceptance test for a favorite on an SMB share that's offline: it stays in the
 * favorites menu (dimmed, still pickable), and picking it mounts the share and lands in
 * the folder.
 *
 * The path no unit test can prove: the backend's reach pass reading the share's rows
 * (`favorites/reach.rs`), the menu rendering the row quiet, `open-favorite.ts` entering
 * the saved place by its id (❌ never by path: an unmounted share's path resolves onto the
 * boot disk), and `place-connect` dialing the real Samba fixture and entering the folder.
 *
 * ❗ The pick test also watches the pane while it connects: the connecting view may show,
 * but no error pane may flash before the folder lands ("path not found" over a share
 * that's still mounting).
 *
 * Linux only, like `smb.spec.ts`: macOS SMB mounting needs a permission prompt nobody
 * can approve headless.
 */

import fs from 'fs'
import os from 'os'
import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import {
  setupSmb,
  teardownSmb,
  guestMountPoint,
  requireGuestSuite,
  unmountSmbShares,
  SMB_E2E_SUITE_DIR,
  SMB_GUEST_SHARE,
} from '../e2e-shared/smb-fixtures.js'
import { initMcpClient, mcpCall, mcpReadResource, mcpSelectVolume, mcpAwaitPath } from '../e2e-shared/mcp-client.js'
import { dispatchMenuCommand, ensureAppReady, escapeOverlayUntilGone, focusPane, pointerClick } from './helpers.js'

test.setTimeout(waitBudget(180_000))

/** Name of the root/local volume (differs by platform). */
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

const MENU = '[data-menu]'
const FAVORITE_NAME = 'E2E offline share favorite'
const FOLDER = 'favorite-target'
const GUEST_HOST_NAME = 'SMB Test (Guest)'

type PageLike = Parameters<typeof ensureAppReady>[0]

/** One `favorites:` entry of `cmdr://state`, the shape `mcp/resources/favorites.rs` writes. */
interface StateFavorite {
  id: string
  name: string
  path: string
  volume: string | null
  reach: string | null
}

async function readFavorites(): Promise<StateFavorite[]> {
  const state = await mcpReadResource('cmdr://state?include=favorites')
  const section = state.split(/^favorites:/m)[1] ?? ''
  return section
    .split(/^\s+- id: /m)
    .slice(1)
    .map((block) => ({
      id: block.split('\n')[0].trim(),
      name: /name: "([^"]*)"/.exec(block)?.[1] ?? '',
      path: /path: "([^"]*)"/.exec(block)?.[1] ?? '',
      volume: /volume: (\S+)/.exec(block)?.[1] ?? null,
      reach: /reach: (\S+)/.exec(block)?.[1] ?? null,
    }))
}

async function ourFavorite(): Promise<StateFavorite | undefined> {
  return (await readFavorites()).find((favorite) => favorite.name === FAVORITE_NAME)
}

/** The favorites menu row carrying this label, as an element expression for `pointerClick` / reads. */
const ROW_EXPR = `(function(){
    var rows = document.querySelectorAll('${MENU} [data-menu-section="favorites"] [data-menu-row]');
    for (var i = 0; i < rows.length; i++) {
      var label = rows[i].querySelector('.favorite-label');
      if (label && label.textContent === ${JSON.stringify(FAVORITE_NAME)}) return rows[i];
    }
    return null;
})()`

/** Mounts the guest share THROUGH Cmdr, so the share is saved and can dial again once it's gone. */
async function mountGuestShareThroughCmdr(): Promise<string> {
  await mcpSelectVolume('left', 'Servers')
  await expect
    .poll(async () => (await mcpReadResource('cmdr://state')).includes(`${GUEST_HOST_NAME}  protocol=`), {
      timeout: waitBudget(15000),
    })
    .toBeTruthy()
  await mcpCall('move_cursor', { pane: 'left', filename: GUEST_HOST_NAME })
  await mcpCall('open_under_cursor', {})
  await expect
    .poll(async () => (await mcpReadResource('cmdr://state')).includes(SMB_GUEST_SHARE), {
      timeout: waitBudget(30000),
    })
    .toBeTruthy()
  await mcpCall('move_cursor', { pane: 'left', filename: SMB_GUEST_SHARE })
  await mcpCall('open_under_cursor', {})
  await expect.poll(() => guestMountPoint(), { timeout: waitBudget(30000) }).not.toBeNull()
  const mount = guestMountPoint() ?? ''
  await mcpAwaitPath('left', mount, 30)
  return mount
}

/**
 * Mounts the guest share through Cmdr, favorites a folder on it, checks the favorite names
 * the SHARE's volume, then takes the share away from outside, the way the file manager or a
 * dropped network does. Hands the favorite's id to `onFavorite` for the cleanup.
 *
 * ❗ From outside, ❌ not Cmdr's eject: that reads a GVFS share as "no longer mounted" (it
 * isn't in the mount table) and does nothing.
 */
async function favoriteAFolderThenLoseTheShare(onFavorite: (id: string) => void): Promise<void> {
  await mountGuestShareThroughCmdr()
  const suite = await requireGuestSuite()
  const folder = `${suite}/${FOLDER}`
  fs.mkdirSync(folder, { recursive: true })
  await expect.poll(() => fs.existsSync(folder), { timeout: waitBudget(10000) }).toBeTruthy()

  await mcpCall('favorites', { action: 'add', path: folder, name: FAVORITE_NAME })
  await expect.poll(async () => (await ourFavorite())?.reach, { timeout: waitBudget(10000) }).toBe('ready')
  const added = await ourFavorite()
  if (added) onFavorite(added.id)
  // The share's own id, ❌ not the GVFS FUSE root every share sits under.
  expect(added?.volume, 'the favorite names the share it lives on').toMatch(/^smb-/)

  await mcpSelectVolume('left', LOCAL_VOLUME_NAME)
  unmountSmbShares()
  await expect.poll(() => guestMountPoint(), { timeout: waitBudget(30000) }).toBeNull()
}

/**
 * Asks the backend to re-broadcast the volume list, so the frontend's copy (what the menu
 * renders) catches up with the unmount.
 *
 * ❗ Needed on Linux only because the GVFS watcher's inotify sees nothing inside the FUSE
 * mount in the Docker lane, so an unmount from outside Cmdr pushes no `volumes-changed`.
 * `cmdr://state` lists fresh, which is why the reach reads right there before the menu does.
 */
async function refreshVolumeList(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('refresh_volumes')`)
}

/** Opens the left pane's favorites menu and waits until our row reads dimmed. */
async function expectOurRowDimmed(tauriPage: PageLike): Promise<void> {
  await focusPane(tauriPage, 0)
  await dispatchMenuCommand(tauriPage, 'favorites.open')
  await tauriPage.waitForSelector(`${MENU} [data-menu-section="favorites"]`, 5000)
  // The label's classes (or what the menu holds instead), so a miss says what it saw.
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<string>(
          `(function(){
              var row = ${ROW_EXPR};
              if (!row) {
                var labels = document.querySelectorAll('${MENU} .favorite-label');
                return 'no row; labels: ' + Array.prototype.map.call(labels, function(l){ return l.textContent; }).join(' | ');
              }
              return row.querySelector('.favorite-label').className + ' / ' + (row.getAttribute('title') || row.getAttribute('aria-description') || '');
          })()`,
        ),
      { timeout: waitBudget(10000) },
    )
    .toContain('is-unreachable')
}

// eslint-disable-next-line @typescript-eslint/unbound-method -- conditional skip
const describeSmb = process.platform === 'darwin' ? test.describe.skip : test.describe

describeSmb('A favorite on an offline SMB share', () => {
  let favoriteId: string | null = null

  test.beforeAll(() => {
    setupSmb()
  })

  test.afterAll(() => {
    teardownSmb()
  })

  test.afterEach(async ({ tauriPage }) => {
    if ((await tauriPage.count(MENU)) > 0) await escapeOverlayUntilGone(tauriPage, MENU)
    if (favoriteId !== null) await mcpCall('favorites', { action: 'remove', id: favoriteId }).catch(() => {})
    favoriteId = null
    const mount = guestMountPoint()
    if (mount) fs.rmSync(`${mount}/${SMB_E2E_SUITE_DIR}/${FOLDER}`, { recursive: true, force: true })
    await mcpSelectVolume('left', LOCAL_VOLUME_NAME).catch(() => {})
  })

  test('stays in the menu, dimmed, once its share is gone', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await initMcpClient(tauriPage)
    await favoriteAFolderThenLoseTheShare((id) => (favoriteId = id))

    // Listed, and a pick can't open it right away: `connects` for a saved share, `forgotten`
    // while the share's direct session is still registered. On Linux nothing notices a GVFS
    // unmount from outside (the watcher's inotify sees nothing inside the FUSE mount), so the
    // registered volume hides the saved row and the favorite reads `forgotten` (#348).
    await expect
      .poll(async () => (await ourFavorite())?.reach, { timeout: waitBudget(15000) })
      .toMatch(/^(connects|forgotten)$/)
    await refreshVolumeList(tauriPage)
    await expectOurRowDimmed(tauriPage)
  })

  test('picking it mounts the share and lands in the folder, with no error flash', async ({ tauriPage }) => {
    // ❗ A GVFS share saves its place now, but an unmount from outside Cmdr leaves its direct
    // session registered (no watcher event), and a registered volume hides the saved row
    // (`server_volumes::fold_saved_smb_shares`), so the favorite reads `forgotten`. Drop this
    // once Linux notices a GVFS unmount (#348).
    test.fixme(process.platform === 'linux', 'a GVFS unmount from outside Cmdr goes unnoticed (#348)')
    await ensureAppReady(tauriPage)
    await initMcpClient(tauriPage)
    await favoriteAFolderThenLoseTheShare((id) => (favoriteId = id))
    await expect.poll(async () => (await ourFavorite())?.reach, { timeout: waitBudget(15000) }).toBe('connects')
    await refreshVolumeList(tauriPage)
    await expectOurRowDimmed(tauriPage)

    // Watch the left pane from just before the pick until it lands.
    await tauriPage.evaluate(`(function(){
        var pane = document.querySelectorAll('.file-pane')[0];
        var seen = { error: false, connecting: false };
        window.__e2eFavoriteWatch = seen;
        var look = function(){
          if (pane.querySelector('.error-pane')) seen.error = true;
          if (pane.querySelector('.remote-connect')) seen.connecting = true;
        };
        var observer = new MutationObserver(look);
        observer.observe(pane, { subtree: true, childList: true });
        window.__e2eFavoriteWatchStop = function(){ observer.disconnect(); };
    })()`)

    expect(await pointerClick(tauriPage, ROW_EXPR)).toBe('clicked')
    await expect.poll(async () => tauriPage.count(MENU), { timeout: waitBudget(3000) }).toBe(0)

    // Mounted again, and the pane is IN the folder.
    await expect.poll(() => guestMountPoint(), { timeout: waitBudget(45000) }).not.toBeNull()
    await mcpAwaitPath('left', `/${FOLDER}`, 45)
    const watched = await tauriPage.evaluate<{ error: boolean; connecting: boolean }>(`(function(){
        window.__e2eFavoriteWatchStop();
        return window.__e2eFavoriteWatch;
    })()`)
    expect(watched.error, 'no error pane flashed before the folder landed').toBe(false)
  })
})
