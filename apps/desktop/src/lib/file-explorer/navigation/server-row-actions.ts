/**
 * What a server row in the volume switcher offers, and what each offer does.
 *
 * Lives beside the switcher rather than inside `VolumeBreadcrumb.svelte` because
 * TWO surfaces raise the same menu (the switcher row and the hub row) and the
 * picked action lands in a THIRD (`DualPaneExplorer`, which
 * owns the `volume-context-action` listener). One module means the three can't
 * drift on which items a row has or what confirming one costs.
 *
 * ❗ **A server says Disconnect, never Eject** (`docs/specs/servers-hub-plan.md`
 * § D6): "Eject" promises safe-to-unplug and a server has nothing to unplug.
 * Disconnecting keeps the row: a pinned place comes back `saved`.
 */

import {
  disconnectPlace,
  forgetServer,
  forgetServerSecret,
  hasServerSecret,
  listSavedServers,
  setPlacePinned,
  showVolumeRowContextMenu,
  type ServerRowMenu,
} from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { confirmDialog } from '$lib/utils/confirm-dialog'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { isServerVolumeId } from '$lib/servers/server-path-utils'
import { openEditServerSheet } from '$lib/servers/open-sign-in'
import { showsDisconnect } from './connection-state'
import type { VolumeContextActionKind } from '$lib/ipc/bindings'
import type { VolumeInfo } from '../types'

const log = getAppLogger('fileExplorer')

/**
 * Whether the servers command family owns this row.
 *
 * ❗ Off the VOLUME ID, which the two id minters spell (`sftp-…`, `webdav-…`), ❌
 * never off `category === 'network'`: a mounted SMB share is one of those, and
 * its session is an OS mount that `disconnectPlace` doesn't speak. SMB shares
 * reach the switcher as ordinary mounted volumes and leave through Eject; they
 * join this family when their unmount does.
 */
export function isServerPlaceRow(volume: VolumeInfo): boolean {
  return volume.category === 'network' && isServerVolumeId(volume.id)
}

/**
 * Raises the native menu for a server row: Disconnect, Pin to switcher / Unpin,
 * Forget saved password, Forget server.
 *
 * ❗ Which items apply is the CALLER's reading of the row, so the two store
 * questions are asked here rather than on Rust's popup path, where "is a secret
 * stored?" would put a Keychain read in front of the menu appearing. `busy` is
 * the backend's own answer and it fills that in. A store that doesn't answer
 * costs the row one item, never the menu.
 */
export async function openServerRowMenu(volume: VolumeInfo): Promise<void> {
  const [hasSavedSecret, isSaved] = await Promise.all([
    hasServerSecret(volume.id).catch(() => false),
    listSavedServers()
      .then((servers) => servers.some((server) => server.places.some((place) => place.volumeId === volume.id)))
      .catch(() => false),
  ])
  const server: ServerRowMenu = {
    showsDisconnect: showsDisconnect(volume.connectionState),
    isSaved,
    hasSavedSecret,
    pinned: volume.pinned === true,
  }
  await showVolumeRowContextMenu(volume.id, volume.name, false, false, server)
}

/**
 * Drops a place's session, from the row's Disconnect control or its menu item.
 *
 * The row SURVIVES: a pinned place comes back as a `saved` row, and a pane
 * standing on it goes home through the `VolumeUnmounted` broadcast the command
 * emits. A `false` answer means there was no session left, which a click racing
 * a dropped connection legitimately is.
 */
export async function disconnectServerPlace(volumeId: string, volumeName: string): Promise<void> {
  try {
    await disconnectPlace(volumeId)
  } catch (e) {
    refused('Disconnecting', volumeId, e, 'fileExplorer.navigation.disconnectRefusedToast', volumeName)
  }
}

/**
 * Asks first, then drops the server, its places, and their pins.
 *
 * ❗ Confirmed because it is not undoable from the UI: the entry, the pins, and
 * the tab that stood on it all go. The stored password does NOT: that is
 * [`forgetSavedSecret`], a separate request the menu offers separately.
 */
export async function forgetSavedServer(volumeId: string, volumeName: string): Promise<void> {
  const confirmed = await confirmDialog(
    tString('fileExplorer.navigation.forgetServerConfirm', { name: volumeName }),
    tString('fileExplorer.navigation.forgetServerConfirmTitle'),
  )
  if (!confirmed) return
  try {
    await forgetServer(volumeId)
  } catch (e) {
    refused('Forgetting', volumeId, e, 'fileExplorer.navigation.forgetServerRefusedToast', volumeName)
  }
}

/**
 * Moves a place's pin, from the "Pin / unpin server" command.
 *
 * ❗ Not confirmed, unlike the two forgets: unpinning loses nothing (the server
 * stays saved and stays in the hub), and the same command puts it back.
 */
