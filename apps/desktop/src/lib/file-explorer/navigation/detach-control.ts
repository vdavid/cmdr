/**
 * Whether a volume gets an eject-or-disconnect control at all, what it says, and
 * which action pressing it runs. The path bar's header chip and every switcher row
 * render the answer this gives; ❌ neither decides for itself.
 *
 * ❗ **One function, because the two surfaces drifted.** The switcher row asked the
 * server question first and the chip never did, so an open SFTP place offered
 * Disconnect in the list and Eject on the chip, three pixels apart — and the chip's
 * button could only ever be refused, since the backend has no eject for a remote
 * volume (`ERR-P7F5Q`). A control whose WORDS and ACTION come from the same answer
 * can't say one thing and do another.
 *
 * ❗ A PHONE says Disconnect and wears the unplug glyph, ❌ never Eject: `adb` has
 * no per-client detach, so nothing is made safe to unplug and the device stays on
 * the cable. Its ACTION is still the ordinary eject path (for ADB that answers
 * `DeviceDisconnect`); only the words and the glyph differ. MTP keeps Eject, which
 * it earns by closing the device session.
 *
 * Three activity states, strongest first: the volume's eject is still running
 * (can't be pressed, a spinner stands in for the glyph, "Ejecting {name}…"); a
 * transfer touches the volume (can't be pressed, says why); idle.
 */
import { tString } from '#lib/intl/messages.svelte.js'
import { volumeScheme } from '#lib/volume-scheme.js'
import { showsDisconnect } from './connection-state'
import { isVolumeEjectable } from './eject-predicate'
import { isServerPlaceRow } from './server-row-actions'
import type { VolumeInfo } from '../types'

/** What pressing the control runs. The words above it are worded for the same one. */
export type DetachAction = 'eject' | 'disconnect-place'

/** The `DetachButton` props for a control. */
export interface DetachButtonLook {
  /** The accessible name, which is also the tooltip. */
  label: string
  /** The glyph shown while no eject runs. */
  icon: 'eject' | 'unplug'
  /** Can't be pressed: an eject is running, or a transfer touches the volume. */
  disabled: boolean
  /** An eject is running, so a spinner stands in for the glyph. */
  ejecting: boolean
}

export interface DetachControl {
  action: DetachAction
  button: DetachButtonLook
}

/** Whether the volume is currently busy with a transfer, or already ejecting. */
export interface DetachActivity {
  busy: boolean
  ejecting: boolean
}

/**
 * The control for `volume`, or `null` when it has none to offer.
 *
 * `null` covers the ordinary case (a fixed disk, a cloud drive, a favorite) and the
 * one that reads like a control but isn't: a `saved` server row, greyed and never
 * connected, has no session to close.
 */
export function detachControlFor(volume: VolumeInfo | undefined, activity: DetachActivity): DetachControl | null {
  if (!volume) return null
  // A server is asked FIRST, since `isVolumeEjectable` says yes to a live session
  // too and would word it as an eject the backend can't perform.
  if (isServerPlaceRow(volume) && showsDisconnect(volume.connectionState)) {
    return { action: 'disconnect-place', button: placeLook(volume, activity) }
  }
  if (isVolumeEjectable(volume)) {
    return { action: 'eject', button: ejectLook(volume, activity) }
  }
  return null
}

/**
 * A server place's words. `ejecting` is always false: the eject store tracks a
 * `diskutil` flight, and dropping a session isn't one.
 */
function placeLook(volume: Pick<VolumeInfo, 'name'>, activity: DetachActivity): DetachButtonLook {
  const label = activity.busy
    ? tString('fileExplorer.navigation.disconnectBusyTooltip')
    : tString('fileExplorer.navigation.disconnectPlaceAriaLabel', { name: volume.name })
  return { label, icon: 'unplug', disabled: activity.busy, ejecting: false }
}

/**
 * What an eject control detaches, which picks its words: a mounted network share and a phone each
 * get their own.
 *
 * ❗ Off the volume ID's scheme, ❌ never `connectionState` or `category`: a share on macOS is an
 * `attached_volume` with `os_mount` or `direct`, and a Linux GVFS share is a `network` row with no
 * state at all, while every one of them is minted `smb-…`. The Rust menu reads the same scheme
 * (`menu_items.rs:detach_word_for_volume_id`), so the two can't word one share two ways.
 */
export type DetachSubject = 'drive' | 'share' | 'phone'

export function detachSubjectOf(volume: Pick<VolumeInfo, 'id'>): DetachSubject {
  const scheme = volumeScheme(volume.id)
  switch (scheme) {
    case 'adb':
      return 'phone'
    case 'smb':
      return 'share'
    case 'root':
    case 'local':
    case 'path':
    case 'sftp':
    case 'webdav':
    case 's3':
    case 'nfs':
    case 'mtp':
    case 'cloud':
    case 'favorite':
    case 'unknown':
      return 'drive'
    default: {
      const unhandled: never = scheme
      return unhandled
    }
  }
}

/**
 * A drive's, a share's, or a phone's words. A share says each OS's own verb (Eject / Disconnect /
 * Unmount, `docs/style-guide.md` § "Platform vocabulary"); a drive says Eject everywhere.
 */
function ejectLook(volume: Pick<VolumeInfo, 'id' | 'name'>, activity: DetachActivity): DetachButtonLook {
  const subject = detachSubjectOf(volume)
  const icon = subject === 'phone' ? 'unplug' : 'eject'
  const name = volume.name
  if (activity.ejecting) return { label: ejectingLabel(subject, name), icon, disabled: true, ejecting: true }
  if (activity.busy) return { label: busyLabel(subject), icon, disabled: true, ejecting: false }
  return { label: idleLabel(subject, name), icon, disabled: false, ejecting: false }
}

function idleLabel(subject: DetachSubject, name: string): string {
  if (subject === 'phone') return tString('adb.disconnectDeviceAriaLabel', { name })
  if (subject === 'share') return tString('fileExplorer.navigation.detachShareAriaLabel', { name })
  return tString('fileExplorer.navigation.ejectVolumeAriaLabel', { name })
}

function ejectingLabel(subject: DetachSubject, name: string): string {
  if (subject === 'phone') return tString('adb.disconnectingDeviceAriaLabel', { name })
  if (subject === 'share') return tString('fileExplorer.navigation.detachingShareAriaLabel', { name })
  return tString('fileExplorer.navigation.ejectingVolumeAriaLabel', { name })
}

function busyLabel(subject: DetachSubject): string {
  if (subject === 'phone') return tString('adb.disconnectBusyTooltip')
  if (subject === 'share') return tString('fileExplorer.navigation.detachShareBusyTooltip')
  return tString('fileExplorer.navigation.ejectBusyTooltip')
}
