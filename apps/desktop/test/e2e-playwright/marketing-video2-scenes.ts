/**
 * The second demo video's footage: "safe file operations, to and from anywhere your files
 * live". Seven beats, dark only, on the same machinery as `marketing-video-scenes.ts`, over
 * the repo's own fixtures dressed as a real person's setup (`marketing-video2-fixtures.ts`):
 * an SMB "Home NAS", an S3 `family-photos` bucket, a "Pixel 8" phone, a restic `Backups`
 * repo. `pnpm marketing:clips --video2` records them.
 */

import { waitBudget } from './wait-budget.js'
import { mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { mcpCall, mcpOpenMtpStorageRoot, mcpReadResource, mcpSelectVolume } from '../e2e-shared/mcp-client.js'
import { expect } from './fixtures.js'
import {
  TRANSFER_DIALOG,
  clickButtonByText,
  closeScopedWindow,
  dismissAllToasts,
  dispatchMenuCommand,
  ensureExplorerFocused,
  openQueueWindow,
} from './helpers.js'
import { clickTransferStart, confirmRollback } from './conflict-helpers.js'
import { setSetting } from './marketing-stage.js'
import {
  drainOperations,
  hold,
  invoke,
  placeQueueOverMain,
  recordWindow,
  setThrottles,
  stageMain,
  typeSlowly,
} from './marketing-clips-scenes.js'
import { DEMO, DEMO_ROOT, resetBackup, restoreProjects } from './marketing-clips-demo.js'
import { PHONE, PRICED_S3, RESTIC, S3, SMB, seedPhone } from './marketing-video2-fixtures.js'
import {
  VIDEO_LOOK,
  beat,
  chooseBrowseLikeAFolder,
  goParent,
  openEntry,
  rowNamed,
  waitForRow,
  waitForRowCount,
} from './marketing-video-scenes.js'

const PROGRESS_DIALOG = '[data-dialog-id="transfer-progress"]'
const LOG_DIALOG = '[data-dialog-id="operation-log"]'

/** Where the NAS share mounts; `mount_network_share` reports it, this is its usual answer. */
let nasRoot = '/Volumes/private'

/**
 * Adds the NAS as "Home NAS" and mounts its share, the production way (`connect_to_server`,
 * then `mount_network_share`, which upgrades to a direct smb2 connection). Idempotent: an
 * already-mounted share answers with its path.
 */
async function connectNas(page: TauriPage): Promise<void> {
  const address = `smb://${SMB.host}:${String(SMB.port)}`
  await page.evaluate(
    `window.__TAURI_INTERNALS__.invoke('connect_to_server', ${JSON.stringify({
      address,
      name: SMB.name,
      username: SMB.username,
      checkReachability: true,
    })}).catch(function () {})`,
  )
  const mounted = await page.evaluate<{ mountPath: string }>(
    `window.__TAURI_INTERNALS__.invoke('mount_network_share', ${JSON.stringify({
      server: SMB.host,
      share: SMB.share,
      username: SMB.username,
      password: SMB.password,
      port: SMB.port,
      timeoutMs: 30000,
      // The saved share's host: the name the volume picker calls the share by.
      hostName: SMB.name,
      attemptId: null,
    })})`,
  )
  nasRoot = mounted.mountPath
}

/** Adds the `family-photos` bucket as an S3 server, the way the Servers sheet does. */
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
      // Remembered (in the clips instance's file-backed secret store; the fixture's public
      // test key): unremembered, the volume asked to sign in again mid-take, 2026-10-23.
      secret: { secret: S3.secretKey, remember: true },
    })}).catch(function (e) { return String(e) })`,
  )
}

/** Waits for a copy's progress dialog to open and close again. */
async function waitForTransferDone(page: TauriPage, timeoutMs = waitBudget(60000)): Promise<void> {
  await expect.poll(async () => !(await page.isVisible(PROGRESS_DIALOG)), { timeout: timeoutMs }).toBeTruthy()
}

/** F5 with whatever is selected (or under the cursor), then Start, the way a person copies. */
async function copyWithF5(page: TauriPage): Promise<void> {
  await dispatchMenuCommand(page, 'file.copy')
  await page.waitForSelector(TRANSFER_DIALOG, waitBudget(10000))
  await hold(1400)
  await clickTransferStart(page)
}

/** 1. The NAS: a 5,000-photo folder lists instantly, then a copy within the share finishes server-side. */
export async function smb(page: TauriPage): Promise<void> {
  await connectNas(page)
  await stageMain(page, 'dark', nasRoot, join(nasRoot, 'Backup'), VIDEO_LOOK)
  const take = await recordWindow(page, { slug: 'smb', theme: 'dark', label: 'main' })
  await beat(take, async () => {
    await hold(800)
    take.note('the "Home NAS" share: Photos, Documents, Family videos')
    await openEntry(page, 'left', 'Photos')
    await waitForRow(page, '2024')
    await hold(700)
    take.note('Enter on 2024: 5,000 photos on the NAS')
    await openEntry(page, 'left', '2024')
    await waitForRow(page, 'IMG_00001')
    take.note('listed: 5,000 files')
    take.markPoster()
    await hold(1300)
    await ensureExplorerFocused(page)
    await page.keyboard.press('End')
    take.note('End: the last of them, already there')
    await hold(1500)
    await goParent(page)
    await waitForRow(page, '2025')
    await openEntry(page, 'left', '2025')
    await waitForRow(page, 'Lisbon trip')
    await hold(600)
    await mcpCall('move_cursor', { pane: 'left', filename: 'Lisbon trip' })
    take.note('F5: copy "Lisbon trip" to Backup, on the same NAS')
    await copyWithF5(page)
    take.note('a server-side copy: the NAS copies it without the data crossing the network')
    await waitForTransferDone(page)
    take.note('done')
    await hold(1800)
  })
  await dismissAllToasts(page)
}

/**
 * 2. S3: the copy dialog's cost estimate, then the upload. On a real priced bucket (R2)
 * when the orchestrator seeded one (`seedPricedS3`); without it, a download from the local
 * fixture, which is an unpriced "Other" endpoint and so shows no estimate.
 */
export async function s3(page: TauriPage): Promise<void> {
  const accountId = process.env.CMDR_CLIPS_R2_ACCOUNT
  const accessKeyId = process.env.CMDR_CLIPS_R2_KEY_ID
  if (accountId !== undefined && accessKeyId !== undefined) await s3Priced(page, { accountId, accessKeyId })
  else await s3FromFixture(page)
}

/**
 * Connects the R2 bucket with NO secret in the call: the orchestrator put it in the
 * instance's store before launch, so it never crosses Playwright, MCP, or IPC.
 */
async function connectPricedBucket(
  page: TauriPage,
  account: { accountId: string; accessKeyId: string },
): Promise<void> {
  const outcome = await page.evaluate(
    `window.__TAURI_INTERNALS__.invoke('connect_server', ${JSON.stringify({
      target: {
        protocol: 's3',
        displayName: PRICED_S3.displayName,
        provider: { kind: 'r2', accountId: account.accountId },
        accessKeyId: account.accessKeyId,
        bucket: PRICED_S3.bucket,
        autoReconnect: false,
      },
      attemptId: `server-connect-clips-${String(Date.now())}`,
      secret: null,
    })}).then(function (r) { return r && r.kind ? r.kind : r }, function (e) { return 'refused' })`,
  )
  console.log(`[marketing-video2] the R2 bucket's connect answered ${JSON.stringify(outcome)}`)
}

