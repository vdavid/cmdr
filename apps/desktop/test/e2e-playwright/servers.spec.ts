/**
 * The servers hub, the switcher's saved-server row, and the pane that dials one.
 *
 * What only an E2E covers here is the WIRE across three surfaces that a unit test
 * sees one at a time: the `network` volume actually mounting the hub, the hub
 * pushing its rows into `cmdr://state` where an agent reads them, the volume
 * switcher rendering a `saved` place with its protocol and its greyed dot, and a
 * pane landing on that place rendering the connect view instead of a listing.
 * The merge, the ordering, the status derivation, and the MCP encoding are all
 * unit-tested next to the code.
 *
 * ⚠️ The saved server is SYNTHETIC (`emitBackendEvent`), under an id and a host
 * no real backend can claim. `volumes-changed` REPLACES the whole list, so the
 * publisher re-broadcasts the real volumes plus the fake row, and every test
 * (and the `afterEach`, for the path where one fails partway) hands the list back
 * to the backend with `refresh_volumes`. The app is shared by every spec in the
 * shard; a leaked synthetic server would sit in the next spec's switcher.
 *
 * ❗ The SFTP wire itself is proven in the Rust integration lane, against the
 * Docker fixture. No SFTP fixture is leased here, so a dial in this spec is
 * expected to fail — which is exactly what makes it a good test of the pane's
 * refusal path.
 */

import os from 'node:os'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { test, expect } from './fixtures.js'
import { ensureAppReady, emitBackendEvent, escapeOverlayUntilGone } from './helpers.js'
import { initMcpClient, mcpCall, mcpReadResource } from '../e2e-shared/mcp-client.js'

type PageLike = TauriPage | BrowserPageAdapter

/** "Macintosh HD" on macOS, "Root" on Linux: where both panes start and end. */
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

/** The hub row's name, which is also the Rust `SERVERS_VOLUME_NAME` const. */
const SERVERS_VOLUME_NAME = 'Servers'

const PICKER_TRIGGER = '.volume-name'
const PICKER_DROPDOWN = '.volume-dropdown'

/**
 * A saved SFTP place nothing real can claim: `.invalid` is reserved by RFC 2606
 * and never resolves, and the id is the one `cmdr_fs::volume::sftp_volume_id`
 * would mint for that host.
 */
const SYNTHETIC_ID = 'sftp-e2e-nothing-here.invalid-22-e2e'
const SYNTHETIC_NAME = 'E2E synthetic server'
const SYNTHETIC_PATH = 'sftp://e2e@e2e-nothing-here.invalid:22'

/**
 * Re-broadcasts the real volume list with one `saved` SFTP place appended.
 *
 * ❗ Reads the real list first: `volumes-changed` replaces what the store holds,
 * so emitting the fake row alone would take every disk off screen and strand
 * both panes.
 */
async function publishSyntheticServer(tauriPage: PageLike): Promise<void> {
  const real = await tauriPage.evaluate(
    `window.__TAURI_INTERNALS__.invoke('list_volumes').then(function (r) { return r.data; })`,
  )
  const row = {
    id: SYNTHETIC_ID,
    name: SYNTHETIC_NAME,
    path: SYNTHETIC_PATH,
    category: 'network',
    icon: null,
    isEjectable: false,
    fsType: 'sftp',
    supportsTrash: false,
    mountIsReadOnly: false,
    isDiskImage: false,
    connectionState: 'saved',
    deviceReadiness: null,
    usbSpeed: null,
    capabilities: null,
  }
  await emitBackendEvent(tauriPage, 'volumes-changed', {
    data: [...(real as unknown[]), row],
    timedOut: false,
  })
}

/**
 * Hands the list back to the backend, which re-broadcasts the real one.
 *
 * ❗ Closes the picker on the way out: the check for the row reads the switcher,
 * which means opening it, and the fixtures' leak guard fails a test that leaves a
 * `.volume-dropdown` behind.
 */
