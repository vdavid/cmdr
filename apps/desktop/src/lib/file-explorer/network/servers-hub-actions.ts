/**
 * What a hub row's F8, right-click, and row-menu picks do, and what the SMB host menu answers.
 *
 * A factory rather than lines in `ServersHub.svelte`, so the component stays the
 * table, the cursor, and the keys. These are the parts that ask a question,
 * write to a store, and toast — the parts worth reading on their own.
 *
 * ❗ **A one-place row, an SMB host, and a saved share take different paths at
 * every branch**, and that is the whole reason this module exists as a unit. A
 * one-place row is a PLACE the servers family speaks for (`forgetSavedServer`,
 * `row-menu.ts`); an SMB host is a manual-server entry plus its share history,
 * whose "disconnect" unmounts shares rather than dropping a session; a saved
 * share is a row under its host, whose Forget drops the row and never the
 * mount. Mixing them is how a Forget removes the wrong thing.
 */

import {
  disconnectNetworkHost,
  forgetSavedSmbHost,
  forgetSavedSmbHostPassword,
  listSavedServers,
  forgetServer,
  forgetServerSecret,
  showNetworkHostContextMenu,
} from '$lib/tauri-commands'
import {
  checkCredentialsForHost,
  forgetCredentials,
  getCredentialStatus,
  setCredentialStatus,
} from './network-store.svelte'
import {
  forgetSavedServer,
  forgetServerQuestion,
  runServerRowAction,
  setServerAutoReconnect,
} from '../navigation/server-row-actions'
import {
  EMPTY_ROW_MENU,
  runRowFix,
  runVolumeRowAction,
  volumeRowMenu,
  type RowMenu,
  type RowMenuEntry,
  type RowToggleKind,
} from '../navigation/row-menu'
import { isVolumeBusy } from '$lib/stores/volume-busy-store.svelte'
import { confirmDialog, confirmWithCheckbox } from '$lib/utils/confirm-dialog'
import { openEditServerSheet } from '$lib/servers/open-sign-in'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { buildHubRows, type HubRow } from './servers-hub-rows'
import type { NetworkHost, VolumeInfo } from '../types'
import type { NetworkHostContextActionKind } from '$lib/ipc/bindings'
import type { MenuAnchor } from '$lib/tauri-commands/file-actions'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('servers')

/** What the actions read from the component, live. */
export interface HubActionDeps {
  /** The rows on screen, for resolving a menu answer back to one. */
  getRows: () => HubRow[]
  /** The volume list, which is where a live place's `VolumeInfo` lives. */
  getVolumes: () => VolumeInfo[]
  /** Re-read the saved list after a write the volume list won't announce. */
  refreshSaved: () => Promise<void>
  /** Do what Enter on the row does: a server's place or share list, a share's place. */
  openRow: (row: HubRow) => void
}

/**
 * What the native SMB-host menu answered.
 *
 * ❗ `action` is the typed wire enum, ❌ never a free string: `forget-secret` is
 * spelled the same here as on a server ROW (`VolumeContextActionKind`), so one
 * act has one name on both sides of the app.
 */
export interface HostContextActionPayload {
  action: NetworkHostContextActionKind
  /** The hub row the menu was raised on: what the answer acts on. */
  rowId: string
  hostId: string
  hostName: string
}

export interface HubActions {
  /** F8, and the host menu's "Forget server". */
  forget: (row: HubRow) => Promise<void>
  /** A one-place row's in-app menu, or `null` for an SMB host (which keeps its native one). */
  rowMenu: (row: HubRow) => RowMenu | null
  /** A pick from a one-place row's menu. */
  runRowEntry: (row: HubRow, entry: RowMenuEntry) => Promise<void>
  /** Right-click (or ⌃⏎, with an `anchor`) on an SMB host row: its native host menu. */
  openHostMenu: (row: HubRow, anchor?: MenuAnchor | null) => Promise<void>
  /** What the native SMB-host menu answered. */
  runHostAction: (payload: HostContextActionPayload) => Promise<void>
  /** The row with this id as the list holds it NOW, or `null` once it left. */
  rowById: (id: string) => HubRow | null
}