/**
 * The priced take: 1,300 tiny thumbnails selected, F5, the dialog's "About $0.01 at
 * Cloudflare R2 list prices", Start, and the upload running. Uploads, never downloads:
 * the dialog's From field spells a bucket's app path, key id included, and an upload's
 * From is the local folder.
 */
async function s3Priced(page: TauriPage, account: { accountId: string; accessKeyId: string }): Promise<void> {
  const thumbnails = join(DEMO_ROOT, DEMO.photos, PRICED_S3.thumbnails)
  stageThumbnails(thumbnails)
  try {
    await connectPricedBucket(page, account)
    await stageMain(page, 'dark', thumbnails, DEMO_ROOT, VIDEO_LOOK)
    // The picker names a bucket place by its bucket, whatever the account is called.
    await mcpSelectVolume('right', PRICED_S3.bucket)
    // The bucket's app path, read off the pane state. ❗ Never print the state: an S3 app
    // path carries the access key id.
    const bucketPath = new RegExp(`s3://\\S*?/${PRICED_S3.bucket}(?=/|\\s|"|$)`)
    let bucketRoot: string | null = null
    await expect
      .poll(
        async () => {
          bucketRoot = bucketPath.exec(await mcpReadResource('cmdr://state?include=panes'))?.[0] ?? null
          return bucketRoot !== null
        },
        { timeout: waitBudget(15000), message: `the right pane never opened the ${PRICED_S3.bucket} bucket` },
      )
      .toBe(true)
    const albumPath = `${String(bucketRoot)}/${PRICED_S3.prefix}/${PRICED_S3.album}/`
    await mcpCall('nav_to_path', { pane: 'right', path: albumPath })
    await waitForRowIn(page, 1, 'README')
    await waitForRowIn(page, 0, 'thumb-0001')
    const take = await recordWindow(page, { slug: 's3', theme: 'dark', label: 'main' })
    await beat(take, async () => {
      await hold(800)
      take.note('a family-photos folder in a Cloudflare R2 bucket, beside 1,300 thumbnails on the Mac')
      await mcpCall('select', { pane: 'left', all: true })
      await ensureExplorerFocused(page)
      await hold(1000)
      take.note('F5: copy all 1,300 into the bucket')
      await dispatchMenuCommand(page, 'file.copy')
      await page.waitForSelector(TRANSFER_DIALOG, waitBudget(10000))
      await page.waitForSelector(`${TRANSFER_DIALOG} .s3-cost-amount`, waitBudget(30000))
      take.note('the cost estimate: "About $0.01 at Cloudflare R2 list prices"')
      take.markPoster()
      await hold(3200)
      await clickTransferStart(page)
      await page.waitForSelector(PROGRESS_DIALOG, waitBudget(10000)).catch(() => {})
      take.note('uploading')
      await waitForTransferDone(page, waitBudget(300000))
      take.note('all 1,300 in the bucket')
      await hold(1500)
    })
  } finally {
    await drainOperations(page)
    await dispatchMenuCommand(page, 'selection.deselectAll')
    await dismissAllToasts(page)
    rmSync(thumbnails, { recursive: true, force: true })
  }
}

