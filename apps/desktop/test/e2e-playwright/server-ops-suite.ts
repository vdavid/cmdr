/**
 * The server scenarios both `server-ops-*.spec.ts` files run, one per protocol:
 * a real fixture server added through the sheet, browsed, written to both ways,
 * and let go of, all through the UI.
 *
 * What only an E2E covers here is the whole wire at once: the Go menu's
 * command opening the ONE sheet, the sheet's rounds (host key, then password),
 * the backend registering and saving the place, the switcher offering it, a
 * pane listing it, and the transfer machinery moving real bytes to and from a
 * real server. The Rust integration lane covers each backend verb in depth; the
 * unit tests cover the sheet's state machine. This is the part neither sees.
 */

import { waitBudget } from './wait-budget.js'
import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { initMcpClient } from '../e2e-shared/mcp-client.js'
import { serverFixture, uniqueScratchDir, type ServerProtocol } from '../e2e-shared/server-fixtures.js'
import {
  dismissAllToasts,
  drainOperations,
  ensureAppReady,
  getFixtureRoot,
  moveCursorToFile,
  pointerClick,
  setRenameInput,
  TRANSFER_DIALOG,
  waitForTransferUiToSettle,
} from './helpers.js'
import {
  clickTransferStart,
  resetRememberedConflictPolicy,
  selectConflictPolicy,
  waitForConflictPolicy,
} from './conflict-helpers.js'
import {
  addServerThroughSheet,
  autoReconnectRow,
  cancelSheet,
  closeSwitcher,
  focusSide,
  forgetServer,
  isChecked,
  listLocal,
  openRowSubmenuWithArrowKeys,
  openServerInPane,
  openSwitcher,
  paneLists,
  paneLocation,
  payload,
  press,
  readLocal,
  registeredVolumeFor,
  requireSavedServer,
  savedServerFor,
  SERVER_PASSWORD,
  SHEET,
  switcherRow,
  type PageLike,
} from './server-ops-helpers.js'

/** A landmark every scratch directory carries, so a pane on it has something to list. */
const LANDMARK = 'landmark.txt'

const DELETE_DIALOG = '[data-dialog-id="delete-confirmation"]'

/**
 * The suite's scratch directory on the server: random, so no other run can pick
 * it, and kept beside the app's data for as long as that app lives.
 *
 * ❗ Kept, because a retry runs in a NEW worker that re-evaluates this module,
 * while the app (shared, and still up) keeps the server the failed worker saved.
 * A fresh name there means forgetting and re-adding the same account under a new
 * root, and the switcher can hold the OLD root for a while after (its list waits
 * on local mount discovery, up to 2 s), so the retry would open a folder the
 * first worker's `afterAll` already deleted.
 */
function scratchDirForThisApp(protocol: ServerProtocol): string {
  const dataDir = process.env.CMDR_DATA_DIR
  if (!dataDir) return uniqueScratchDir(protocol)
  const record = path.join(dataDir, `e2e-server-ops-${protocol}.dir`)
  if (fs.existsSync(record)) return fs.readFileSync(record, 'utf-8').trim()
  const dir = uniqueScratchDir(protocol)
  fs.mkdirSync(dataDir, { recursive: true })
  fs.writeFileSync(record, dir)
  return dir
}

