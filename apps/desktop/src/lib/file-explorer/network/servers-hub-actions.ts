/**
 * What a hub row's F8 and right-click do, and what the SMB host menu answers.
 *
 * A factory rather than lines in `ServersHub.svelte`, so the component stays the
 * table, the cursor, and the keys. These are the parts that ask a question,
 * write to a store, and toast — the parts worth reading on their own.
 *
 * ❗ **A one-place row and an SMB host take different paths at every branch**, and
 * that is the whole reason this module exists as a unit. A one-place row is a
 * PLACE the servers family speaks for (`forgetSavedServer`, `openServerRowMenu`);
 * an SMB host is a manual-server entry whose "disconnect" unmounts shares rather
 * than dropping a session. Mixing the two is how a Forget removes the wrong
 * thing.
 */

import { disconnectNetworkHost, removeManualServer, showNetworkHostContextMenu } from '$lib/tauri-commands'
import { checkCredentialsForHost, forgetCredentials, getCredentialStatus } from './network-store.svelte'
import { forgetSavedServer, openServerRowMenu } from '../navigation/server-row-actions'
import { confirmDialog } from '$lib/utils/confirm-dialog'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import type { HubRow } from './servers-hub-rows'
import type { NetworkHost, VolumeInfo } from '../types'
import type { NetworkHostContextActionKind } from '$lib/ipc/bindings'

/** What the actions read from the component, live. */
export interface HubActionDeps {
  /** The rows on screen, for resolving a menu answer back to one. */
  getRows: () => HubRow[]
  /** The discovery store's hosts, for the SMB host menu's Disconnect. */
  getHosts: () => NetworkHost[]
  /** The volume list, which is where a live place's `VolumeInfo` lives. */
  getVolumes: () => VolumeInfo[]
  /** Re-read the saved list after a write the volume list won't announce. */
  refreshSaved: () => Promise<void>
}

/** The payload the native SMB-host menu answers with. */
/**
 * What the native SMB-host menu answered.
 *
 * ❗ `action` is the typed wire enum, ❌ never a free string: `forget-secret` is
 * spelled the same here as on a server ROW (`VolumeContextActionKind`), so one
 * act has one name on both sides of the app.
 */
export interface HostContextActionPayload {
  action: NetworkHostContextActionKind
  hostId: string
  hostName: string
}

export interface HubActions {
  /** F8, and the host menu's "Forget server". */
  forget: (row: HubRow) => Promise<void>
  /** Right-click on a row. */
  openMenu: (row: HubRow) => Promise<void>
  /** What the native SMB-host menu answered. */
  runHostAction: (payload: HostContextActionPayload) => Promise<void>
}

export function createHubActions(deps: HubActionDeps): HubActions {
  /**
   * F8. A saved server goes (after a confirmation); a host only mDNS knows about
   * has nothing to forget, and says so.
   */
  async function forget(row: HubRow): Promise<void> {
    if (!row.saved) {
      addToast(tString('fileExplorer.network.browser.cannotRemoveDiscovered'), { level: 'warn' })
      return
    }
    if (row.volumeId) {
      // A one-place server: the servers family owns the confirmation and the
      // toast, so the hub and the switcher's menu ask the same question.
      await forgetSavedServer(row.volumeId, row.name)
      return
    }
    await removeSavedSmbHost(row)
  }

  /** Forgets a saved SMB host, which is a manual-server entry rather than a place. */
  async function removeSavedSmbHost(row: HubRow): Promise<void> {
    const confirmed = await confirmDialog(
      tString('fileExplorer.network.browser.removeHostConfirm', { hostName: row.name }),
      tString('fileExplorer.network.browser.removeHostConfirmButton'),
    )
    if (!confirmed) return
    try {
      await removeManualServer(row.id)
      addToast(tString('fileExplorer.network.browser.hostRemoved', { hostName: row.name }), { level: 'success' })
      // The manual store is not the volume list, so nothing broadcasts this.
      await deps.refreshSaved()
    } catch {
      addToast(tString('fileExplorer.network.browser.hostRemoveFailed', { hostName: row.name }), { level: 'error' })
    }
  }

  /**
   * Right-click.
   *
   * A one-place row raises the SERVERS menu (Disconnect, Forget saved password,
   * Forget server), the same one the switcher row raises, so the two surfaces
   * can't drift. An SMB host keeps its own host menu.
   */
  async function openMenu(row: HubRow): Promise<void> {
    if (row.volumeId) {
      await openServerRowMenu(volumeForRow(row))
      return
    }
    const host = row.host
    if (!host) return
    // ❗ Asked here rather than on Rust's popup path, where "is a secret stored?"
    // would put a Keychain read in front of the menu appearing.
    if (getCredentialStatus(host.name) === 'unknown') {
      await checkCredentialsForHost(host.name)
    }
    await showNetworkHostContextMenu(
      host.id,
      host.name,
      host.source === 'manual',
      getCredentialStatus(host.name) === 'has_creds',
    )
  }

  /**
   * The row's `VolumeInfo`, for the menu builder.
   *
   * The volume list is the source when it has the row; a saved server that is
   * neither pinned nor connected has no row there, and the stand-in carries the
   * fields the menu actually reads.
   */
  function volumeForRow(row: HubRow): VolumeInfo {
    const known = deps.getVolumes().find((volume) => volume.id === row.volumeId)
    if (known) return known
    return {
      id: row.volumeId ?? row.id,
      name: row.name,
      path: row.saved?.places[0]?.appRoot ?? '',
      category: 'network',
      isEjectable: false,
      fsType: row.protocol,
      connectionState: null,
    }
  }

  /** Actions dispatched from the native SMB-host context menu. */
  async function runHostAction(payload: HostContextActionPayload): Promise<void> {
    switch (payload.action) {
      case 'forget-server': {
        const row = deps.getRows().find((r) => r.host?.id === payload.hostId || r.id === payload.hostId)
        if (row) await forget(row)
        return
      }
      case 'forget-secret':
        await forgetHostSecret(payload.hostName)
        return
      case 'disconnect':
        await disconnectHost(payload)
        return
    }
  }

  async function forgetHostSecret(hostName: string): Promise<void> {
    try {
      await forgetCredentials(hostName)
      addToast(tString('fileExplorer.network.forgotPassword', { hostName }), { level: 'success' })
    } catch {
      addToast(tString('fileExplorer.network.deletePasswordFailed'), { level: 'error' })
    }
  }

  /**
   * An SMB host's Disconnect UNMOUNTS its shares; it does not drop a session the
   * way a place's does. Zero unmounted is a normal answer, not a fault.
   */
  async function disconnectHost(payload: HostContextActionPayload): Promise<void> {
    const host = deps.getHosts().find((h) => h.id === payload.hostId)
    if (!host) return
    try {
      const unmounted = await disconnectNetworkHost(host.id, host.name, host.ipAddress)
      if (unmounted.length > 0) {
        addToast(tString('fileExplorer.network.browser.disconnected', { hostName: payload.hostName }), {
          level: 'success',
        })
      } else {
        addToast(tString('fileExplorer.network.browser.noMountedShares', { hostName: payload.hostName }))
      }
    } catch (e) {
      addToast(tString('fileExplorer.network.browser.disconnectFailed', { message: String(e) }), { level: 'error' })
    }
  }

  return { forget, openMenu, runHostAction }
}
