/**
 * The two "reach" clips: everywhere Cmdr browses besides the local disk, on the repo's own
 * fixtures dressed as a home setup (`marketing-video2-fixtures.ts`, staged by the
 * orchestrator before launch, `scripts/marketing-clips.ts`).
 *
 * - `reach-network`: a NAS added from ⌘K and browsed over SMB, then an S3 bucket.
 * - `reach-devices`: a Git repo's history as folders, a restic backup unlocked into its
 *   snapshots, and an Android phone over MTP.
 *
 * Both loop: each place opens in a new tab (⌘T) or is left with ⌘↑, and the tab is closed
 * again (⌘W), so the last frame is the first one.
 *
 * ❗ The fixtures listen on 127.0.0.1 ports. The NAS is reached as `home-nas.local`, a
 * Bonjour host the orchestrator proxies to 127.0.0.1 (`dns-sd -P`, no root), so no frame
 * shows a loopback address. The SMB port (11481) still shows in the typed address.
 */

import { waitBudget } from './wait-budget.js'
import { spawnSync } from 'node:child_process'
import { join } from 'node:path'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { mcpCall, mcpOpenMtpStorageRoot, mcpReadResource, mcpSelectVolume } from '../e2e-shared/mcp-client.js'
import { expect } from './fixtures.js'
import { clickButtonByText, dismissAllToasts, ensureExplorerFocused } from './helpers.js'
import { setSetting } from './marketing-stage.js'
import { LEAD_IN_MS, hold, inTake, invoke, recordWindow, stageMain, typeSlowly } from './marketing-clips-scenes.js'
import { DEMO, DEMO_ROOT } from './marketing-clips-demo.js'
import { captionKey } from './marketing-clips-take.js'
import type { ClipTheme } from './marketing-clips-take.js'
import { keyboardCaptioned, pressCaptioned, runCaptioned, typeCaptioned } from './marketing-clips-keys.js'
import {
  BUCKET_FAVORITE,
  NAS_HOST,
  NFS_FAVORITE,
  PHONE,
  RESTIC,
  RESTIC_CLIPS_PARENT,
  S3,
  SMB,
  seedPhone,
} from './marketing-video2-fixtures.js'
import { rowNamed, waitForRow, waitForRowCount } from './marketing-video-scenes.js'

const SHEET = '[data-dialog-id="server-sign-in"]'
const PASSWORD_DIALOG = '[data-dialog-id="archive-password"]'
const MENU = '[data-menu]:not([data-menu-submenu])'

/** Typing pace in the sheets: quicker than the search box, since there's more to type. */
const SHEET_CHAR_MS = 55

/** Types `text` into the sheet's `#id` field, captioned as one string. */
async function typeIntoField(page: TauriPage, id: string, text: string, caption = text): Promise<void> {
  const selector = `${SHEET} #${id}`
  await page.waitForSelector(selector, waitBudget(10000))
  captionKey(caption)
  await typeSlowly(page, selector, text, SHEET_CHAR_MS)
}

/** Enter on the focused entry named `name` (moved to off camera, as the cursor would be), captioned. */
async function enter(page: TauriPage, pane: 'left' | 'right', name: string): Promise<void> {
  await mcpCall('move_cursor', { pane, filename: name })
  await hold(350)
  await ensureExplorerFocused(page)
  await keyboardCaptioned(page, 'Enter')
}

/** ⌘↑, then waits for the focused pane to list `expect`. */
async function up(page: TauriPage, expectRow: string): Promise<void> {
  await ensureExplorerFocused(page)
  await pressCaptioned(page, 'Meta+ArrowUp')
  await waitRow(page, expectRow)
}

/**
 * Opens the favorites menu (⌃D) and arrows down to the favorite named `name`, then Enter:
 * the way a person jumps to a saved place from the keyboard. One press per round trip,
 * reading the highlight back before the next (as `server-ops-helpers.ts` does), and no
 * more presses than the menu has rows. ❗ Favorites, ❌ never the volume switcher: the
 * switcher lists every volume this Mac has, David's own NAS and cloud drives among them
 * (2026-11-03).
 */