export function defineServerOpsSuite(protocol: ServerProtocol): void {
  const fixture = serverFixture(protocol)
  // On a server other lanes and worktrees share: the backend's scratch helpers
  // use the same idea for the same reason.
  const dir = scratchDirForThisApp(protocol)
  /** Whether a saved server is the one this suite added, rooted at its own directory. */
  const pointsHere = (saved: { address: string; places: { appRoot: string }[] }): boolean =>
    saved.address.includes(dir) || saved.places.some((place) => place.appRoot.includes(dir))

  // Network round-trips on top of the usual UI waits, on both lanes.
  test.describe.configure({ timeout: waitBudget(90_000) })

  test.describe(`${fixture.label} server through the UI`, () => {
    test.beforeAll(() => {
      fixture.mkdir(dir)
      fixture.writeFile(`${dir}/${LANDMARK}`, 'here\n')
    })

    // ❗ Side door only: an `afterAll` gets no `tauriPage` (it's per-test), so the
    // app's half of the teardown is the last test's `finally`. A retry's worker
    // puts the directory back in its own `beforeAll`.
    test.afterAll(() => {
      fixture.remove(dir)
    })

    test.beforeEach(async ({ tauriPage }) => {
      await initMcpClient(tauriPage)
      await ensureAppReady(tauriPage)
      await resetRememberedConflictPolicy(tauriPage)
    })

    // One hook, drain before restore: a restore under a live op deletes its source.
    test.afterEach(async ({ tauriPage }) => {
      await drainOperations(tauriPage)
      await cancelSheet(tauriPage).catch(() => undefined)
      await closeSwitcher(tauriPage).catch(() => undefined)
      restoreFixtureTree(getFixtureRoot())
      await resetRememberedConflictPolicy(tauriPage)
    })

    /**
     * The saved place's volume id, adding the server through the sheet first if
     * nothing has yet.
     *
     * ❗ A saved server counts only when it points HERE: with no data dir to keep
     * the scratch name in (`scratchDirForThisApp`), a retry's worker picks a new
     * one while the app still holds the server the failed worker saved.
     */
    async function ensureServer(tauriPage: PageLike): Promise<string> {
      const saved = await savedServerFor(tauriPage, fixture)
      if (saved && pointsHere(saved)) return saved.places[0].volumeId
      await forgetServer(tauriPage, fixture)
      expect(await addServerThroughSheet(tauriPage, fixture, dir, SERVER_PASSWORD)).toBe('closed')
      const added = await requireSavedServer(tauriPage, fixture)
      // Where the sheet left a pane is the landing test's business; every other
      // test starts from both panes on local disk.
      await ensureAppReady(tauriPage)
      return added.places[0].volumeId
    }

    /**
     * Presses `key` on the cursored entry of the focused pane, confirms the
     * transfer dialog (answering a conflict with `policy` when one shows), and
     * waits for the write to be over on both sides.
     */
    async function transferCursored(tauriPage: PageLike, key: 'F5' | 'F6', policy?: 'skip' | 'overwrite') {
      await press(tauriPage, key)
      await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
      if (policy) {
        await waitForConflictPolicy(tauriPage)
        await selectConflictPolicy(tauriPage, policy)
      }
      await clickTransferStart(tauriPage)
      await waitForTransferUiToSettle(tauriPage)
      await dismissAllToasts(tauriPage)
    }

    test('a wrong password is refused in the sheet, which stays open, and nothing is saved', async ({ tauriPage }) => {
      await forgetServer(tauriPage, fixture)

      const round = await addServerThroughSheet(tauriPage, fixture, dir, 'not-the-password')

      // ❗ Under the password, and the sheet still up: closing would take what was typed with it.
      expect(round).toBe('secret_refused')
      expect(await tauriPage.isVisible(SHEET)).toBe(true)
      expect(await savedServerFor(tauriPage, fixture)).toBeNull()
      expect(await registeredVolumeFor(tauriPage, fixture)).toBeNull()
    })

    test('adding it through the sheet signs in and lands the focused pane on it', async ({ tauriPage }) => {
      await forgetServer(tauriPage, fixture)
      await focusSide(tauriPage, 'left')

      expect(await addServerThroughSheet(tauriPage, fixture, dir, SERVER_PASSWORD)).toBe('closed')

      const volumeId = (await requireSavedServer(tauriPage, fixture)).places[0].volumeId
      // The pane the person was in lands on what they just connected to.
      await expect
        .poll(async () => (await paneLocation('left')).volumeId, { timeout: waitBudget(15000) })
        .toBe(volumeId)
      await expect.poll(async () => paneLists(tauriPage, 'left', LANDMARK), { timeout: waitBudget(15000) }).toBeTruthy()
    })

    test('copies a file to the server and back, byte for byte', async ({ tauriPage }) => {
      const volumeId = await ensureServer(tauriPage)
      const fixtureRoot = getFixtureRoot()
      const up = payload(`${protocol} up`)
      const down = payload(`${protocol} down`)
      fs.writeFileSync(path.join(fixtureRoot, 'left', 'upload-me.txt'), up)
      fixture.writeFile(`${dir}/download-me.txt`, down)
      await ensureAppReady(tauriPage, { leftPane: ['upload-me.txt'] })
      await openServerInPane(tauriPage, 'right', volumeId, 'download-me.txt')

      // Local → server.
      await focusSide(tauriPage, 'left')
      expect(await moveCursorToFile(tauriPage, 'upload-me.txt')).toBe(true)
      await transferCursored(tauriPage, 'F5')
      expect(fixture.readFile(`${dir}/upload-me.txt`)).toEqual(up)
      expect(readLocal(path.join(fixtureRoot, 'left', 'upload-me.txt')), 'a copy leaves the source').toEqual(up)
      await expect
        .poll(async () => paneLists(tauriPage, 'right', 'upload-me.txt'), { timeout: waitBudget(10000) })
        .toBeTruthy()

      // Server → local.
      await focusSide(tauriPage, 'right')
      expect(await moveCursorToFile(tauriPage, 'download-me.txt')).toBe(true)
      await transferCursored(tauriPage, 'F5')
      expect(readLocal(path.join(fixtureRoot, 'left', 'download-me.txt'))).toEqual(down)
      expect(fixture.readFile(`${dir}/download-me.txt`), 'a copy leaves the source').toEqual(down)
    })

    test('moves a file to the server and back, leaving no copy behind', async ({ tauriPage }) => {
      const volumeId = await ensureServer(tauriPage)
      const fixtureRoot = getFixtureRoot()
      const out = payload(`${protocol} move out`)
      const home = payload(`${protocol} move home`)
      fs.writeFileSync(path.join(fixtureRoot, 'left', 'move-out.txt'), out)
      fixture.writeFile(`${dir}/move-home.txt`, home)
      await ensureAppReady(tauriPage, { leftPane: ['move-out.txt'] })
      await openServerInPane(tauriPage, 'right', volumeId, 'move-home.txt')

      await focusSide(tauriPage, 'left')
      expect(await moveCursorToFile(tauriPage, 'move-out.txt')).toBe(true)
      await transferCursored(tauriPage, 'F6')
      expect(fixture.readFile(`${dir}/move-out.txt`)).toEqual(out)
      expect(readLocal(path.join(fixtureRoot, 'left', 'move-out.txt')), 'a move takes the source').toBeNull()

      await focusSide(tauriPage, 'right')
      expect(await moveCursorToFile(tauriPage, 'move-home.txt')).toBe(true)
      await transferCursored(tauriPage, 'F6')
      expect(readLocal(path.join(fixtureRoot, 'left', 'move-home.txt'))).toEqual(home)
      expect(fixture.readFile(`${dir}/move-home.txt`), 'a move takes the source').toBeNull()
      await expect
        .poll(async () => paneLists(tauriPage, 'right', 'move-home.txt'), { timeout: waitBudget(10000) })
        .toBeFalsy()
    })

    test('renames a file on the server in place', async ({ tauriPage }) => {
      const volumeId = await ensureServer(tauriPage)
      const bytes = payload(`${protocol} rename`)
      fixture.writeFile(`${dir}/before-rename.txt`, bytes)
      await openServerInPane(tauriPage, 'right', volumeId, 'before-rename.txt')

      await focusSide(tauriPage, 'right')
      expect(await moveCursorToFile(tauriPage, 'before-rename.txt')).toBe(true)
      await press(tauriPage, 'F2')
      await tauriPage.waitForSelector('.rename-input', 3000)
      await setRenameInput(tauriPage, 'after-rename.txt')
      await tauriPage.press('.rename-input', 'Enter')
      await expect.poll(async () => tauriPage.isVisible('.rename-input'), { timeout: waitBudget(5000) }).toBeFalsy()

      await expect
        .poll(async () => paneLists(tauriPage, 'right', 'after-rename.txt'), { timeout: waitBudget(10000) })
        .toBeTruthy()
      expect(fixture.readFile(`${dir}/after-rename.txt`)).toEqual(bytes)
      expect(fixture.readFile(`${dir}/before-rename.txt`)).toBeNull()
    })

    test('deletes a file on the server, and only that file', async ({ tauriPage }) => {
      const volumeId = await ensureServer(tauriPage)
      const keep = payload(`${protocol} keep`)
      fixture.writeFile(`${dir}/delete-me.txt`, payload(`${protocol} delete`))
      fixture.writeFile(`${dir}/keep-me.txt`, keep)
      await openServerInPane(tauriPage, 'right', volumeId, 'delete-me.txt')

      await focusSide(tauriPage, 'right')
      expect(await moveCursorToFile(tauriPage, 'delete-me.txt')).toBe(true)
      await press(tauriPage, 'F8')
      await tauriPage.waitForSelector(DELETE_DIALOG, 5000)
      // A server has no trash, so the dialog's way forward is the permanent delete.
      await expect
        .poll(async () => tauriPage.isEnabled(`${DELETE_DIALOG} .btn-danger`), { timeout: waitBudget(5000) })
        .toBeTruthy()
      await tauriPage.click(`${DELETE_DIALOG} .btn-danger`)
      await waitForTransferUiToSettle(tauriPage)
      await expect.poll(async () => tauriPage.isVisible('.modal-overlay'), { timeout: waitBudget(10000) }).toBeFalsy()
      await dismissAllToasts(tauriPage)

      await expect
        .poll(async () => paneLists(tauriPage, 'right', 'delete-me.txt'), { timeout: waitBudget(10000) })
        .toBeFalsy()
      expect(fixture.readFile(`${dir}/delete-me.txt`)).toBeNull()
      expect(fixture.readFile(`${dir}/keep-me.txt`)).toEqual(keep)
    })

    test('a copy onto a name the server already has keeps the server file when Skip is picked', async ({
      tauriPage,
    }) => {
      const volumeId = await ensureServer(tauriPage)
      const fixtureRoot = getFixtureRoot()
      const theirs = payload(`${protocol} server copy of clash`)
      fixture.writeFile(`${dir}/clash-skip.txt`, theirs)
      fs.writeFileSync(path.join(fixtureRoot, 'left', 'clash-skip.txt'), payload(`${protocol} local clash`))
      await ensureAppReady(tauriPage, { leftPane: ['clash-skip.txt'] })
      await openServerInPane(tauriPage, 'right', volumeId, 'clash-skip.txt')

      await focusSide(tauriPage, 'left')
      expect(await moveCursorToFile(tauriPage, 'clash-skip.txt')).toBe(true)
      await transferCursored(tauriPage, 'F5', 'skip')

      // ❗ Nothing silently clobbered: the server's own bytes, and nothing left beside them.
      expect(fixture.readFile(`${dir}/clash-skip.txt`)).toEqual(theirs)
      expect(fixture.list(dir).filter((name) => name.includes('clash-skip'))).toEqual(['clash-skip.txt'])
    })

    test('a copy onto a name the server already has replaces it only when Overwrite is picked', async ({
      tauriPage,
    }) => {
      const volumeId = await ensureServer(tauriPage)
      const fixtureRoot = getFixtureRoot()
      const ours = payload(`${protocol} local clash, the winner`)
      fixture.writeFile(`${dir}/clash-overwrite.txt`, payload(`${protocol} server clash`))
      fs.writeFileSync(path.join(fixtureRoot, 'left', 'clash-overwrite.txt'), ours)
      await ensureAppReady(tauriPage, { leftPane: ['clash-overwrite.txt'] })
      await openServerInPane(tauriPage, 'right', volumeId, 'clash-overwrite.txt')

      await focusSide(tauriPage, 'left')
      expect(await moveCursorToFile(tauriPage, 'clash-overwrite.txt')).toBe(true)
      await transferCursored(tauriPage, 'F5', 'overwrite')

      expect(fixture.readFile(`${dir}/clash-overwrite.txt`)).toEqual(ours)
      expect(fixture.list(dir).filter((name) => name.includes('clash-overwrite'))).toEqual(['clash-overwrite.txt'])
    })

    test('an uploaded name lands composed, and a server name comes down byte for byte', async ({ tauriPage }) => {
      const volumeId = await ensureServer(tauriPage)
      const fixtureRoot = getFixtureRoot()
      // Three different words, so a filesystem that folds normalization for
      // lookups (APFS does) can't take any two for one name. `nfd` is spelled
      // decomposed in this source file.
      const nfd = 'Café nfd.txt'
      const nfc = 'Crème nfc.txt'
      const nfdUp = 'Árvíz nfd-up.txt'.normalize('NFD')
      fixture.writeFile(`${dir}/${nfd}`, payload('nfd'))
      fs.writeFileSync(path.join(fixtureRoot, 'left', nfc), payload('nfc'))
      fs.writeFileSync(path.join(fixtureRoot, 'left', nfdUp), payload('nfd-up'))
      await ensureAppReady(tauriPage, { leftPane: [nfc, nfdUp] })
      await openServerInPane(tauriPage, 'right', volumeId, nfd)

      // A name Cmdr creates on the server goes out composed (NFC), whatever
      // spelling the local disk gave it: servers match bytes, and NFC is what
      // links, scripts, and other clients there expect.
      await focusSide(tauriPage, 'left')
      expect(await moveCursorToFile(tauriPage, nfc)).toBe(true)
      await transferCursored(tauriPage, 'F5')
      expect(await moveCursorToFile(tauriPage, nfdUp)).toBe(true)
      await transferCursored(tauriPage, 'F5')
      const onServer = fixture.list(dir)
      expect(onServer).toContain(nfc)
      expect(onServer, 'an NFC name stays NFC').not.toContain(nfc.normalize('NFD'))
      expect(fixture.readFile(`${dir}/${nfc}`)).toEqual(payload('nfc'))
      expect(onServer, 'a decomposed local name lands composed').toContain(nfdUp.normalize('NFC'))
      expect(onServer, 'and never in its decomposed spelling').not.toContain(nfdUp)
      expect(fixture.readFile(`${dir}/${nfdUp.normalize('NFC')}`)).toEqual(payload('nfd-up'))

      await focusSide(tauriPage, 'right')
      expect(await moveCursorToFile(tauriPage, nfd)).toBe(true)
      await transferCursored(tauriPage, 'F5')
      const local = listLocal(path.join(fixtureRoot, 'left'))
      expect(local).toContain(nfd)
      expect(local, 'the local disk got the NFD spelling, not a re-normalized one').not.toContain(nfd.normalize('NFC'))
      expect(readLocal(path.join(fixtureRoot, 'left', nfd))).toEqual(payload('nfd'))
    })

    test("the switcher row's Reconnect automatically checkbox flips the saved setting and keeps it", async ({
      tauriPage,
    }) => {
      const volumeId = await ensureServer(tauriPage)
      const toggle = autoReconnectRow(volumeId)
      const before = (await requireSavedServer(tauriPage, fixture)).autoReconnect

      await openRowSubmenuWithArrowKeys(tauriPage, 'right', volumeId)
      expect(await isChecked(tauriPage, toggle)).toBe(before)
      expect(await pointerClick(tauriPage, `document.querySelector('${toggle}')`)).toBe('clicked')
      await expect
        .poll(async () => (await savedServerFor(tauriPage, fixture))?.autoReconnect, { timeout: waitBudget(5000) })
        .toBe(!before)

      // A fresh open reads the switch back from Rust rather than from the click.
      await closeSwitcher(tauriPage)
      await openRowSubmenuWithArrowKeys(tauriPage, 'right', volumeId)
      expect(await isChecked(tauriPage, toggle)).toBe(!before)

      // And back, so the next test starts where this one did.
      expect(await pointerClick(tauriPage, `document.querySelector('${toggle}')`)).toBe('clicked')
      await expect
        .poll(async () => (await savedServerFor(tauriPage, fixture))?.autoReconnect, { timeout: waitBudget(5000) })
        .toBe(before)
      await closeSwitcher(tauriPage)
    })

    test('disconnecting from the switcher sends the pane home and keeps the server saved', async ({ tauriPage }) => {
      try {
        const volumeId = await ensureServer(tauriPage)
        await openServerInPane(tauriPage, 'right', volumeId, LANDMARK)

        await openSwitcher(tauriPage, 'right')
        const disconnect = `${switcherRow(volumeId)} .eject-button`
        await expect.poll(async () => tauriPage.isVisible(disconnect), { timeout: waitBudget(5000) }).toBeTruthy()
        expect(await pointerClick(tauriPage, `document.querySelector('${disconnect}')`)).toBe('clicked')

        await expect
          .poll(async () => (await paneLocation('right')).volumeId, { timeout: waitBudget(15000) })
          .not.toBe(volumeId)
        expect((await paneLocation('right')).path.startsWith(`${protocol}://`)).toBe(false)
        await expect
          .poll(async () => registeredVolumeFor(tauriPage, fixture), { timeout: waitBudget(10000) })
          .toBeNull()
        expect(await savedServerFor(tauriPage, fixture), 'Disconnect drops the session, not the server').not.toBeNull()
      } finally {
        // The app's half of this suite's teardown (see the `afterAll`).
        await closeSwitcher(tauriPage).catch(() => undefined)
        await forgetServer(tauriPage, fixture)
      }
    })
  })
}
