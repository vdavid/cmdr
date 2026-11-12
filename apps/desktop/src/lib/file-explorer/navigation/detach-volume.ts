import { ejectAllOnDisk, ejectVolume } from '#lib/tauri-commands/index.js'
import { isVolumeBusy, isVolumeEjecting } from '#lib/stores/volume-busy-store.svelte.js'
import { addToast } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { asEjectError } from './eject-error'
import { ejectRefusalReport, wordEjectRefusal } from './eject-error-messages'
import { disconnectServerPlace } from './server-row-actions'
import { detachSubjectOf, type DetachAction } from './detach-control'
import type { VolumeInfo } from '../types'

/**
 * Runs the action `detachControlFor` chose for this volume.
 *
 * ❗ Both surfaces go through here, so the button that says Disconnect can't send an
 * eject: the words and the action come from one answer and are spent in one place.
 */
export async function runDetach(volume: VolumeInfo, action: DetachAction): Promise<void> {
  if (action === 'disconnect-place') {
    if (isVolumeBusy(volume.id)) return
    await disconnectServerPlace(volume.id, volume.name)
    return
  }
  await detachVolume(volume)
}

/**
 * Press the eject-or-disconnect control: the chip's and every switcher row's run this.
 *
 * ❗ It guards again on its own. The controls render disabled while a transfer touches the
 * volume or its eject is still running, but a keyboard or edge path could still reach here:
 * don't tear down a volume mid-transfer, and don't ask twice (the backend would only join
 * the eject already running).
 *
 * Success mostly says nothing: the volume leaves the list on its own through `volume-unmounted` /
 * `mtp-device-disconnected`, which is the feedback. A refusal speaks the catalog through
 * `wordEjectRefusal`, ❌ never `diskutil`'s stderr.
 */
export async function detachVolume(volume: VolumeInfo): Promise<void> {
  if (isVolumeBusy(volume.id) || isVolumeEjecting(volume.id)) return
  await ejectAndSay(volume.id, volume.name)
}

/** The volume menu's "Eject all", with the same guards as {@link detachVolume}. */
export async function detachAllOnDisk(volume: VolumeInfo): Promise<void> {
  if (isVolumeBusy(volume.id) || isVolumeEjecting(volume.id)) return
  await ejectAllAndSay(volume.id)
}

/**
 * Ejects one volume and says what the person needs to hear. A drive that stays attached is the one
 * success with words: the row leaving the list would otherwise read as "safe to unplug".
 */
export async function ejectAndSay(volumeId: string, volumeName: string): Promise<void> {
  try {
    const outcome = await ejectVolume(volumeId)
    if (outcome === 'volumeEjectedDiskStays') {
      addToast(tString('fileExplorer.pane.ejectedDriveStaysToast', { volumeName }), { level: 'info' })
    }
  } catch (e) {
    // A share's toast says its OS's verb ("Couldn't unmount media: …"), as its control did.
    const key =
      detachSubjectOf({ id: volumeId }) === 'share'
        ? 'fileExplorer.pane.detachShareFailedToast'
        : 'fileExplorer.pane.ejectFailedToast'
    addToast(tString(key, { volumeName, message: wordEjectRefusal(e) }), {
      level: 'error',
      report: ejectRefusalReport(e),
    })
  }
}

/**
 * Ejects every volume on the disk, saying whether a refusal left the drive exactly as it was.
 * Success says nothing, since every row leaves the list.
 */
export async function ejectAllAndSay(volumeId: string): Promise<void> {
  try {
    await ejectAllOnDisk(volumeId)
  } catch (e) {
    const key =
      asEjectError(e)?.type === 'heldBeforeEject'
        ? 'fileExplorer.pane.ejectAllHeldToast'
        : 'fileExplorer.pane.ejectAllFailedToast'
    addToast(tString(key, { message: wordEjectRefusal(e) }), { level: 'error', report: ejectRefusalReport(e) })
  }
}