/**
 * `PRICED_S3.count` tiny JPEG-named files, about 4 KB each: enough requests for the
 * estimate to round to a cent, while the upload stays a few MB.
 */
function stageThumbnails(dir: string): void {
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  for (let i = 1; i <= PRICED_S3.count; i++) {
    writeFileSync(join(dir, `thumb-${String(i).padStart(4, '0')}.jpg`), Buffer.alloc(3_200 + ((i * 977) % 1_800), 0x5a))
  }
}

/** The fixture take: browse the local bucket, copy photos down. No estimate (an unpriced endpoint). */
async function s3FromFixture(page: TauriPage): Promise<void> {
  await connectBucket(page)
  const fromCloud = join(DEMO_ROOT, DEMO.photos, 'From the cloud')
  resetFolder(fromCloud)
  await stageMain(page, 'dark', fromCloud, DEMO_ROOT, VIDEO_LOOK)
  await mcpSelectVolume('right', S3.bucket)
  // The volume reopens where it was last left (an earlier take's album), so go to the
  // bucket's root: its path is the pane's up to and including the bucket name.
  const state = await mcpReadResource('cmdr://state?include=panes')
  const here = new RegExp(`(s3://\\S*?/${S3.bucket})/`).exec(state)
  if (here === null) throw new Error(`the right pane isn't in the ${S3.bucket} bucket`)
  const bucketRoot = here[1]
  await mcpCall('nav_to_path', { pane: 'right', path: `${bucketRoot}/` })
  await waitForRowIn(page, 1, '2025')
  const take = await recordWindow(page, { slug: 's3', theme: 'dark', label: 'main' })
  await beat(take, async () => {
    await hold(800)
    take.note('the family-photos bucket, browsed like a folder')
    // Into the albums the way the cursor goes, then the path: Enter from the keyboard
    // didn't reach the right pane here (it stayed on the bucket root, 2026-10-23).
    await mcpCall('move_cursor', { pane: 'right', filename: '2025' })
    await hold(600)
    await mcpCall('nav_to_path', { pane: 'right', path: `${bucketRoot}/2025/` })
    await waitForRowIn(page, 1, 'Easter')
    await mcpCall('move_cursor', { pane: 'right', filename: 'Easter' })
    await hold(600)
    await mcpCall('nav_to_path', { pane: 'right', path: `${bucketRoot}/2025/Easter/` })
    await waitForRowIn(page, 1, 'IMG_', { prefix: true })
    await hold(1500)
    // Downloads, not uploads: an upload into the VersityGW fixture was admitted and then
    // never moved for two minutes (2026-10-23, reported as a possible app bug).
    take.note('select the Easter photos in the bucket')
    await mcpCall('select', { pane: 'right', all: true })
    // The MCP select leaves the DOM focus elsewhere, and F5 then went nowhere (2026-10-23).
    await ensureExplorerFocused(page)
    await hold(1000)
    take.note('copy them down to the Mac')
    // Straight to the progress dialog: the copy dialog's From field spells the bucket's
    // URL, access key included.
    void mcpCall('copy', { autoConfirm: true }).catch(() => {})
    // A fast download can come and go before this looks; the done wait below still holds.
    await page.waitForSelector(PROGRESS_DIALOG, waitBudget(10000)).catch(() => {})
    take.markPoster()
    take.note('downloading')
    await waitForTransferDone(page, waitBudget(120000))
    take.note('on the Mac')
    await hold(1500)
  })
  await dispatchMenuCommand(page, 'selection.deselectAll')
  await dismissAllToasts(page)
}