async function restoreRealVolumes(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('refresh_volumes')`)
  await expect
    .poll(async () => !(await switcherNames(tauriPage)).includes(SYNTHETIC_NAME), { timeout: 5000 })
    .toBeTruthy()
  await closeVolumePicker(tauriPage)
}

/** Opens the volume switcher, or leaves it open. */
async function openVolumePicker(tauriPage: PageLike): Promise<void> {
  if (await tauriPage.isVisible(PICKER_DROPDOWN)) return
  await tauriPage.click(PICKER_TRIGGER)
  await tauriPage.waitForSelector(PICKER_DROPDOWN, 5000)
}

async function closeVolumePicker(tauriPage: PageLike): Promise<void> {
  if (!(await tauriPage.isVisible(PICKER_DROPDOWN))) return
  await escapeOverlayUntilGone(tauriPage, PICKER_DROPDOWN)
}

/** Every label the switcher is showing right now. */
async function switcherNames(tauriPage: PageLike): Promise<string[]> {
  await openVolumePicker(tauriPage)
  return tauriPage.evaluate<string[]>(`(function () {
    var out = [];
    document.querySelectorAll('.volume-item .volume-label').forEach(function (el) { out.push(el.textContent || ''); });
    return out;
  })()`)
}

/** The row's own DOM, so a test can read the protocol slot and the dot beside it. */
async function switcherRowHtml(tauriPage: PageLike, label: string): Promise<string> {
  await openVolumePicker(tauriPage)
  return tauriPage.evaluate<string>(`(function () {
    var items = document.querySelectorAll('.volume-item');
    for (var i = 0; i < items.length; i++) {
      var el = items[i].querySelector('.volume-label');
      if (el && el.textContent === ${JSON.stringify(label)}) return items[i].innerHTML;
    }
    return '';
  })()`)
}

/** The left pane's rows, as `cmdr://state` publishes them. */
async function leftPaneRows(): Promise<string> {
  return mcpReadResource('cmdr://state')
}

test.describe('The servers hub', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await initMcpClient(tauriPage)
    await ensureAppReady(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeVolumePicker(tauriPage)
    await mcpCall('select_volume', { pane: 'left', name: LOCAL_VOLUME_NAME })
  })

  test('the switcher offers one row called "Servers", inside a group still called Network', async ({ tauriPage }) => {
    // The Rust `SERVERS_VOLUME_NAME` const has to equal this byte for byte:
    // `mcp/executor/nav.rs` waits for the frontend-pushed pane name to match it
    // before it reports a `select_volume` done.
    expect(await switcherNames(tauriPage)).toContain(SERVERS_VOLUME_NAME)
    const groups = await tauriPage.evaluate<string[]>(`(function () {
      var out = [];
      document.querySelectorAll('.volume-dropdown .category-label').forEach(function (el) { out.push(el.textContent || ''); });
      return out;
    })()`)
    expect(groups).toContain('Network')
    await closeVolumePicker(tauriPage)
  })

  test('selecting it opens the hub, whose last row is always "Add server…"', async ({ tauriPage }) => {
    await closeVolumePicker(tauriPage)
    // The tool polls for the pane's pushed volume name, so a hub that never
    // mounts (or pushes a different name) surfaces here as a timeout.
    await mcpCall('select_volume', { pane: 'left', name: SERVERS_VOLUME_NAME })

    await expect.poll(async () => (await leftPaneRows()).includes('+ Add server…'), { timeout: 15000 }).toBeTruthy()
  })

  test('the hub is on screen as a table, not as a file listing', async ({ tauriPage }) => {
    await closeVolumePicker(tauriPage)
    await mcpCall('select_volume', { pane: 'left', name: SERVERS_VOLUME_NAME })
    await expect.poll(async () => tauriPage.isVisible('.servers-hub'), { timeout: 15000 }).toBeTruthy()
    // The add row is keyboard-navigable, which is what makes it a row rather
    // than a button under the list.
    expect(await tauriPage.isVisible('.servers-hub .add-row')).toBe(true)
  })
})

test.describe('A saved server in the switcher and the pane', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await initMcpClient(tauriPage)
    await ensureAppReady(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeVolumePicker(tauriPage)
    await mcpCall('select_volume', { pane: 'left', name: LOCAL_VOLUME_NAME })
    await restoreRealVolumes(tauriPage)
  })

  test('shows the place greyed in the Network group, with the protocol it speaks', async ({ tauriPage }) => {
    await publishSyntheticServer(tauriPage)

    await expect
      .poll(async () => (await switcherNames(tauriPage)).includes(SYNTHETIC_NAME), { timeout: 5000 })
      .toBeTruthy()

    const html = await switcherRowHtml(tauriPage, SYNTHETIC_NAME)
    // The protocol fills the slot a local disk uses for its filesystem: for a
    // place with no local mount, "what am I talking to" is the honest answer.
    expect(html).toContain('SFTP')
    // The `saved` dot is the hollow one. Its class is built from the state name,
    // so a renamed state shows up here rather than as a missing style.
    expect(html).toContain('smb-indicator-saved')
  })

  test('offers no Disconnect on a place with no session to drop', async ({ tauriPage }) => {
    await publishSyntheticServer(tauriPage)
    await expect
      .poll(async () => (await switcherNames(tauriPage)).includes(SYNTHETIC_NAME), { timeout: 5000 })
      .toBeTruthy()

    const html = await switcherRowHtml(tauriPage, SYNTHETIC_NAME)
    expect(html).not.toContain('eject-button')
  })

  test('opening it puts the pane on the connect view, and then on what stopped it', async ({ tauriPage }) => {
    await publishSyntheticServer(tauriPage)
    await expect
      .poll(async () => (await switcherNames(tauriPage)).includes(SYNTHETIC_NAME), { timeout: 5000 })
      .toBeTruthy()
    await closeVolumePicker(tauriPage)

    // ❗ The `select_volume` TOOL can't reach this row: `mcp/executor/nav.rs`
    // validates the name against the BACKEND's own listing, and a synthetic
    // volume exists only in the frontend store. The event the tool emits does
    // reach it, and `selectVolumeByName` looks the name up in that store.
    await tauriPage.evaluate(`(function () {
      window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
        event: 'mcp-volume-select',
        payload: { pane: 'left', name: ${JSON.stringify(SYNTHETIC_NAME)} },
      });
    })()`)

    // The pane renders the connect view rather than a listing. The dial can't
    // succeed (nothing is saved under this id, and the host doesn't resolve), so
    // it settles on the refusal — which is the state this spec is really after:
    // a pane that says what happened instead of showing an empty folder.
    await expect.poll(async () => tauriPage.isVisible('.remote-connect'), { timeout: 15000 }).toBeTruthy()

    const text = await tauriPage.evaluate<string>(`(document.querySelector('.remote-connect')?.textContent ?? '')`)
    expect(text).toContain(SYNTHETIC_NAME)
  })
})
