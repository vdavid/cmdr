/**
 * E2E test pinning the fresh-listing-reuse contract for MTP copy pre-flight (M4).
 *
 * Setup: connect to the virtual MTP device, navigate the left pane into a
 * subfolder of `/DCIM` so the FE has the listing cached and the device's
 * watcher is marked "alive" by `MtpVolume::listing_is_watched`. Select a
 * handful of files, press F5, and assert that the "Verifying before copy…"
 * pre-flight is satisfied from the cache:
 *
 *   1. The transfer-confirmation dialog's scan completes WELL under the
 *      cold-cache MTP cost (USB roundtrip + parent-listing). The bound is
 *      deliberately wider than the cache hit's actual cost (~5 ms) but tight
 *      enough that a regression which falls back to a real `list_directory`
 *      walk would blow past it.
 *   2. `filesFound` matches the selection count.
 *
 * The spec then cancels the dialog: the value here is the pre-flight
 * behavior, not the copy itself.
 *
 * Requires the app to be built with `--features playwright-e2e,virtual-mtp`.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { recreateMtpFixtures, writeMtpDrainSentinel, MTP_FIXTURE_ROOT } from '../e2e-shared/mtp-fixtures.js'
import {
  initMcpClient,
  mcpCall,
  mcpReadResource,
  mcpSelectVolume,
  mcpNavToPath,
  mcpAwaitItem,
} from '../e2e-shared/mcp-client.js'
import {
  dismissOverlay,
  dispatchMenuCommand,
  ensureAppReady,
  expectAndDismissToast,
  getFixtureRoot,
  pollUntil,
  moveCursorToFile,
  pressKey,
  isStateClean,
  LOCAL_VOLUME_NAME,
  TRANSFER_DIALOG,
} from './helpers.js'

const INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'

/**
 * Upper bound on the time from F5-press to the transfer-confirmation dialog
 * showing `scanComplete` (the inline "✓" next to the scan stats). The MTP
 * cold-cache cost for a small folder is hundreds of ms; the cache-hit cost
 * should be ~5 ms plus dialog mount + one scan-preview round-trip. 1500 ms
 * is a ~3× safety margin and well clear of the cold-cache regime.
 */
const SCAN_COMPLETE_BOUND_MS = 1500

// MTP operations go through the virtual device and add protocol overhead.
test.setTimeout(60_000)

/** Reads cmdr://state and returns true when both panes show the local volume. */
async function bothPanesOnLocalVolume(): Promise<boolean> {
  const state = await mcpReadResource('cmdr://state')
  const volumeLines = (state.match(/\n {2}volume: ([^\n]+)/g) ?? []).map((line) => line.replace(/^\n {2}volume: /, ''))
  return volumeLines.length >= 2 && volumeLines[0] === LOCAL_VOLUME_NAME && volumeLines[1] === LOCAL_VOLUME_NAME
}

/** Discovers the mtp:// path prefix for a named MTP storage from cmdr://state. */
async function getMtpVolumePath(storageName: string): Promise<string> {
  const state = await mcpReadResource('cmdr://state')
  const lines = state.split('\n')
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].includes(`name: ${storageName}`) && lines[i + 1]?.includes('id:')) {
      const id = lines[i + 1].trim().replace('id: ', '')
      const [deviceId, storageId] = id.split(':')
      return `mtp://${deviceId}/${storageId}`
    }
  }
  throw new Error(`MTP volume "${storageName}" not found in cmdr://state`)
}

/**
 * Seeds extra files in `/DCIM` so the test can select a non-trivial subset.
 * The base fixture only ships one direct file in `internal/DCIM/`; this brings
 * the top-level file count up to 5 so a 3-file selection is meaningful.
 */
function seedDcimWithExtras(): string[] {
  const dcim = path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM')
  const names = ['cache-a.jpg', 'cache-b.jpg', 'cache-c.jpg', 'cache-d.jpg']
  for (const name of names) {
    fs.writeFileSync(path.join(dcim, name), Buffer.from([0xff, 0xd8, 0xff, 0xe0, ...Buffer.from('cache-test-' + name)]))
  }
  return names
}

test.beforeEach(async ({ tauriPage }) => {
  recreateFixtures(getFixtureRoot())
  await initMcpClient(tauriPage)

  // Pause the virtual MTP watcher across the disk swap, then resync. Mirrors
  // mtp.spec.ts so the rescan can't race with stale FSEvents.
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('pause_virtual_mtp_watcher')`)
  recreateMtpFixtures()
  seedDcimWithExtras()
  // Sentinel goes LAST so per-dir FS-event ordering proves the watcher has
  // observed every preceding write by the time it lands.
  const sentinel = writeMtpDrainSentinel()
  await tauriPage.evaluate(
    `window.__TAURI_INTERNALS__.invoke('resync_virtual_mtp_after_disk_change', { sentinelSuffix: ${JSON.stringify(sentinel)} })`,
  )

  // Reset both panes to the local volume so ensureAppReady's mcp-nav-to-path
  // events aren't rejected by a leftover MTP pane.
  if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME))) {
    await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: ${JSON.stringify(LOCAL_VOLUME_NAME)} } });
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: ${JSON.stringify(LOCAL_VOLUME_NAME)} } });
    })()`)
    await expect.poll(() => bothPanesOnLocalVolume(), { timeout: 5000 }).toBeTruthy()
    // Previously: double-Escape + best-effort modal-overlay poll to clean up
    // dialogs leaked from prior tests. The global afterEach safety net in
    // fixtures.ts now catches and auto-cleans any leaks at the point of leak,
    // so this defensive cleanup is no longer needed.
  }
})