/**
 * What `ServersHub.svelte` calls on its `ServersHubRowMenu`. In a `.ts` because a type read
 * off a `.svelte` instance resolves to `any` under the plain-TypeScript lint service.
 */
export interface HubRowMenuAPI {
  /** Opens `row`'s menu: the in-app one at `point`, or an SMB host's native one at `anchor` (`null`: the pointer). */
  open: (row: HubRow, point: MenuAnchor, anchor: MenuAnchor | null) => Promise<void>
}

/** Edit's "nothing to edit here" hints share one toast `id`, so a key pressed again replaces it rather than stacking copies. */
const EDIT_HINT = { level: 'info', id: 'servers-edit-hint' } as const

/**
 * "Edit server…" on whatever hub row the cursor is on: what Edit and Rename (`file.edit`, `file.rename`, F4 and
 * F2 / ⇧F6 by default) and `servers.edit` do in the Servers list, since a server's name and settings are what it has
 * to rename or edit.
 *
 * A one-place server goes through the row menu's own Edit, which re-reads the saved list; an SMB host opens on the
 * entry its row carries, as its native menu's Edit does. ❗ A row with nothing saved to edit (a share, a host only
 * mDNS knows) says why, ❌ never does nothing: a key that silently did nothing reads as a broken key.
 */
export async function editHubRow(row: HubRow): Promise<void> {
  if (row.kind === 'place' && row.protocol === 'smb') {
    addToast(tString('servers.hub.editShareHint'), EDIT_HINT)
    return
  }
  if (!row.saved) {
    addToast(tString('servers.hub.editNearbyHint', { name: row.name }), EDIT_HINT)
    return
  }
  // A one-place server, or an S3 place, which edits on its own through its volume id.
  if (row.volumeId) {
    await runServerRowAction({ action: 'edit', volumeId: row.volumeId, volumeName: row.name })
    return
  }
  // An SMB host, or an S3 account: no place, so the sheet edits the server itself (its name, for S3 its secret too).
  await openEditServerSheet(row.saved)
}

/**
 * "Edit server…" for what the Servers volume shows (`file.edit`, `file.rename`, `servers.edit` there): the hub row
 * under the cursor, else the server whose share list is up, found among the saved ones by the same merge the hub
 * uses. ❗ Every case answers: "Add server…" under the cursor asks for a server, ❌ never nothing.
 */
export async function editServerInView(view: { row: HubRow | null; host: NetworkHost | null }): Promise<void> {
  if (view.row) {
    await editHubRow(view.row)
    return
  }
  const { host } = view
  if (!host) {
    addToast(tString('servers.hub.editPickHint'), EDIT_HINT)
    return
  }
  const saved = await listSavedServers().catch((e: unknown) => {
    log.warn('Reading the saved servers to edit {host} broke down: {error}', { host: host.name, error: String(e) })
    return []
  })
  // The host is the only one in the merge, so it is either claimed by its saved server or a nearby row of its own.
  const row = buildHubRows({ saved, hosts: [host], volumes: [] }).find((r) => r.host?.id === host.id)
  if (row) await editHubRow(row)
  else addToast(tString('servers.hub.editNearbyHint', { name: host.name }), EDIT_HINT)
}