async function chooseFavorite(page: TauriPage, name: string): Promise<void> {
  await runCaptioned(page, 'favorites.open')
  const n = JSON.stringify(name)
  const row = `[...document.querySelectorAll('${MENU} [data-menu-row]')].find((r) => (r.querySelector('.favorite-label')?.textContent ?? '').trim() === ${n})`
  await expect
    .poll(() => page.evaluate<boolean>(`!!(${row})`), {
      timeout: waitBudget(10000),
      message: `the favorites menu lists no '${name}'`,
    })
    .toBe(true)
    .catch(async (e: unknown) => {
      const seen = await page.evaluate<string>(
        `JSON.stringify([...document.querySelectorAll('${MENU} .favorite-label')].map((l) => l.textContent.trim()))`,
      )
      throw new Error(`${String(e)}; its rows: ${seen}`)
    })
  const rows = await page.evaluate<number>(`document.querySelectorAll('${MENU} [data-menu-row]').length`)
  const highlighted = `!!(${row})?.hasAttribute('data-highlighted')`
  for (let i = 0; i <= rows && !(await page.evaluate<boolean>(highlighted)); i++) {
    await pressCaptioned(page, 'ArrowDown')
    await hold(220)
  }
  await expect.poll(() => page.evaluate<boolean>(highlighted), { timeout: waitBudget(2000) }).toBe(true)
  await hold(300)
  await pressCaptioned(page, 'Enter')
}

/**
 * Off camera: makes the place the right pane shows now a favorite named `name`, at the
 * root `rootOf` reads off the pane state. ❗ Never prints the state: an S3 app path carries
 * the access key id.
 */
async function favoriteRightPane(page: TauriPage, name: string, rootOf: RegExp): Promise<void> {
  let root: string | null = null
  await expect
    .poll(
      async () => {
        root = rootOf.exec(await mcpReadResource('cmdr://state?include=panes'))?.[0] ?? null
        return root !== null
      },
      { timeout: waitBudget(15000), message: `the right pane never opened ${name}` },
    )
    .toBe(true)
  const added = await page.evaluate<string>(
    `window.__TAURI_INTERNALS__.invoke('add_favorite', ${JSON.stringify({ path: String(root), name })}).then(function () { return 'ok' }, function (e) { return JSON.stringify(e) })`,
  )
  if (added !== 'ok') throw new Error(`couldn't favorite ${name}: ${added}`)
}

/** Off camera: puts the right pane on the volume named `name`, once it's registered. */
async function selectVolumeWhenListed(name: string): Promise<void> {
  await expect
    .poll(
      () =>
        mcpSelectVolume('right', name).then(
          () => true,
          () => false,
        ),
      { timeout: waitBudget(15000), message: `no volume named ${name} registered` },
    )
    .toBe(true)
}

/** `waitForRow`, saying what the panes show when the row never comes. */
async function waitRow(page: TauriPage, name: string, options: { prefix?: boolean } = {}): Promise<void> {
  await waitForRow(page, name, options).catch(async (e: unknown) => {
    const seen = await page.evaluate<string>(
      `JSON.stringify([...document.querySelectorAll('.file-pane')].map((p) => ({ focused: p.classList.contains('is-focused'), text: (p.textContent || '').replace(/\\s+/g, ' ').slice(0, 240) })))`,
    )
    throw new Error(`${String(e)}; the panes: ${seen}`)
  })
}

/** Waits until the focused pane has `n` tabs. Each pane has one tab bar, in pane order. */
async function waitForTabs(page: TauriPage, n: number): Promise<void> {
  const count = `(function () {
    var panes = [...document.querySelectorAll('.file-pane')];
    var i = panes.findIndex(function (p) { return p.classList.contains('is-focused') });
    var bar = document.querySelectorAll('.tab-bar')[i];
    return bar ? bar.querySelectorAll('[role="tab"]').length : -1;
  })()`
  await expect.poll(() => page.evaluate<number>(count), { timeout: waitBudget(5000) }).toBe(n)
}

/** One saved server as `list_saved_servers` answers it, the fields the reset reads. */
interface SavedServer {
  id: string
  protocol: string
  displayName: string
  address: string
}

/**
 * Off camera: forgets the NAS (every saved row naming it: the host, its share, its saved
 * password) and unmounts its share, so each take adds it from scratch. ❗ Unmounts only a
 * share of the NAS fixture, matched by host and port, ❌ never another `smbfs` mount.
 */
