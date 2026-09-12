/**
 * What a volume's eject-or-disconnect control says and does: the path bar's
 * header chip and every dropdown row render the same one.
 *
 * ❗ A PHONE says Disconnect and wears the unplug glyph, ❌ never Eject: `adb` has
 * no per-client detach, so nothing is made safe to unplug and the device stays
 * on the cable. The ACTION is still the ordinary eject path (for ADB that
 * answers `DeviceDisconnect`); only the words and the glyph differ. MTP keeps
 * Eject, which it earns by closing the device session.
 *
 * Three states, strongest first: the volume's eject is still running (can't be
 * pressed, a spinner stands in for the glyph, "Ejecting {name}…"); a transfer
 * touches the volume (can't be pressed, says why); idle.
 */
import { tString } from '$lib/intl/messages.svelte'
import { isAdbVolumeId } from '$lib/adb/adb-path-utils'
import type { VolumeInfo } from '../types'

export interface DetachControl {
  /** The accessible name, which is also the tooltip. */
  label: string
  /** The glyph shown while no eject runs. */
  icon: 'eject' | 'unplug'
  /** Can't be pressed: an eject is running, or a transfer touches the volume. */
  disabled: boolean
  /** An eject is running, so a spinner stands in for the glyph. */
  ejecting: boolean
}

/** What the control shows for `volume`, given whether it's busy or ejecting right now. */
export function detachControl(
  volume: Pick<VolumeInfo, 'id' | 'name'>,
  activity: { busy: boolean; ejecting: boolean },
): DetachControl {
  const onAPhone = isAdbVolumeId(volume.id)
  const icon = onAPhone ? 'unplug' : 'eject'
  const name = volume.name
  if (activity.ejecting) {
    const label = onAPhone
      ? tString('adb.disconnectingDeviceAriaLabel', { name })
      : tString('fileExplorer.navigation.ejectingVolumeAriaLabel', { name })
    return { label, icon, disabled: true, ejecting: true }
  }
  if (activity.busy) {
    const label = onAPhone ? tString('adb.disconnectBusyTooltip') : tString('fileExplorer.navigation.ejectBusyTooltip')
    return { label, icon, disabled: true, ejecting: false }
  }
  const label = onAPhone
    ? tString('adb.disconnectDeviceAriaLabel', { name })
    : tString('fileExplorer.navigation.ejectVolumeAriaLabel', { name })
  return { label, icon, disabled: false, ejecting: false }
}