/** Waits for pane `index` to list `name` (see `rowNamed` for what matches). */
async function waitForRowIn(
  page: TauriPage,
  index: 0 | 1,
  name: string,
  options: { prefix?: boolean } = {},
): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate<boolean>(`[...(document.querySelectorAll('.file-pane')[${String(index)}]?.querySelectorAll('.file-entry') ?? [])].some((e) =>
          ${rowNamed(name, options)})`),
      { timeout: waitBudget(30000) },
    )
    .toBe(true)
    .catch(async () => {
      const seen =
        await page.evaluate<string>(`JSON.stringify([...document.querySelectorAll('.file-pane')].map((p) => ({
        focused: p.classList.contains('is-focused'),
        rows: [...p.querySelectorAll('.file-entry')].slice(0, 5).map((e) => (e.textContent || '').trim().slice(0, 30)),
      })))`)
      throw new Error(`pane ${String(index)} never listed '${name}'; the panes: ${seen}`)
    })
}

/** 3. The phone over MTP: browse DCIM, copy the camera roll to the Mac. */
export async function phone(page: TauriPage): Promise<void> {
  seedPhone(join(DEMO_ROOT, DEMO.trip))
  await invoke(page, 'rescan_virtual_mtp', {}).catch(() => {})
  const fromPhone = join(DEMO_ROOT, DEMO.photos, 'From phone')
  resetFolder(fromPhone)
  await stageMain(page, 'dark', DEMO_ROOT, fromPhone, VIDEO_LOOK)
  await mcpOpenMtpStorageRoot('left', `${PHONE.model} - Internal Storage`)
  await waitForRow(page, 'DCIM')
  await setThrottles(page, 60, null)
  try {
    const take = await recordWindow(page, { slug: 'phone', theme: 'dark', label: 'main' })
    await beat(take, async () => {
      await hold(800)
      take.note('the Pixel 8, plugged in over USB')
      await openEntry(page, 'left', 'DCIM')
      await waitForRow(page, 'Camera')
      await hold(800)
      await openEntry(page, 'left', 'Camera')
      await waitForRow(page, 'PXL_', { prefix: true })
      take.note("DCIM/Camera: the phone's photos")
      await hold(1500)
      await dispatchMenuCommand(page, 'selection.selectAll')
      take.note('all selected; copy to the Mac')
      await hold(900)
      // Straight to the progress dialog: the copy dialog's From field spells the device's
      // internal path, which carries the fixture's `cmdr-e2e-virtual` serial.
      await mcpCall('copy', { autoConfirm: true })
      await page.waitForSelector(PROGRESS_DIALOG, waitBudget(10000))
      take.markPoster()
      take.note('copying off the phone')
      await waitForTransferDone(page, waitBudget(120000))
      take.note('on the Mac')
      await hold(1500)
    })
  } finally {
    await setThrottles(page, null, null)
    await dispatchMenuCommand(page, 'selection.deselectAll')
    await dismissAllToasts(page)
  }
}

/** Empties a demo folder (creating it), keeping the folder itself (`ensureDemoTree` says why). */
function resetFolder(dir: string): void {
  mkdirSync(dir, { recursive: true })
  for (const entry of readdirSync(dir)) rmSync(join(dir, entry), { recursive: true, force: true })
}

