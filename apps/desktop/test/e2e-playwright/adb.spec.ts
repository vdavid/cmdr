/**
 * A phone in the switcher, and the pane that opens it.
 *
 * What only an E2E covers here is the WIRE a unit test sees one end of at a
 * time: a `volumes-changed` broadcast actually reaching the switcher's readiness
 * rules, a pane landing on a row through the real navigation path, and — the one
 * that decides whether this feature feels finished — the pane leaving its
 * waiting state ON ITS OWN when a later broadcast says the user tapped Allow.
 * The readiness table, the words, and the connect seam are all unit-tested next
 * to the code.
 *
 * ⚠️ The phone is SYNTHETIC (`emitBackendEvent`), under a serial and an id no
 * real backend can claim, so nothing here touches USB. `volumes-changed`
 * REPLACES the whole list, so the publisher re-broadcasts the real volumes plus
 * the fake row, and the `afterEach` hands the list back with `refresh_volumes`.
 * The app is shared by every spec in the shard; a leaked phone would sit in the
 * next spec's switcher.
 *
 * ❗ A dial in this spec is EXPECTED to stop: no device answers to this serial,
 * and on a machine with no platform tools there is no daemon either. That is
 * what makes the ready transition assertable — the pane moving off the waiting
 * sentence with nothing pressed is the whole claim, whatever it moves onto.
 *
 * The real-device pass (the authorize prompt on hardware, an `unauthorized` →
 * `device` transition mid-session, a 2 GB transfer) stays a by-hand job:
 * `docs/specs/android-adb-backend-follow-ups.md` § 1.
 */

import os from 'node:os'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { test, expect } from './fixtures.js'
import { ensureAppReady, escapeOverlayUntilGone } from './helpers.js'
import {
  openVolumePicker,
  publishSyntheticVolumes,
  restoreRealVolumes,
  switcherNames,
  switcherRowHtml,
  SYNTHETIC_VOLUME_DEFAULTS,
} from './synthetic-volumes.js'
import { initMcpClient, mcpCall } from '../e2e-shared/mcp-client.js'

type PageLike = TauriPage | BrowserPageAdapter

/** "Macintosh HD" on macOS, "Root" on Linux: where both panes start and end. */
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

const PICKER_DROPDOWN = '.volume-dropdown'

/**
 * A phone nothing real can claim. The serial is not a shape any vendor mints,
 * and the id is spelled the way `cmdr_fs::volume::adb_volume_id` would.
 */
const PHONE_SERIAL = 'E2ENOSUCHDEVICE'
const PHONE_NAME = 'E2E synthetic phone'
const PHONE_ID = 'adb-e2e-synthetic-phone-e2e'

/** What the pane says while a phone is showing its own prompt. */
const WAITING_SENTENCE = 'Check your phone and tap Allow.'

type Readiness = { kind: 'ready' } | { kind: 'waiting_for_authorization' } | { kind: 'unavailable'; reason: string }

/** Puts the phone in the store at `readiness`, beside the real volumes. */
async function publishPhone(tauriPage: PageLike, readiness: Readiness): Promise<void> {
  await publishSyntheticVolumes(tauriPage, [
    {
      ...SYNTHETIC_VOLUME_DEFAULTS,
      id: PHONE_ID,
      name: PHONE_NAME,
      path: `adb://${PHONE_SERIAL}`,
      category: 'mobile_device',
      fsType: 'adb',
      // Every device row is ejectable; what the SLOT says is the frontend's call.
      isEjectable: true,
      deviceReadiness: readiness,
    },
  ])
  await expect.poll(async () => (await switcherNames(tauriPage)).includes(PHONE_NAME), { timeout: 5000 }).toBeTruthy()
}

async function closeVolumePicker(tauriPage: PageLike): Promise<void> {
  if (!(await tauriPage.isVisible(PICKER_DROPDOWN))) return
  await escapeOverlayUntilGone(tauriPage, PICKER_DROPDOWN)
}

/** What the connect view is saying right now, or `''` when it isn't on screen. */
async function paneConnectText(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`(document.querySelector('.remote-connect')?.textContent ?? '')`)
}

/**
 * Lands the focused pane on the phone.
 *
 * ❗ The `select_volume` TOOL can't reach it: `mcp/executor/nav.rs` validates the
 * name against the BACKEND's own listing, and a synthetic volume exists only in
 * the frontend store. The event the tool emits does reach it.
 */
async function openPhone(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function () {
    window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'mcp-volume-select',
      payload: { pane: 'left', name: ${JSON.stringify(PHONE_NAME)} },
    });
  })()`)
}

test.describe('A phone in the volume switcher', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await initMcpClient(tauriPage)
    await ensureAppReady(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeVolumePicker(tauriPage)
    await mcpCall('select_volume', { pane: 'left', name: LOCAL_VOLUME_NAME })
    await restoreRealVolumes(tauriPage, PHONE_NAME)
    await closeVolumePicker(tauriPage)
  })

  test('says Disconnect on a phone, because nothing here is made safe to unplug', async ({ tauriPage }) => {
    await publishPhone(tauriPage, { kind: 'ready' })

    const html = await switcherRowHtml(tauriPage, PHONE_NAME)
    expect(html).toContain('Disconnect')
    // ❌ Never "Eject": that word promises the device is safe to remove, and
    // `adb` has no per-client detach to make it true.
    expect(html).not.toContain('Eject')
  })

  test('greys a phone the daemon lists but cannot use, and says why', async ({ tauriPage }) => {
    await publishPhone(tauriPage, { kind: 'unavailable', reason: 'offline' })

    const html = await switcherRowHtml(tauriPage, PHONE_NAME)
    expect(html).toContain('is-unavailable')
    expect(html).toContain('aria-disabled="true"')
  })

  test('keeps a phone waiting for its Allow tap openable, and opens into the wait', async ({ tauriPage }) => {
    await publishPhone(tauriPage, { kind: 'waiting_for_authorization' })

    const html = await switcherRowHtml(tauriPage, PHONE_NAME)
    // ❗ NOT greyed. A disabled row here is the silence that makes people
    // conclude Cmdr cannot see their phone.
    expect(html).not.toContain('is-unavailable')
    await closeVolumePicker(tauriPage)

    await openPhone(tauriPage)

    // The pane says what is happening rather than showing an empty folder or a
    // listing that refused.
    await expect.poll(async () => paneConnectText(tauriPage), { timeout: 15000 }).toContain(WAITING_SENTENCE)
  })

  test('walks in by itself when the phone turns ready, with nothing pressed', async ({ tauriPage }) => {
    await publishPhone(tauriPage, { kind: 'waiting_for_authorization' })
    await closeVolumePicker(tauriPage)
    await openPhone(tauriPage)
    await expect.poll(async () => paneConnectText(tauriPage), { timeout: 15000 }).toContain(WAITING_SENTENCE)

    // What the ADB tracker's `unauthorized` → `device` push does to the store.
    await publishPhone(tauriPage, { kind: 'ready' })
    await closeVolumePicker(tauriPage)

    // ❗ The claim: the pane leaves the wait on its own. ❌ No button was pressed,
    // and none exists to press. Where it lands next is a real dial against a
    // serial no device answers to, so it stops with something else to say —
    // which is exactly what makes "it moved" observable here.
    await expect
      .poll(async () => !(await paneConnectText(tauriPage)).includes(WAITING_SENTENCE), { timeout: 20000 })
      .toBeTruthy()
  })
})