async function forgetNas(page: TauriPage): Promise<void> {
  const saved = await page.evaluate<SavedServer[]>(`window.__TAURI_INTERNALS__.invoke('list_saved_servers')`)
  for (const server of saved) {
    const isNas =
      server.protocol === 'smb' &&
      (server.displayName === SMB.name || server.address.includes(NAS_HOST) || server.address.includes(SMB.host))
    if (!isNas) continue
    await invoke(page, 'forget_server_secret', { id: server.id }).catch(() => {})
    await invoke(page, 'forget_server', { id: server.id }).catch(() => {})
  }
  // The sign-in files the password under the dialed `host:port`, which the saved row's
  // forget doesn't reach (it went on answering from the cache, 2026-11-03).
  for (const server of [`${NAS_HOST}:${String(SMB.port)}`, `${NAS_HOST}.:${String(SMB.port)}`]) {
    for (const share of [null, SMB.share]) {
      await invoke(page, 'delete_smb_credentials', { server, share }).catch(() => {})
    }
  }
  const mounts = spawnSync('mount', [], { encoding: 'utf8' }).stdout.split('\n')
  for (const line of mounts) {
    // `//testuser@home-nas.local.:11481/private on /Volumes/private (smbfs, …)`
    const m = /^\/\/(?:[^@\s]+@)?([^:\s]+):(\d+)\/\S+ on (.+?) \(smbfs/.exec(line)
    if (m === null) continue
    const [, host, port, mountPoint] = m
    const ours = Number(port) === SMB.port && [NAS_HOST, `${NAS_HOST}.`, SMB.host].includes(host)
    if (ours) spawnSync('diskutil', ['unmount', mountPoint], { encoding: 'utf8' })
  }
}

/**
 * The NAS's NFS export the clip browses: the Docker fixture's `/insecure` (the one an
 * unprivileged client mounts), opened in its `photos` folder. The app reaches the
 * fixture's portmapper through `CMDR_NFS_PORTMAP_PORT`, which the orchestrator sets
 * (`crates/cmdr-nfs/DETAILS.md` § "The E2E portmapper override").
 */
const NFS = { export: '/insecure', folder: 'photos', uid: 1000, gid: 1000 } as const

/**
 * Off camera: the NAS's NFS export saved and connected, named "Home NAS" and dialed on
 * 127.0.0.1 like the bucket (its label shows only the export), and its
 * `photos` folder made a favorite. Idempotent: an existing place is left alone, and a
 * re-added favorite moves rather than doubling.
 */
async function connectNfsAndFavorite(page: TauriPage): Promise<void> {
  const saved = await page.evaluate<SavedServer[]>(`window.__TAURI_INTERNALS__.invoke('list_saved_servers')`)
  if (!saved.some((s) => s.protocol === 'nfs' && s.address.includes(SMB.host))) {
    const outcome = await page.evaluate<string>(
      `window.__TAURI_INTERNALS__.invoke('connect_server', ${JSON.stringify({
        target: {
          protocol: 'nfs',
          displayName: SMB.name,
          host: SMB.host,
          port: null,
          export: NFS.export,
          uid: NFS.uid,
          gid: NFS.gid,
          otherGids: [],
          startFolder: null,
          autoReconnect: false,
        },
        attemptId: `server-connect-clips-${String(Date.now())}`,
        secret: null,
      })}).then(function (r) { return JSON.stringify(r) }, function (e) { return 'refused: ' + JSON.stringify(e) })`,
    )
    if (outcome.startsWith('refused')) throw new Error(`the NFS export didn't connect: ${outcome}`)
  }
  // `nfsAppRoot`'s spelling: the host, 2049 for an address that named no port, the export as one segment.
  const path = `nfs://${SMB.host}:2049/${NFS.export.slice(1)}/${NFS.folder}`
  await expect
    .poll(
      () =>
        page.evaluate<string>(
          `window.__TAURI_INTERNALS__.invoke('add_favorite', ${JSON.stringify({ path, name: NFS_FAVORITE })}).then(function () { return 'ok' }, function (e) { return JSON.stringify(e) })`,
        ),
      { timeout: waitBudget(15000), message: `couldn't favorite ${path}` },
    )
    .toBe('ok')
}

/**
 * Off camera: the `family-photos` bucket saved and connected. On 127.0.0.1, ❌ not
 * `NAS_HOST`: nothing on screen names a bucket's host, and the Bonjour name failed to
 * resolve mid-take under load (2026-11-03). Only the SMB share's label shows its host.
 */
async function connectBucket(page: TauriPage): Promise<void> {
  await page.evaluate(
    `window.__TAURI_INTERNALS__.invoke('connect_server', ${JSON.stringify({
      target: {
        protocol: 's3',
        displayName: S3.bucket,
        provider: { kind: 'other', endpoint: S3.endpoint, region: S3.region, pathStyle: true },
        accessKeyId: S3.accessKey,
        bucket: S3.bucket,
        autoReconnect: false,
      },
      attemptId: `server-connect-clips-${String(Date.now())}`,
      // The fixture's public test key, remembered in the clips instance's file store.
      secret: { secret: S3.secretKey, remember: true },
    })}).catch(function (e) { return String(e) })`,
  )
}

/**
 * A NAS added from ⌘K by its network name, signed in to and browsed over SMB, then an S3
 * bucket opened from the favorites. Each in a new tab, closed again at the end.
 */
export async function reachNetwork(page: TauriPage, theme: ClipTheme): Promise<void> {
  await forgetNas(page)
  // Connected once per run (the orchestrator forgets it before launch): a second connect
  // re-registered the live bucket, and it took longer than the select's patience to come
  // back. Asked of the saved list, ❌ not of `select_volume`, which also answers to the
  // bucket's favorite and opened a bucket nothing had connected (2026-11-03).
  const saved = await page.evaluate<SavedServer[]>(`window.__TAURI_INTERNALS__.invoke('list_saved_servers')`)
  if (!saved.some((s) => s.protocol === 's3' && s.displayName === S3.bucket)) await connectBucket(page)
  await selectVolumeWhenListed(S3.bucket)
  await favoriteRightPane(page, BUCKET_FAVORITE, new RegExp(`s3://[^\\s"]*?/${S3.bucket}(?=/|\\s|"|$)`))
  await connectNfsAndFavorite(page)
  await stageMain(page, theme, DEMO_ROOT, join(DEMO_ROOT, DEMO.photos))
  await dismissAllToasts(page)
  const take = await recordWindow(page, { slug: 'reach-network', theme, label: 'main' })
  try {
    await inTake(take, async () => {
      await hold(700)
      take.markStart()
      await hold(LEAD_IN_MS)

      // The NAS: a new tab, ⌘K, its address, a name, the account.
      await runCaptioned(page, 'tab.new')
      await waitForTabs(page, 2)
      await hold(500)
      await runCaptioned(page, 'servers.connect')
      // The trailing dot keeps the name whole: the app strips a final `.local` before it
      // dials SMB, and `home-nas` alone resolves to nothing (`build_smb_addr` in
      // `crates/cmdr-smb/src/connection.rs`; reported 2026-11-03).
      await typeIntoField(page, 'server-address', `smb://${NAS_HOST}.:${String(SMB.port)}/${SMB.share}`)
      await hold(300)
      await typeIntoField(page, 'server-name', SMB.name)
      await hold(300)
      await typeIntoField(page, 'server-username', SMB.username)
      await hold(500)
      await pressCaptioned(page, 'Enter')
      // The NAS asks for the password on a sign-in sheet.
      await expect
        .poll(() => page.evaluate<boolean>(`!!document.querySelector('${SHEET} #sign-in-secret')`), {
          timeout: waitBudget(30000),
          message: 'the NAS never asked for its password',
        })
        .toBe(true)
      await hold(600)
      await typeIntoField(page, 'sign-in-secret', SMB.password, '••••••••')
      await hold(400)
      await pressCaptioned(page, 'Enter')
      // Signed in, the hand-off mounts the share the address named and opens it.
      await waitRow(page, 'Photos')
      await hold(1100)
      await enter(page, 'left', 'Photos')
      await waitRow(page, '2024')
      await hold(600)
      await enter(page, 'left', '2024')
      await waitRow(page, 'IMG_00001')
      take.markPoster()
      await hold(1600)
      // Back out to the share's root: a place reopens where it was left, and the next
      // take has to find the share at its top too.
      await up(page, '2024')
      await up(page, 'Photos')
      await hold(400)

      // The same NAS over NFS, from its favorite, in the same tab.
      await chooseFavorite(page, NFS_FAVORITE)
      await waitRow(page, '2024 summer')
      await hold(700)
      await enter(page, 'left', '2024 summer')
      await waitRow(page, 'beach')
      await hold(1300)

      // The bucket, in a new tab on the right.
      await keyboardCaptioned(page, 'Tab')
      await hold(400)
      await runCaptioned(page, 'tab.new')
      await waitForTabs(page, 2)
      await hold(400)
      await chooseFavorite(page, BUCKET_FAVORITE)
      await waitRow(page, '2025')
      await hold(900)
      await enter(page, 'right', '2025')
      await waitRow(page, 'Easter')
      await hold(500)
      await enter(page, 'right', 'Easter')
      await waitRow(page, 'IMG_', { prefix: true })
      await hold(1600)
      await up(page, 'Easter')
      await up(page, '2025')
      await hold(400)

      // Both tabs closed again: back where the clip started.
      await runCaptioned(page, 'tab.close')
      await waitForTabs(page, 1)
      await hold(400)
      await keyboardCaptioned(page, 'Tab')
      await hold(300)
      await runCaptioned(page, 'tab.close')
      await waitForTabs(page, 1)
      await hold(1000)
    })
  } finally {
    await dismissAllToasts(page)
    await forgetNas(page)
  }
}

/**
 * Off camera: locks the backup again, so the next take asks for its password too. Its
 * container is the repo folder; `forget_saved_container_password` forgets the password
 * in memory and in the instance's store.
 */
async function lockBackup(page: TauriPage): Promise<void> {
  const repo = join(DEMO_ROOT, RESTIC_CLIPS_PARENT, RESTIC.folder)
  for (const path of [join(repo, 'Browse snapshots'), repo]) {
    await invoke(page, 'forget_saved_container_password', { parentVolumeId: 'root', path }).catch(() => {})
  }
}

/**
 * A Git repo's history as folders, a restic backup unlocked into its snapshots, and an
 * Android phone over MTP, each left again so the clip ends where it began.
 */
export async function reachDevices(page: TauriPage, theme: ClipTheme): Promise<void> {
  seedPhone(join(DEMO_ROOT, DEMO.trip))
  await invoke(page, 'rescan_virtual_mtp', {}).catch(() => {})
  await mcpOpenMtpStorageRoot('right', `${PHONE.model} - Internal Storage`)
  await favoriteRightPane(page, PHONE.model, /mtp:\/\/[^\s/]+\/[^\s/")]+/)
  await stageMain(page, theme, join(DEMO_ROOT, DEMO.code), join(DEMO_ROOT, RESTIC_CLIPS_PARENT))
  // The `.git` folder is hidden; the portal shows its history once it's listed. Neither
  // pane's folder holds another hidden file (the demo tree's marker is in its root).
  await setSetting(page, 'listing.showHiddenFiles', true)
  await invoke(page, 'set_show_virtual_git_portal', { enabled: true })
  await lockBackup(page)
  await mcpCall('move_cursor', { pane: 'right', filename: RESTIC.folder })
  // On `..`, where closing the phone's tab leaves the cursor too, so the loop's seam is clean.
  await mcpCall('move_cursor', { pane: 'left', index: 0 })
  await waitRow(page, 'website')
  try {
    const take = await recordWindow(page, { slug: 'reach-devices', theme, label: 'main' })
    await inTake(take, async () => {
      await hold(700)
      take.markStart()
      await hold(LEAD_IN_MS)

      // Git: the repo, its .git, the commits as folders.
      await enter(page, 'left', 'website')
      await waitRow(page, '.git')
      await hold(700)
      await enter(page, 'left', '.git')
      await waitRow(page, 'commits')
      await hold(800)
      await enter(page, 'left', 'commits')
      await waitForRowCount(page, 5)
      take.markPoster()
      await hold(1800)
      await up(page, 'branches')
      await up(page, '.git')
      await up(page, 'website')
      await hold(500)

      // restic: the backup, its password, the snapshots.
      await keyboardCaptioned(page, 'Tab')
      await hold(400)
      await enter(page, 'right', RESTIC.folder)
      await waitRow(page, 'Browse snapshots')
      await hold(600)
      await enter(page, 'right', 'Browse snapshots')
      await page.waitForSelector(PASSWORD_DIALOG, waitBudget(10000))
      await hold(500)
      await typeCaptioned('••••••••', () =>
        typeSlowly(page, `${PASSWORD_DIALOG} input`, RESTIC.password, SHEET_CHAR_MS),
      )
      await hold(400)
      // A click: the dialog takes no synthetic Enter in its field.
      await clickButtonByText(page, `${PASSWORD_DIALOG} button`, 'Unlock')
      await waitRow(page, 'latest')
      await hold(1800)
      await up(page, 'Browse snapshots')
      await up(page, RESTIC.folder)
      await hold(400)

      // The phone, in a new tab on the left.
      await keyboardCaptioned(page, 'Tab')
      await hold(400)
      await runCaptioned(page, 'tab.new')
      await waitForTabs(page, 2)
      await hold(400)
      await chooseFavorite(page, PHONE.model)
      await waitRow(page, 'DCIM')
      await hold(800)
      await enter(page, 'left', 'DCIM')
      await waitRow(page, 'Camera')
      await hold(500)
      await enter(page, 'left', 'Camera')
      await waitRow(page, 'PXL_', { prefix: true })
      await hold(1600)
      await runCaptioned(page, 'tab.close')
      await waitForTabs(page, 1)
      await expect
        .poll(
          () =>
            page.evaluate<boolean>(
              `[...document.querySelectorAll('.file-pane.is-focused .file-entry')].some((e) => ${rowNamed('website')})`,
            ),
          {
            timeout: waitBudget(5000),
          },
        )
        .toBe(true)
      await hold(1000)
    })
  } finally {
    await setSetting(page, 'listing.showHiddenFiles', false)
    await lockBackup(page)
    await dismissAllToasts(page)
  }
}