/** 4. A zip opened like a folder and a file copied out; then a restic snapshot unlocked and a file restored. */
export async function archives(page: TauriPage): Promise<void> {
  const restored = join(DEMO_ROOT, DEMO.backup)
  resetBackup()
  await stageMain(page, 'dark', join(DEMO_ROOT, DEMO.code), restored, VIDEO_LOOK)
  const take = await recordWindow(page, { slug: 'archives', theme: 'dark', label: 'main' })
  await beat(take, async () => {
    await hold(700)
    take.note('Enter on a zip: browse it like a folder')
    await openEntry(page, 'left', 'Brand assets.zip')
    await chooseBrowseLikeAFolder(page)
    await waitForRow(page, 'Brand refresh')
    await openEntry(page, 'left', 'Brand refresh')
    await waitForRow(page, 'Brand refresh 01')
    await hold(900)
    await mcpCall('move_cursor', { pane: 'left', filename: 'Brand refresh 04.docx' })
    take.note('F5: copy one file out of the zip')
    await copyWithF5(page)
    await waitForTransferDone(page)
    await hold(1200)
    take.note('a restic backup repo: "Backups"')
    await mcpCall('nav_to_path', { pane: 'left', path: DEMO_ROOT })
    await waitForRow(page, RESTIC.folder)
    await openEntry(page, 'left', RESTIC.folder)
    await waitForRow(page, 'Browse snapshots')
    await hold(900)
    await openEntry(page, 'left', 'Browse snapshots')
    await page.waitForSelector('[data-dialog-id="archive-password"]', waitBudget(10000))
    take.note('its password')
    await hold(600)
    await typeSlowly(page, '[data-dialog-id="archive-password"] input', RESTIC.password, 70)
    await hold(500)
    await clickButtonByText(page, '[data-dialog-id="archive-password"] button', 'Unlock')
    await waitForRow(page, 'latest')
    take.note('unlocked: one folder per snapshot')
    take.markPoster()
    await hold(1500)
    await openEntry(page, 'left', 'latest')
    // A snapshot holds absolute paths: Users/Shared/Studio/Documents.
    for (const part of ['Users', 'Shared', 'Studio', 'Documents']) {
      await waitForRow(page, part)
      await openEntry(page, 'left', part)
    }
    await waitForRow(page, 'Budget 2026')
    take.note('inside the snapshot; F5 restores a file')
    await mcpCall('move_cursor', { pane: 'left', filename: 'Budget 2026.csv' })
    await hold(700)
    await copyWithF5(page)
    await waitForTransferDone(page)
    take.note('restored')
    await hold(1200)
  })
  await dismissAllToasts(page)
  resetBackup()
}

