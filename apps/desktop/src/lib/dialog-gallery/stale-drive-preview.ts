/**
 * The `drive-index-stale` preview: the gallery's one EVENT-SEEDED row.
 *
 * `StaleDriveDialog` takes no props and holds its own `open` flag. It shows only
 * when an `index-freshness-changed` event lands with `freshness: 'stale'` for a
 * non-`root` volume, AND the `indexing.staleNotify` setting is on, AND the
 * persisted one-shot flag is still clear. So the preview arranges those three
 * preconditions and emits the REAL event; the dialog's own listener does the
 * rest, exactly as it does when a drive comes back stale for real.
 *
 * Two things this deliberately doesn't do:
 *
 * - ❌ It doesn't hand the dialog a synthetic volume id, or one the shipping
 *   dialog would never name. `volumeName()` falls back to the raw id for a
 *   volume that isn't in the store, so a made-up id renders `vol-abc123` in the
 *   body copy: a preview that looks fine and is silently wrong, which is the
 *   failure this instrument exists to prevent. The volume comes from the live
 *   store, narrowed to the categories whose index can really go stale.
 * - ❌ It doesn't restore the setting or the flag afterwards. Both writes are
 *   real, and the dialog itself writes them too ("Never show again" turns the
 *   setting off; showing stamps the one-shot), so a restore would fight the
 *   component. The gallery row discloses them instead.
 */

import { getSetting, setSetting } from '$lib/settings'
import { getVolumes } from '$lib/stores/volume-store.svelte'
import { isDriveRow } from '$lib/file-explorer/navigation/drive-index-manager.svelte'
import { resetFirstStaleDialogShown } from '$lib/indexing/drive-index-prefs'
import { emitIndexFreshnessChanged } from '$lib/tauri-commands/indexing'
import { addToast } from '$lib/ui/toast/toast-store.svelte'
import type { LocationCategory, VolumeInfo } from '$lib/file-explorer/types'
import { getAppLogger } from '$lib/logging/logger'
import { closeGalleryDialog } from './gallery-state.svelte'
import { staleDriveFixtures } from './fixtures/indexing'

const log = getAppLogger('dialogGallery')

/**
 * What a trigger did, so the caller (and the test) can see which branch ran
 * rather than inferring it from a side effect.
 */
export type StaleDrivePreviewOutcome =
  /** The event went out for this volume; the dialog opens if it's listening. */
  | { kind: 'emitted'; volumeId: string }
  /** No drive to name, so nothing was emitted and the reviewer was told why. */
  | { kind: 'no-drive' }
  /** A state id the row doesn't advertise. Same "open nothing" rule as a missing fixture. */
  | { kind: 'unknown-state' }

/**
 * The volume categories whose index can actually go stale: a non-journaled one
 * the app can lose sight of while it's away. External disks AND mounted SMB
 * shares are `attached_volume`; MTP devices are `mobile_device`. The local disk
 * and the cloud-drive folders sitting on it are journaled, so the shipping
 * dialog would never name them, and a preview that did would put a scenario on
 * screen the app can't produce. (`LocationCategory.network` is unconstructed in
 * the volume list — only the synthetic switcher row uses it.)
 */
const STALEABLE_CATEGORIES = new Set<LocationCategory>(['attached_volume', 'mobile_device'])

/**
 * The drive the preview names, or `undefined` when this machine has none.
 *
 * `isDriveRow` is the app's own chokepoint for "a real drive that can carry an
 * index badge", so this also drops favorites, the synthetic `network` /
 * `search-results` rows, and mounted disk images (deliberately never indexed).
 * Store order decides the rest, which puts an attached drive ahead of a share.
 */
function pickStaleDrive(): VolumeInfo | undefined {
  return getVolumes().find((volume) => STALEABLE_CATEGORIES.has(volume.category) && isDriveRow(volume))
}

/**
 * Opens the stale-drive explainer through its real event.
 *
 * The one-shot reset runs on EVERY trigger: the dialog stamps the flag the
 * moment it shows, so without this the row would work once per machine.
 */
export async function openStaleDrivePreview(stateId: string): Promise<StaleDrivePreviewOutcome> {
  const buildPayload = staleDriveFixtures[stateId]
  if (buildPayload === undefined) return { kind: 'unknown-state' }

  const drive = pickStaleDrive()
  if (!drive) {
    // The honest outcome, not a fallback preview: with no drive in the store the
    // dialog would name a raw volume id, and a design review can't trust a
    // gallery that shows one silently wrong screen.
    addToast(
      'The stale-drive dialog names a real drive, and this machine has none mounted. Plug in an external drive (or connect a share) and trigger it again.',
      { level: 'info', timeoutMs: 8000 },
    )
    log.info('Dialog gallery: no external drive to name, so drive-index-stale stayed closed')
    return { kind: 'no-drive' }
  }

  // A previewed dialog would sit visible underneath this one, since the stale
  // dialog isn't the gallery's own preview and closes on its own terms.
  closeGalleryDialog()

  if (!getSetting('indexing.staleNotify')) setSetting('indexing.staleNotify', true)
  resetFirstStaleDialogShown()

  await emitIndexFreshnessChanged(buildPayload(drive.id))
  return { kind: 'emitted', volumeId: drive.id }
}