test.describe('MTP copy pre-flight reuses watcher-backed listing', () => {
  test('F5 pre-flight scan completes from cache and reports the right file count', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const mtpPath = await getMtpVolumePath(INTERNAL_STORAGE)

    // Navigate the left pane into the MTP /DCIM folder. This populates
    // `LISTING_CACHE` for the parent and marks the MTP volume as watched
    // (the virtual device is connected). The oracle's
    // `try_get_watched_listing(volume_id, path)` should hit on every entry.
    await mcpSelectVolume('left', INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'DCIM')
    await mcpNavToPath('left', `${mtpPath}/DCIM`)
    await mcpAwaitItem('left', 'cache-a.jpg', 30)

    // Right pane should already be on local right/ from ensureAppReady.
    // Pick three top-level files. The watcher-backed cache holds their
    // size + is_directory, so neither the parent re-listing nor a per-file
    // metadata probe is needed.
    const selection = ['cache-a.jpg', 'cache-b.jpg', 'cache-c.jpg']
    for (const name of selection) {
      await moveCursorToFile(tauriPage, name)
      await pressKey(tauriPage, 'Space')
      await expect
        .poll(
          async () =>
            tauriPage.evaluate<boolean>(
              `!!document.querySelector('.file-pane.is-focused .file-entry[data-filename=' + ${JSON.stringify(JSON.stringify(name))} + '].is-selected')`,
            ),
          { timeout: 2000 },
        )
        .toBeTruthy()
    }

    // The first Space press fires the persistent Quick Look hint toast.
    // Dismiss it before continuing so it doesn't sit through the rest of the
    // test and trip the safety-net leak guard.
    await expectAndDismissToast(tauriPage, 'Space')

    // Open the transfer-confirmation dialog via the same command path F5 hits
    // in production. `dispatchMenuCommand` is unaffected by DOM focus drift.
    const startedAt = Date.now()
    await dispatchMenuCommand(tauriPage, 'file.copy')
    await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

    // Wait for the inline scan-preview to finish. `.scan-checkmark` renders
    // when `scanComplete === true` in `TransferDialog.svelte`. With the
    // fresh-listing oracle the scan is fed from the watcher-backed cache
    // and should converge in low ms; without it, the MTP backend would
    // re-list the parent dir.
    const completedQuickly = await pollUntil(
      tauriPage,
      async () => tauriPage.evaluate<boolean>(`!!document.querySelector('${TRANSFER_DIALOG} .scan-checkmark')`),
      SCAN_COMPLETE_BOUND_MS,
      25,
    )
    const elapsed = Date.now() - startedAt
    expect(
      completedQuickly,
      `scan-preview did not complete within ${String(SCAN_COMPLETE_BOUND_MS)} ms (took ${String(elapsed)} ms)`,
    ).toBe(true)

    // `filesFound` is rendered inside `.scan-stats .scan-stat:nth-child(3) .scan-value`
    // ([size, files, dirs] order, separated by `.scan-divider`). Read it
    // directly: the cache supplied 3 file entries; if the oracle missed and
    // the walker fell back to a real parent listing, this would either match
    // (correct but slow) or wildly overshoot if the bug came back where MTP
    // returned every sibling in the parent dir as part of `filesFound`.
    const filesFoundText = await tauriPage.evaluate<string>(`(function() {
      var stats = document.querySelectorAll('${TRANSFER_DIALOG} .scan-stats .scan-stat .scan-value');
      // Index 1 is files (size is index 0, dirs is index 2).
      return stats[1] ? stats[1].textContent.trim() : '';
    })()`)
    expect(filesFoundText).toBe(String(selection.length))

    // Cancel the dialog. Don't run the copy: this spec is about the
    // pre-flight contract, and skipping the actual transfer keeps the test
    // independent of MTP write throughput.
    await dismissOverlay(tauriPage)

    // Source files must still be on the device (no copy happened).
    for (const name of selection) {
      expect(fs.existsSync(path.join(MTP_FIXTURE_ROOT, 'internal', 'DCIM', name))).toBe(true)
    }

    // Destination must not have any of them.
    const fixtureRoot = getFixtureRoot()
    for (const name of selection) {
      expect(fs.existsSync(path.join(fixtureRoot, 'right', name))).toBe(false)
    }

    // Avoid leaking state for the next test in the spec.
    await mcpCall('refresh', {})
  })
})