/** 5. Copy, move, and compress jobs: queued, one in the background, paused and resumed, one cancelled. */
export async function transfers(page: TauriPage): Promise<void> {
  resetBackup()
  await stageMain(page, 'dark', join(DEMO_ROOT, DEMO.projects), join(DEMO_ROOT, DEMO.backup), VIDEO_LOOK)
  // Slow enough that the copy is still running while the move and the compress queue up
  // behind it, and the Operations window has something to pause.
  await setThrottles(page, 450, null)
  const queue = await openQueueWindow(page)
  try {
    await placeQueueOverMain(page)
    const take = await recordWindow(page, { slug: 'transfers', theme: 'dark', label: 'main', include: ['queue'] })
    // Above the main window: focusing main hid the Operations window behind it (2026-10-23).
    await invoke(page, 'plugin:window|set_focus', { label: 'queue' })
    await beat(take, async () => {
      await hold(700)
      take.note('F5: copy "Annual report"')
      await mcpCall('move_cursor', { pane: 'left', filename: 'Annual report' })
      await copyWithF5(page)
      await page.waitForSelector(PROGRESS_DIALOG, waitBudget(10000))
      await hold(1200)
      take.note('Background: it keeps going in the Operations window')
      await clickButtonByText(page, `${PROGRESS_DIALOG} button`, 'Background')
      await hold(1000)
      take.note('F6: move "Podcast season 3" (queued behind the copy)')
      await mcpCall('move_cursor', { pane: 'left', filename: 'Podcast season 3' })
      await dispatchMenuCommand(page, 'file.move')
      await page.waitForSelector(TRANSFER_DIALOG, waitBudget(10000))
      await hold(1100)
      await clickTransferStart(page)
      await queueFromProgress(page)
      take.note('compress "Workshop" into a zip (queued too)')
      await mcpCall('move_cursor', { pane: 'left', filename: 'Workshop' })
      await dispatchMenuCommand(page, 'file.compress')
      await page.waitForSelector(TRANSFER_DIALOG, waitBudget(10000))
      await hold(1100)
      await clickTransferStart(page)
      await queueFromProgress(page)
      take.markPoster()
      await hold(1200)
      const rows = await queueRows(queue)
      const running = rows.find((r) => r.status === 'running')
      const waiting = rows.filter((r) => r.status === 'queued')
      if (running) {
        take.note('Pause the running copy')
        await clickQueueRowButton(queue, running.id, 'Pause')
        await hold(1600)
        take.note('Resume')
        await clickQueueRowButton(queue, running.id, 'Resume')
        await hold(1200)
      }
      const last = waiting.at(-1)
      if (last) {
        take.note('Cancel the compress before it starts')
        await clickQueueRowButton(queue, last.id, 'Cancel')
      }
      take.note('the queue works through the rest')
      await expect
        .poll(
          () =>
            page.evaluate<number>(
              `window.__TAURI_INTERNALS__.invoke('list_operations').then(function(o){ return o.length })`,
            ),
          {
            timeout: waitBudget(90000),
          },
        )
        .toBe(0)
      await hold(1200)
    })
  } finally {
    await setThrottles(page, null, null)
    await drainOperations(page)
    await closeScopedWindow(page, queue, 'queue')
    await dismissAllToasts(page)
    // The move took "Podcast season 3" out of Projects; put it back.
    resetBackup()
    restoreProjects()
  }
}

/** Sends a just-started operation's progress dialog to the queue ("Queue" or "Background"). */
async function queueFromProgress(page: TauriPage): Promise<void> {
  await page.waitForSelector(PROGRESS_DIALOG, waitBudget(10000))
  await hold(700)
  await page.evaluate(`(function(){
    var b = [...document.querySelectorAll(${JSON.stringify(`${PROGRESS_DIALOG} button`)})].find(function (x) {
      var t = (x.textContent || '').trim(); return t.indexOf('Queue') === 0 || t.indexOf('Background') === 0;
    });
    if (b) b.click();
  })()`)
  await expect.poll(async () => !(await page.isVisible(PROGRESS_DIALOG)), { timeout: waitBudget(10000) }).toBeTruthy()
  await hold(800)
}

async function queueRows(queue: TauriPage): Promise<{ id: string; status: string }[]> {
  return queue.evaluate<
    { id: string; status: string }[]
  >(`[...document.querySelectorAll('.queue-row')].map(function (r) {
    return { id: r.getAttribute('data-operation-id'), status: r.getAttribute('data-status') };
  })`)
}

/** Presses a queue row's button by its visible word (as `operation-queue.spec.ts` does). */
async function clickQueueRowButton(queue: TauriPage, id: string, word: string): Promise<void> {
  await queue.evaluate(`(function() {
    var row = document.querySelector('.queue-row[data-operation-id=' + JSON.stringify(${JSON.stringify(id)}) + ']');
    if (!row) return;
    var btn = Array.prototype.find.call(row.querySelectorAll('.actions-cell button'), function(b) {
      var copy = b.cloneNode(true);
      copy.querySelectorAll('[aria-hidden="true"]').forEach(function(h) { h.remove(); });
      return copy.textContent.trim() === ${JSON.stringify(word)};
    });
    if (btn) btn.click();
  })()`)
}