export function createHubActions(deps: HubActionDeps): HubActions {
  /**
   * F8. A saved server goes (after a confirmation); a host only mDNS knows about
   * has nothing to forget, and says so.
   */
  async function forget(row: HubRow): Promise<void> {
    if (row.kind === 'place' && row.protocol === 'smb') {
      await forgetShare(row)
      return
    }
    if (!row.saved) {
      addToast(tString('fileExplorer.network.browser.cannotRemoveDiscovered'), { level: 'warn' })
      return
    }
    if (row.volumeId) {
      // A one-place server, or an S3 place: the servers family owns the confirmation
      // and the toast, so the hub and the switcher's menu ask the same question.
      await forgetSavedServer(row.volumeId, row.name)
      return
    }
    if (row.protocol === 's3') {
      await forgetS3Account(row)
      return
    }
    await removeSavedSmbHost(row)
  }

  /**
   * Forgets an S3 account: every place saved under it, and (the box, checked by
   * default) the secret access key they share. ❗ The secret FIRST, through one of
   * the places: once they're gone, nothing names the account's entry any more.
   */
  async function forgetS3Account(row: HubRow): Promise<void> {
    const places = row.saved?.places ?? []
    if (places.length === 0) return
    const { confirmed, checked } = await confirmWithCheckbox(
      forgetServerQuestion(
        tString('servers.hub.forgetAccountConfirm', { name: row.name, count: places.length }),
        tString('servers.hub.forgetSecretKeyToo'),
      ),
    )
    if (!confirmed) return
    if (checked) {
      try {
        await forgetServerSecret(places[0].volumeId)
      } catch (e) {
        log.warn('Forgetting the secret of the S3 account {id} broke down: {error}', { id: row.id, error: String(e) })
        addToast(tString('fileExplorer.navigation.forgetSecretRefusedToast', { name: row.name }), { level: 'error' })
      }
    }
    for (const place of places) {
      try {
        await forgetServer(place.volumeId)
      } catch (e) {
        log.warn('Forgetting the S3 place {id} broke down: {error}', { id: place.volumeId, error: String(e) })
        addToast(tString('fileExplorer.navigation.forgetServerRefusedToast', { name: place.name }), { level: 'error' })
      }
    }
  }

  /**
   * Forgets a saved SMB host: its manual entry, its sign-in history, and the
   * shares saved under it, and (the box, checked by default) its stored password.
   * ❗ Nothing is unmounted.
   */
  async function removeSavedSmbHost(row: HubRow): Promise<void> {
    const question = forgetServerQuestion(
      tString('fileExplorer.network.browser.removeHostConfirm', { hostName: row.name }),
    )
    // ❗ The password box only where one may be stored, known without a Keychain read.
    const { confirmed, checked } = mayHaveStoredPassword(row)
      ? await confirmWithCheckbox(question)
      : { confirmed: await confirmDialog(question.message, question.title, question.confirmLabel), checked: false }
    if (!confirmed) return
    // ❗ The password FIRST: the backend finds its names on the rows the Forget removes.
    if (checked) await forgetHostPassword(row)
    try {
      const forgotten = await forgetSavedSmbHost(row.id)
      if (!forgotten) throw new Error('nothing saved under that host')
      addToast(tString('fileExplorer.network.browser.hostRemoved', { hostName: row.name }), { level: 'success' })
      await deps.refreshSaved()
    } catch {
      addToast(tString('fileExplorer.network.browser.hostRemoveFailed', { hostName: row.name }), { level: 'error' })
    }
  }

  /**
   * Whether a password may be stored for this SMB host: it's used with an account (the
   * host's own, or a saved share's), or this session already read one (the in-memory
   * credential status). ❌ Never a Keychain read to find out.
   */
  function mayHaveStoredPassword(row: HubRow): boolean {
    const accounts = [row.saved?.username, ...(row.saved?.places ?? []).map((place) => place.username)]
    if (accounts.some((account) => account)) return true
    return row.host !== null && getCredentialStatus(row.host.name) === 'has_creds'
  }

  /** The host's stored password, all its names. A store that refuses still lets the host go, and says so. */
  async function forgetHostPassword(row: HubRow): Promise<void> {
    try {
      await forgetSavedSmbHostPassword(row.id)
      if (row.host) setCredentialStatus(row.host.name, 'no_creds')
    } catch (e) {
      log.warn('Forgetting the saved password of {id} broke down: {error}', { id: row.id, error: String(e) })
      addToast(tString('fileExplorer.navigation.forgetSecretRefusedToast', { name: row.name }), { level: 'error' })
    }
  }

  /**
   * Forgets a saved share, after asking: its row and its pin. ❗ A mounted share
   * stays mounted and no password is touched, which the question says. The row
   * leaves with the `volumes-changed` the command emits.
   */
  async function forgetShare(row: HubRow): Promise<void> {
    if (!row.volumeId) return
    const confirmed = await confirmDialog(
      tString('servers.hub.forgetShareConfirm', { name: row.name }),
      tString('servers.hub.forgetShareConfirmTitle'),
      tString('fileExplorer.navigation.forgetConfirmButton'),
    )
    if (!confirmed) return
    try {
      await forgetServer(row.volumeId)
    } catch {
      addToast(tString('fileExplorer.navigation.forgetServerRefusedToast', { name: row.name }), { level: 'error' })
    }
  }

  /**
   * A one-place row's right-click menu: the servers list from `row-menu.ts`, the
   * same one the switcher row's submenu shows, so the two surfaces can't drift.
   * Read live, so a transfer starting under the open menu greys its Disconnect.
   * `null` for an SMB host, which keeps its own native host menu
   * ([`openHostMenu`]).
   */
  function rowMenu(row: HubRow): RowMenu | null {
    // An S3 place is a server place like any one-place row, so it gets that menu below.
    if (row.kind === 'place' && row.protocol === 'smb') return shareMenu(row)
    if (!row.volumeId) return null
    const volume = volumeForRow(row)
    return volumeRowMenu(volume, {
      busy: isVolumeBusy(volume.id),
      ejecting: false,
      isSaved: row.saved !== null,
      directConnection: undefined,
      autoReconnect: placeAutoReconnect(row) ?? undefined,
    })
  }

  /**
   * The row's place's own "Reconnect automatically" switch: a place row's, else a
   * one-place server's only place. ❗ Per PLACE, ❌ never the server's: an S3 account's
   * buckets each keep their own.
   */
  function placeAutoReconnect(row: HubRow): boolean | null {
    const place = row.place ?? row.saved?.places.find((p) => p.volumeId === row.volumeId) ?? null
    return place?.autoReconnect ?? row.saved?.autoReconnect ?? null
  }

  /**
   * A saved share's menu: Open, the pin, and Forget share. ❗ No Disconnect: a
   * share's session is a mount, and its row in the switcher is where Eject is.
   */
  function shareMenu(row: HubRow): RowMenu {
    return {
      ...EMPTY_ROW_MENU,
      actions: [
        { type: 'action', action: 'open', label: tString('menu.network.open'), icon: 'arrow-right' },
        row.pinned
          ? {
              type: 'action',
              action: 'unpin',
              label: tString('menu.network.unpin'),
              icon: 'pin-off',
              keepsMenuOpen: true,
            }
          : {
              type: 'action',
              action: 'pin',
              label: tString('menu.network.pinToSwitcher'),
              icon: 'pin',
              keepsMenuOpen: true,
            },
        { type: 'action', action: 'forget-server', label: tString('servers.hub.forgetShare'), icon: 'trash-2' },
      ],
    }
  }

  /**
   * A pick from a one-place row's menu. Open takes the hub's own Enter path, so
   * it moves THIS pane; everything else runs where the switcher's runs.
   */
  async function runRowEntry(row: HubRow, entry: RowMenuEntry): Promise<void> {
    if (entry.type === 'toggle') {
      await flipToggle[entry.toggle](row)
      return
    }
    if (entry.type === 'fix') {
      await runRowFix({ volume: volumeForRow(row), fix: entry.fix })
      return
    }
    if (entry.action === 'open') {
      deps.openRow(row)
      return
    }
    if (row.kind === 'place' && row.protocol === 'smb' && entry.action === 'forget-server') {
      await forgetShare(row)
      return
    }
    await runVolumeRowAction({ volume: volumeForRow(row), action: entry.action })
  }

  /** What flipping each row switch does. A `Record`, so a new `RowToggleKind` won't compile until it's handled. */
  const flipToggle: Record<RowToggleKind, (row: HubRow) => Promise<void>> = {
    // An SMB share's switch: never on a one-place (SFTP or WebDAV) row.
    'direct-connection': () => Promise.resolve(),
    // The row shows the saved entry's switch; the `volumes-changed` the command emits is what
    // re-reads the saved list, so the next open shows the new state.
    'auto-reconnect': async (row) => {
      if (row.volumeId && row.saved) await setServerAutoReconnect(row.volumeId, !placeAutoReconnect(row))
    },
  }

  /**
   * An SMB host's right-click: its native host menu, raised for THIS row (its id
   * goes out with the menu and comes back with the answer). `anchor` places a
   * keyboard-opened one.
   */
  async function openHostMenu(row: HubRow, anchor: MenuAnchor | null = null): Promise<void> {
    const host = row.host
    if (!host) return
    // ❗ Asked here rather than on Rust's popup path, where "is a secret stored?"
    // would put a Keychain read in front of the menu appearing.
    if (getCredentialStatus(host.name) === 'unknown') {
      await checkCredentialsForHost(host.name)
    }
    await showNetworkHostContextMenu(
      row.id,
      host,
      host.source === 'manual',
      // A SAVED host has a name to edit; one mDNS merely sees does not.
      row.saved !== null,
      getCredentialStatus(host.name) === 'has_creds',
      anchor,
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
      path: row.place?.appRoot ?? row.saved?.places[0]?.appRoot ?? '',
      category: 'network',
      isEjectable: false,
      fsType: row.protocol,
      connectionState: null,
      pinned: row.pinned,
    }
  }

  /**
   * Actions dispatched from the native SMB-host context menu.
   *
   * ❗ **Every answer acts on the row the menu was raised on**, found by its
   * exact id (`payload.rowId`), ❌ never by the host, a name, or an address. Two
   * rows can stand on one discovered host, and a lookup by host picks whichever
   * comes first: "Edit server…" on the second row once saved into the first. A
   * row that left the list while the menu was up gets nothing.
   */
  async function runHostAction(payload: HostContextActionPayload): Promise<void> {
    const row = deps.getRows().find((r) => r.id === payload.rowId)
    if (!row) {
      log.info('The host menu answered for a row the list no longer has; nothing to do')
      return
    }
    switch (payload.action) {
      case 'forget-server':
        await forget(row)
        return
      case 'forget-secret':
        if (row.host) await forgetHostSecret(row.host.name, row.name)
        return
      case 'edit':
        if (row.saved) await openEditServerSheet(row.saved)
        return
      case 'disconnect':
        if (row.host) await disconnectHost(row.host, row.name)
        return
    }
  }

  /** `hostName` keys the Keychain; `label` is what the row calls the host, for the toast. */
  async function forgetHostSecret(hostName: string, label: string): Promise<void> {
    try {
      await forgetCredentials(hostName)
      addToast(tString('fileExplorer.network.forgotPassword', { hostName: label }), { level: 'success' })
    } catch {
      addToast(tString('fileExplorer.network.deletePasswordFailed'), { level: 'error' })
    }
  }

  /**
   * An SMB host's Disconnect UNMOUNTS its shares; it does not drop a session the
   * way a place's does. Zero unmounted is a normal answer, not a fault.
   */
  async function disconnectHost(host: NetworkHost, label: string): Promise<void> {
    try {
      const unmounted = await disconnectNetworkHost(host)
      if (unmounted.length > 0) {
        addToast(tString('fileExplorer.network.browser.disconnected', { hostName: label }), {
          level: 'success',
        })
      } else {
        addToast(tString('fileExplorer.network.browser.noMountedShares', { hostName: label }))
      }
    } catch (e) {
      addToast(tString('fileExplorer.network.browser.disconnectFailed', { message: String(e) }), { level: 'error' })
    }
  }

  function rowById(id: string): HubRow | null {
    return deps.getRows().find((row) => row.id === id) ?? null
  }

  return { forget, rowMenu, runRowEntry, openHostMenu, runHostAction, rowById }
}
