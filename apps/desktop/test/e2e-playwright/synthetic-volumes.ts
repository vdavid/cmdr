/**
 * Putting a volume the backend has never heard of into the frontend's store, and
 * taking it back out.
 *
 * ⚠️ `volumes-changed` REPLACES the whole list, so a publisher that emitted only
 * the fake row would take every disk off screen and strand both panes. Every
 * function here reads the REAL list first and appends.
 *
 * ⚠️ The app is shared by every spec in the shard, so a synthetic row outlives
 * the test that made it: each caller hands the list back with `refresh_volumes`
 * in its own `afterEach`, and every id used here is one no real backend can mint.
 *
 * Two specs need this (`servers.spec.ts` for a saved server, `adb.spec.ts` for a
 * phone), which is why it is a module rather than a copy in each: the reading of
 * `volumes-changed` is one thing, and two copies would drift the moment the
 * payload grows a field.
 */

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { expect } from './fixtures.js'
import { emitBackendEvent } from './helpers.js'

type PageLike = TauriPage | BrowserPageAdapter

/**
 * One row's worth of `VolumeInfo`, as the store holds it. Every field the
 * frontend reads has a value here, so a caller overrides only what its test is
 * about; the type stays loose on purpose, because this is a WIRE payload rather
 * than the app's own type and pinning it would couple the harness to `bindings.ts`.
 */
export type SyntheticVolume = Record<string, unknown>

/** Everything a row carries, at its most boring. Callers spread and override. */
export const SYNTHETIC_VOLUME_DEFAULTS: SyntheticVolume = {
  icon: null,
  isEjectable: false,
  supportsTrash: false,
  mountIsReadOnly: false,
  isDiskImage: false,
  connectionState: null,
  pinned: null,
  deviceReadiness: null,
  usbSpeed: null,
  capabilities: null,
}

/** Re-broadcasts the real volume list with `rows` appended. */
export async function publishSyntheticVolumes(tauriPage: PageLike, rows: SyntheticVolume[]): Promise<void> {
  const real = await tauriPage.evaluate(
    `window.__TAURI_INTERNALS__.invoke('list_volumes').then(function (r) { return r.data; })`,
  )
  await emitBackendEvent(tauriPage, 'volumes-changed', {
    data: [...(real as unknown[]), ...rows],
    timedOut: false,
  })
}

/**
 * Hands the list back to the backend, which re-broadcasts the real one, and
 * waits until `goneName` has actually left the switcher.
 */
export async function restoreRealVolumes(tauriPage: PageLike, goneName: string): Promise<void> {
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('refresh_volumes')`)
  await expect.poll(async () => !(await switcherNames(tauriPage)).includes(goneName), { timeout: 5000 }).toBeTruthy()
}

const PICKER_TRIGGER = '.volume-name'
const PICKER_DROPDOWN = '.volume-dropdown'

/** Opens the volume switcher, or leaves it open. */
export async function openVolumePicker(tauriPage: PageLike): Promise<void> {
  if (await tauriPage.isVisible(PICKER_DROPDOWN)) return
  await tauriPage.click(PICKER_TRIGGER)
  await tauriPage.waitForSelector(PICKER_DROPDOWN, 5000)
}

/** Every label the switcher is showing right now. Opens the picker to read it. */
export async function switcherNames(tauriPage: PageLike): Promise<string[]> {
  await openVolumePicker(tauriPage)
  return tauriPage.evaluate<string[]>(`(function () {
    var out = [];
    document.querySelectorAll('.volume-item .volume-label').forEach(function (el) { out.push(el.textContent || ''); });
    return out;
  })()`)
}

/** One row's own DOM, so a test can read the controls and the state classes on it. */
export async function switcherRowHtml(tauriPage: PageLike, label: string): Promise<string> {
  await openVolumePicker(tauriPage)
  return tauriPage.evaluate<string>(`(function () {
    var items = document.querySelectorAll('.volume-item');
    for (var i = 0; i < items.length; i++) {
      var el = items[i].querySelector('.volume-label');
      if (el && el.textContent === ${JSON.stringify(label)}) return items[i].outerHTML;
    }
    return '';
  })()`)
}