export async function setServerPinned(volumeId: string, volumeName: string, pinned: boolean): Promise<void> {
  try {
    await setPlacePinned(volumeId, pinned)
    addToast(
      tString(pinned ? 'fileExplorer.navigation.serverPinnedToast' : 'fileExplorer.navigation.serverUnpinnedToast', {
        name: volumeName,
      }),
      { level: 'success' },
    )
  } catch (e) {
    refused('Pinning', volumeId, e, 'fileExplorer.navigation.pinRefusedToast', volumeName)
  }
}

/** Asks first, then forgets the place's remembered secret, keeping the server. */
export async function forgetSavedSecret(volumeId: string, volumeName: string): Promise<void> {
  const confirmed = await confirmDialog(
    tString('fileExplorer.navigation.forgetSecretConfirm', { name: volumeName }),
    tString('fileExplorer.navigation.forgetSecretConfirmTitle'),
  )
  if (!confirmed) return
  try {
    await forgetServerSecret(volumeId)
  } catch (e) {
    refused('Forgetting the secret for', volumeId, e, 'fileExplorer.navigation.forgetSecretRefusedToast', volumeName)
  }
}

/**
 * Runs the item the user picked from a server row's native menu, or the palette
 * command that mirrors it.
 *
 * ❗ The one consumer, wired from `DualPaneExplorer` (which owns the
 * `volume-context-action` listener), because these actions are global: the row
 * can be in either pane's switcher, or in the hub. `eject` and the two favorite
 * actions are NOT here — their owners are the eject listener and the open
 * dropdown respectively.
 *
 * `open`, `pin`, `unpin`, and `edit` are typed variants with no producer yet:
 * Rust builds only the three items below for a server row today, and the rest
 * arrive with the hub (`pin` / `unpin` / `open`) and the sign-in sheet (`edit`).
 * They log rather than silently doing nothing, so the first menu that emits one
 * says so in the log instead of looking broken.
 */
export async function runServerRowAction(payload: {
  action: VolumeContextActionKind
  volumeId: string
  volumeName: string
  /**
   * Navigates the focused pane onto the place. Supplied by the surface that owns
   * a `navigate()` transaction; without one, Open logs rather than pretending.
   */
  onOpen?: (volumeId: string) => void
}): Promise<void> {
  const { action, volumeId, volumeName } = payload
  switch (action) {
    case 'disconnect':
      await disconnectServerPlace(volumeId, volumeName)
      return
    case 'forget-secret':
      await forgetSavedSecret(volumeId, volumeName)
      return
    case 'forget-server':
      await forgetSavedServer(volumeId, volumeName)
      return
    case 'pin':
    case 'unpin':
      await setServerPinned(volumeId, volumeName, action === 'pin')
      return
    case 'open':
      // The `navigate()` transaction lives in the pane, so the caller supplies
      // it. A menu raised somewhere with no pane to move logs rather than
      // pretending it did something.
      if (payload.onOpen) payload.onOpen(volumeId)
      else log.info('Open on {volumeId} had no pane to navigate', { volumeId })
      return
    case 'edit':
      await editServer(volumeId, volumeName)
      return
    case 'eject':
    case 'rename-favorite':
    case 'remove-favorite':
      // Owned elsewhere: the eject listener, and the open switcher dropdown.
      return
  }
}

/**
 * One sentence for the user, the raw value for the log.
 *
 * ❌ Never `String(e)` in the toast: these three commands answer a bool, so
 * anything thrown is the IPC transport itself — untranslated diagnostic text,
 * which is exactly what a person must not be handed.
 */
function refused(
  what: string,
  volumeId: string,
  error: unknown,
  key:
    | 'fileExplorer.navigation.disconnectRefusedToast'
    | 'fileExplorer.navigation.forgetServerRefusedToast'
    | 'fileExplorer.navigation.forgetSecretRefusedToast'
    | 'fileExplorer.navigation.pinRefusedToast',
  volumeName: string,
): void {
  log.warn('{what} {volumeId} broke down: {error}', { what, volumeId, error: String(error) })
  addToast(tString(key, { name: volumeName }), { level: 'error' })
}

/**
 * Opens the sign-in sheet on this server, prefilled.
 *
 * ❗ Looked up in the saved list rather than reconstructed from the row: a
 * `VolumeInfo` carries no key file, no remote folder, and no auto-reconnect
 * switch, and an edit form seeded from half a server would save the other half
 * away.
 */
async function editServer(volumeId: string, volumeName: string): Promise<void> {
  const saved = await listSavedServers().catch((e: unknown) => {
    log.warn('Reading the saved servers to edit {volumeId} broke down: {error}', { volumeId, error: String(e) })
    return []
  })
  const server = saved.find((entry) => entry.places.some((place) => place.volumeId === volumeId))
  if (!server) {
    // A forget that raced the menu. Nothing to edit and nothing worth saying:
    // the row is already gone from the switcher.
    log.info('Editing {volumeName} found no saved server behind it', { volumeName })
    return
  }
  await openEditServerSheet(server)
}