/** 6. Undo a copy that just finished, then roll back something done earlier from the operation log. */
export async function undo(page: TauriPage): Promise<void> {
  resetBackup()
  await stageMain(page, 'dark', join(DEMO_ROOT, DEMO.photos), join(DEMO_ROOT, DEMO.backup), {
    ...VIDEO_LOOK,
    zoom: 100,
  })
  // "Earlier": an operation done before the take, which the log rolls back.
  await page.evaluate(
    `window.__TAURI_INTERNALS__.invoke('copy_between_volumes', ${JSON.stringify({
      sourceVolumeId: 'root',
      sourcePaths: [join(DEMO_ROOT, DEMO.rawImports)],
      destVolumeId: 'root',
      destPath: join(DEMO_ROOT, DEMO.backup),
      config: {
        conflictResolution: 'rename',
        progressIntervalMs: 100,
        maxConflictsToShow: 10,
        previewId: null,
        preKnownConflicts: [],
      },
      initiator: 'user',
    })})`,
  )
  await expect
    .poll(
      () =>
        page.evaluate<number>(
          `window.__TAURI_INTERNALS__.invoke('list_operations').then(function(o){ return o.length })`,
        ),
      {
        timeout: waitBudget(30000),
      },
    )
    .toBe(0)
  await setSetting(page, 'fileOperations.keepProgressOpenWhenDone', true)
  await setThrottles(page, 40, 60)
  await mcpCall('move_cursor', { pane: 'left', filename: 'Iceland 2026' })
  await dismissAllToasts(page)
  try {
    const take = await recordWindow(page, { slug: 'undo', theme: 'dark', label: 'main' })
    await beat(take, async () => {
      await hold(800)
      take.note('F5: copy "Iceland 2026" to Backup')
      await copyWithF5(page)
      await expect
        .poll(
          () =>
            page.evaluate<boolean>(
              `[...document.querySelectorAll(${JSON.stringify(`${PROGRESS_DIALOG} button`)})].some(function (b) { return (b.textContent || '').trim() === 'Undo' })`,
            ),
          {
            timeout: waitBudget(60000),
          },
        )
        .toBe(true)
      take.note('done; the dialog offers Undo')
      await hold(1500)
      await clickButtonByText(page, `${PROGRESS_DIALOG} button`, 'Undo')
      await page.waitForSelector('[data-dialog-id="rollback-confirmation"]', waitBudget(10000))
      await hold(1000)
      await confirmRollback(page)
      take.note('undone: the copy is gone again')
      await expect
        .poll(async () => !(await page.isVisible('.modal-overlay')), { timeout: waitBudget(30000) })
        .toBeTruthy()
      await hold(1200)
      take.note('the operation log: everything Cmdr did, ready to roll back')
      await dispatchMenuCommand(page, 'log.operationLog')
      await page.waitForSelector(`${LOG_DIALOG} .op-list`, waitBudget(10000))
      take.markPoster()
      await hold(1800)
      take.note('Roll back the earlier copy of "Raw imports"')
      await page.evaluate(`(function(){
        var b = [...document.querySelectorAll(${JSON.stringify(`${LOG_DIALOG} button`)})].find(function (x) { return (x.textContent || '').trim() === 'Roll back' });
        if (b) b.click();
      })()`)
      await page.waitForSelector('[data-dialog-id="rollback-confirmation"]', waitBudget(10000))
      await hold(1000)
      await confirmRollback(page)
      take.note('rolled back')
      await hold(2500)
    })
  } finally {
    await setThrottles(page, null, null)
    await setSetting(page, 'fileOperations.keepProgressOpenWhenDone', false)
    await drainOperations(page)
    await page.evaluate(
      `(function(){ var c = document.querySelector('${LOG_DIALOG} button[aria-label="Close"]'); if (c) c.click(); })()`,
    )
    await dismissAllToasts(page)
    resetBackup()
  }
}

/** 7. A calm backdrop: a local pane and the NAS side by side, the cursor moving between them. */
export async function hero2(page: TauriPage): Promise<void> {
  await connectNas(page)
  await stageMain(page, 'dark', join(DEMO_ROOT, DEMO.trip), join(nasRoot, 'Photos', '2025', 'Lisbon trip'), VIDEO_LOOK)
  await waitForRowCount(page, 10)
  const take = await recordWindow(page, { slug: 'hero2', theme: 'dark', label: 'main' })
  await beat(take, async () => {
    await ensureExplorerFocused(page)
    take.note('local photos on the left, the NAS on the right')
    for (let i = 0; i < 4; i++) {
      await hold(850)
      await page.keyboard.press('ArrowDown')
    }
    await hold(900)
    take.note('Tab: over to the NAS')
    await page.keyboard.press('Tab')
    take.markPoster()
    for (let i = 0; i < 4; i++) {
      await hold(900)
      await page.keyboard.press('ArrowDown')
    }
    await hold(1000)
    await page.keyboard.press('Tab')
    await hold(1200)
  })
}
