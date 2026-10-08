/**
 * What F8, a right-click, and a row-menu pick do to each kind of hub row.
 *
 * The whole risk here is that a one-place row and an SMB host take different
 * paths at every branch, and the wrong branch removes the wrong thing: a Forget
 * on a place drops a saved server plus its pins, a Forget on an SMB host drops a
 * manual-server entry, and a Disconnect on a host unmounts shares rather than
 * dropping a session.
 */

import { describe, it, expect, vi, beforeEach, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { NetworkHost, VolumeInfo } from '../types'

const forgetSavedSmbHost = vi.fn((_id: string) => Promise.resolve(true))
const forgetServer = vi.fn((_volumeId: string) => Promise.resolve(true))
const forgetServerSecret = vi.fn((_volumeId: string) => Promise.resolve(true))
const disconnectNetworkHost = vi.fn(() => Promise.resolve(['/Volumes/Public']))
const showNetworkHostContextMenu = vi.fn(() => Promise.resolve())
const forgetSavedServer = vi.fn(() => Promise.resolve())
const setServerAutoReconnect = vi.fn((_volumeId: string, _on: boolean) => Promise.resolve())
const runVolumeRowAction = vi.fn((_payload: unknown) => Promise.resolve())
const openRow = vi.fn()
const forgetCredentials = vi.fn(() => Promise.resolve())
const addToast = vi.fn()
const confirmDialog = vi.fn(() => Promise.resolve(true))
const confirmWithCheckbox = vi.fn((_question: unknown) => Promise.resolve({ confirmed: true, checked: true }))
const forgetSavedSmbHostPassword = vi.fn((_id: string) => Promise.resolve(true))
const listSavedServers = vi.fn(() => Promise.resolve<unknown[]>([]))
const setCredentialStatus = vi.fn()
/** What the store's in-memory credential status says about the host. */
let credentialStatus = 'has_creds'
const openEditServerSheet = vi.fn((_server: unknown) => Promise.resolve({ kind: 'saved' }))
const runServerRowAction = vi.fn((_payload: unknown) => Promise.resolve())

vi.mock('$lib/tauri-commands', () => ({
  forgetSavedSmbHost: (id: string) => forgetSavedSmbHost(id),
  forgetSavedSmbHostPassword: (id: string) => forgetSavedSmbHostPassword(id),
  listSavedServers: () => listSavedServers(),
  forgetServer: (volumeId: string) => forgetServer(volumeId),
  forgetServerSecret: (volumeId: string) => forgetServerSecret(volumeId),
  disconnectNetworkHost: (...args: unknown[]) => disconnectNetworkHost(...(args as [])),
  showNetworkHostContextMenu: (...args: unknown[]) => showNetworkHostContextMenu(...(args as [])),
}))
vi.mock('./network-store.svelte', () => ({
  getCredentialStatus: () => credentialStatus,
  checkCredentialsForHost: vi.fn(() => Promise.resolve()),
  forgetCredentials: (...args: unknown[]) => forgetCredentials(...(args as [])),
  setCredentialStatus: (...args: unknown[]) => {
    setCredentialStatus(...(args as []))
  },
}))
vi.mock('../navigation/server-row-actions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../navigation/server-row-actions')>()),
  forgetSavedServer: (...args: unknown[]) => forgetSavedServer(...(args as [])),
  setServerAutoReconnect: (volumeId: string, on: boolean) => setServerAutoReconnect(volumeId, on),
  runServerRowAction: (payload: unknown) => runServerRowAction(payload),
}))
vi.mock('$lib/stores/volume-busy-store.svelte', () => ({ isVolumeBusy: () => false, isVolumeEjecting: () => false }))
vi.mock('../navigation/row-menu', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../navigation/row-menu')>()),
  runVolumeRowAction: (payload: unknown) => runVolumeRowAction(payload),
}))
vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    addToast(...(args as []))
  },
}))
vi.mock('$lib/servers/open-sign-in', () => ({
  openEditServerSheet: (server: unknown) => openEditServerSheet(server),
}))
vi.mock('$lib/utils/confirm-dialog', () => ({
  confirmDialog: (...args: unknown[]) => confirmDialog(...(args as [])),
  confirmWithCheckbox: (question: unknown) => confirmWithCheckbox(question),
}))

import { createHubActions, editHubRow, editServerInView } from './servers-hub-actions'
import type { HubRow } from './servers-hub-rows'
import type { NetworkHostContextActionKind, VolumeContextActionKind } from '$lib/ipc/bindings'

const host: NetworkHost = { id: 'h1', name: 'Attic NAS', ipAddress: '10.0.0.4', port: 445, source: 'manual' }

/** A one-place server: the servers family speaks for it. */
const placeRow: HubRow = {
  id: 'sftp-nas.local-22-ada',
  kind: 'server',
  parentId: null,
  account: null,
  place: null,
  name: 'Naspolya',
  protocol: 'sftp',
  address: 'nas.local:22',
  status: 'saved',
  lastConnectedAt: null,
  volumeId: 'sftp-nas.local-22-ada',
  pinned: true,
  saved: {
    id: 'sftp-nas.local-22-ada',
    protocol: 'sftp',
    displayName: 'Naspolya',
    nameSource: 'user',
    address: 'nas.local:22',
    username: 'ada',
    pinned: true,
    lastConnectedAt: null,
    autoReconnect: true,
    places: [
      {
        volumeId: 'sftp-nas.local-22-ada',
        name: 'Naspolya',
        pinned: true,
        connected: false,
        appRoot: 'sftp://ada@nas.local:22',
        username: 'ada',
        autoReconnect: true,
      },
    ],
  },
  host: null,
}

/** A saved SMB host: a manual-server entry, with no place to act on. */
const savedHostRow: HubRow = {
  id: 'manual-10-0-0-4-445',
  kind: 'server',
  parentId: null,
  account: null,
  place: null,
  name: 'Attic NAS',
  protocol: 'smb',
  address: '10.0.0.4',
  status: 'found_nearby',
  lastConnectedAt: null,
  volumeId: null,
  pinned: false,
  saved: {
    id: 'manual-10-0-0-4-445',
    protocol: 'smb',
    displayName: 'Attic NAS',
    nameSource: 'user',
    address: '10.0.0.4',
    username: null,
    pinned: false,
    lastConnectedAt: null,
    autoReconnect: null,
    places: [],
  },
  host,
}

/** A saved share under the saved host: a row of its own (`docs/specs/saved-smb-shares.md`). */
const shareRow: HubRow = {
  ...savedHostRow,
  id: 'share:smb-container',
  kind: 'place',
  parentId: savedHostRow.id,
  account: { kind: 'user', username: 'sven' },
  name: 'Container',
  status: 'saved',
  volumeId: 'smb-container',
  pinned: true,
  place: {
    volumeId: 'smb-container',
    name: 'Container',
    pinned: true,
    connected: false,
    appRoot: '/Volumes/Container',
    username: 'sven',
    autoReconnect: null,
  },
}

/** A host only mDNS knows about: nothing of the user's to remove. */
const nearbyOnlyRow: HubRow = {
  ...savedHostRow,
  id: 'h2',
  saved: null,
  host: { ...host, id: 'h2', source: 'discovered' },
}

const liveVolume: VolumeInfo = {
  id: 'sftp-nas.local-22-ada',
  name: 'Naspolya',
  path: 'sftp://ada@nas.local:22',
  category: 'network',
  fsType: 'sftp',
  isEjectable: false,
  connectionState: 'direct',
}

const refreshSaved = vi.fn(() => Promise.resolve())

function actions(volumes: VolumeInfo[] = []) {
  return createHubActions({
    getRows: () => [placeRow, savedHostRow, nearbyOnlyRow],
    getVolumes: () => volumes,
    refreshSaved,
    openRow,
  })
}

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})
beforeEach(() => {
  vi.clearAllMocks()
  confirmDialog.mockResolvedValue(true)
  confirmWithCheckbox.mockResolvedValue({ confirmed: true, checked: true })
  credentialStatus = 'has_creds'
})

describe('forget', () => {
  it('sends a one-place server to the servers family, so the hub asks what the switcher asks', async () => {
    await actions().forget(placeRow)
    expect(forgetSavedServer).toHaveBeenCalledWith('sftp-nas.local-22-ada', 'Naspolya')
    expect(forgetSavedSmbHost).not.toHaveBeenCalled()
  })

  it('removes a saved SMB host from the manual store instead, after asking', async () => {
    await actions().forget(savedHostRow)
    expect(confirmWithCheckbox).toHaveBeenCalledOnce()
    expect(forgetSavedSmbHost).toHaveBeenCalledWith('manual-10-0-0-4-445')
    expect(forgetSavedServer).not.toHaveBeenCalled()
  })

  /**
   * ❗ One verb: the menu says "Forget server", so the alert's title, its button, and
   * the toast say Forget too. The title slot used to carry the button's word
   * ("Remove") and the button read "OK" (QA round 2).
   */
  it('asks with a Forget title and a Forget button, and says Forgot after', async () => {
    await actions().forget(savedHostRow)
    expect(confirmWithCheckbox).toHaveBeenCalledWith({
      message: 'Forget Attic NAS? Cmdr stops listing it and the shares saved under it. Nothing gets unmounted.',
      title: 'Forget server',
      confirmLabel: 'Forget',
      checkboxLabel: 'Also forget the saved password',
      checked: true,
    })
    expect(addToast).toHaveBeenCalledWith('Forgot Attic NAS', { level: 'success' })
  })

  /**
   * ❗ The box is checked by default, and a checked box takes the host's stored
   * password FIRST: the backend finds its names on the rows the Forget then removes.
   */
  it('forgets the host’s saved password first when the box stays checked', async () => {
    const order: string[] = []
    forgetSavedSmbHostPassword.mockImplementationOnce(() => {
      order.push('password')
      return Promise.resolve(true)
    })
    forgetSavedSmbHost.mockImplementationOnce(() => {
      order.push('host')
      return Promise.resolve(true)
    })
    await actions().forget(savedHostRow)
    expect(order).toEqual(['password', 'host'])
    expect(forgetSavedSmbHostPassword).toHaveBeenCalledWith('manual-10-0-0-4-445')
    expect(setCredentialStatus).toHaveBeenCalledWith('Attic NAS', 'no_creds')
  })

  /**
   * ❗ The box shows only where a password may be stored: an account the host is used
   * with, or one this session already read. A guest-only host offered to forget a
   * password it never had (final QA). Known without a Keychain read.
   */
  it('asks plainly, with no password box, for a host with no account and nothing read', async () => {
    credentialStatus = 'unknown'
    await actions().forget(savedHostRow)
    expect(confirmWithCheckbox).not.toHaveBeenCalled()
    expect(confirmDialog).toHaveBeenCalledOnce()
    expect(forgetSavedSmbHostPassword).not.toHaveBeenCalled()
    expect(forgetSavedSmbHost).toHaveBeenCalledOnce()
  })

  it('offers the box for a host used with an account, even with nothing read', async () => {
    credentialStatus = 'unknown'
    const withAccount = { ...savedHostRow, saved: savedHostRow.saved && { ...savedHostRow.saved, username: 'ada' } }
    await actions().forget(withAccount)
    expect(confirmWithCheckbox).toHaveBeenCalledOnce()
  })

  it('keeps the host’s saved password when the box was unchecked', async () => {
    confirmWithCheckbox.mockResolvedValueOnce({ confirmed: true, checked: false })
    await actions().forget(savedHostRow)
    expect(forgetSavedSmbHostPassword).not.toHaveBeenCalled()
    expect(forgetSavedSmbHost).toHaveBeenCalledOnce()
  })

  it('still forgets the host when its password won’t go, and says which half didn’t', async () => {
    forgetSavedSmbHostPassword.mockRejectedValueOnce(new Error('keychain locked'))
    await actions().forget(savedHostRow)
    expect(forgetSavedSmbHost).toHaveBeenCalledOnce()
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining('saved password'), { level: 'error' })
  })

  it('asks a share’s Forget with a Forget button too', async () => {
    await actions().forget(shareRow)
    expect(confirmDialog).toHaveBeenCalledWith(expect.any(String), 'Forget share', 'Forget')
  })

  it('re-reads the saved list after removing a host, which nothing broadcasts', async () => {
    await actions().forget(savedHostRow)
    expect(refreshSaved).toHaveBeenCalledOnce()
  })

  it('keeps a host the user said no to', async () => {
    confirmWithCheckbox.mockResolvedValue({ confirmed: false, checked: false })
    await actions().forget(savedHostRow)
    expect(forgetSavedSmbHost).not.toHaveBeenCalled()
  })

  it('says a discovered host is not the user’s to remove, rather than doing nothing', async () => {
    await actions().forget(nearbyOnlyRow)
    expect(forgetSavedSmbHost).not.toHaveBeenCalled()
    expect(forgetSavedServer).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledWith('Can’t remove discovered hosts', { level: 'warn' })
  })
})

describe('rowMenu', () => {
  /** The menu's entries as their action names, so an assertion reads like the menu. */
  function actionsOf(row: HubRow, volumes: VolumeInfo[] = []): string[] {
    const menu = actions(volumes).rowMenu(row)
    if (!menu) return []
    return [
      ...menu.actions.map((e) => e.action),
      ...menu.fixes.map((e) => e.fix),
      ...menu.settings.map((e) => e.toggle),
    ]
  }

  it('gives a one-place row the servers list, the same one the switcher row’s submenu shows', () => {
    // Live and saved: every server action. Pinned, so the pin reads Unpin. Saved, so
    // "Reconnect automatically" rides below, from the saved entry.
    expect(actionsOf(placeRow, [{ ...liveVolume, pinned: true }])).toEqual([
      'open',
      'edit',
      'disconnect',
      'unpin',
      'forget-secret',
      'forget-server',
      'auto-reconnect',
    ])
    const toggle = actions([liveVolume]).rowMenu(placeRow)?.settings[0]
    expect(toggle).toMatchObject({ toggle: 'auto-reconnect', checked: true })
  })

  it('stands in for a place the volume list has no row for, rather than skipping the menu', () => {
    // A saved server that is neither pinned nor connected isn't in the listing: no session, so no Disconnect.
    expect(actionsOf(placeRow, [])).toEqual([
      'open',
      'edit',
      'unpin',
      'forget-secret',
      'forget-server',
      'auto-reconnect',
    ])
  })

  it('has no in-app menu for an SMB host: that one keeps its own native host menu', () => {
    expect(actions().rowMenu(savedHostRow)).toBeNull()
  })
})

describe('runRowEntry', () => {
  const entry = (action: VolumeContextActionKind) =>
    ({ type: 'action', action, label: action, icon: 'pencil' }) as const

  it('opens the place through the hub’s own Enter path', async () => {
    await actions([liveVolume]).runRowEntry(placeRow, entry('open'))
    expect(openRow).toHaveBeenCalledWith(placeRow)
    expect(runVolumeRowAction).not.toHaveBeenCalled()
  })

  it('hands every other action to the one runner the switcher uses', async () => {
    await actions([liveVolume]).runRowEntry(placeRow, entry('disconnect'))
    expect(runVolumeRowAction).toHaveBeenCalledWith({ volume: liveVolume, action: 'disconnect' })
  })

  it('flips "Reconnect automatically" from the saved entry’s value, through the servers family', async () => {
    const toggle = {
      type: 'toggle',
      toggle: 'auto-reconnect',
      label: 'Reconnect automatically',
      checked: true,
    } as const
    await actions([liveVolume]).runRowEntry(placeRow, toggle)
    expect(setServerAutoReconnect).toHaveBeenCalledWith('sftp-nas.local-22-ada', false)
  })
})

describe('openHostMenu', () => {
  it('raises the SMB host menu for a host row, naming the row it was raised on', async () => {
    await actions().openHostMenu(savedHostRow)
    expect(showNetworkHostContextMenu).toHaveBeenCalledWith('manual-10-0-0-4-445', host, true, true, true, null)
  })

  it('offers no Edit for a host only mDNS knows about, which has nowhere to keep a name', async () => {
    await actions().openHostMenu(nearbyOnlyRow)
    expect(showNetworkHostContextMenu).toHaveBeenCalledWith('h2', nearbyOnlyRow.host, false, false, true, null)
  })

  it('opens where the keyboard says when there is no pointer to use', async () => {
    await actions().openHostMenu(savedHostRow, { x: 40, y: 120 })
    expect(showNetworkHostContextMenu).toHaveBeenCalledWith('manual-10-0-0-4-445', host, true, true, true, {
      x: 40,
      y: 120,
    })
  })
})

describe('runHostAction', () => {
  // ❗ Typed, so a spelling that drifts from the Rust enum is a compile error
  // rather than a silently-dead menu item.
  const payload = (action: NetworkHostContextActionKind, rowId = savedHostRow.id) => ({
    action,
    rowId,
    hostId: 'h1',
    hostName: 'Attic NAS',
  })

  it('routes the host menu’s Forget back through the same branch F8 takes', async () => {
    await actions().runHostAction(payload('forget-server'))
    expect(forgetSavedSmbHost).toHaveBeenCalledWith('manual-10-0-0-4-445')
  })

  it('opens the edit sheet on the saved host the menu was raised for', async () => {
    await actions().runHostAction(payload('edit'))
    expect(openEditServerSheet).toHaveBeenCalledWith(savedHostRow.saved)
  })

  it('forgets the host’s stored password without touching the host itself', async () => {
    await actions().runHostAction(payload('forget-secret'))
    expect(forgetCredentials).toHaveBeenCalledWith('Attic NAS')
    expect(forgetSavedSmbHost).not.toHaveBeenCalled()
  })

  it('unmounts an SMB host’s shares, which is what its Disconnect means', async () => {
    await actions().runHostAction(payload('disconnect'))
    // ❗ The whole host: its mounts are found by where it dials, every name on ITS port.
    expect(disconnectNetworkHost).toHaveBeenCalledWith(host)
    expect(addToast).toHaveBeenCalledWith('Disconnected from Attic NAS', { level: 'success' })
  })

  it('treats nothing-to-unmount as a normal answer, not a fault', async () => {
    disconnectNetworkHost.mockResolvedValueOnce([])
    await actions().runHostAction(payload('disconnect'))
    expect(addToast).toHaveBeenCalledWith('No mounted shares from Attic NAS')
  })

  it('does nothing for a row that is gone by the time the menu answers', async () => {
    for (const action of ['disconnect', 'edit', 'forget-server', 'forget-secret'] as const) {
      await actions().runHostAction(payload(action, 'gone'))
    }
    expect(disconnectNetworkHost).not.toHaveBeenCalled()
    expect(openEditServerSheet).not.toHaveBeenCalled()
    expect(forgetSavedSmbHost).not.toHaveBeenCalled()
    expect(forgetCredentials).not.toHaveBeenCalled()
  })

  /**
   * ❗ **Two rows can stand on one discovered host**: a saved host and a second
   * saved row for the same machine (QA 2026-09-25: "Edit server…" on the SECOND
   * row saved its name into the FIRST). The answer names the row, so it lands on
   * the row it was raised on, whichever row comes first in the list.
   */
  describe('when two rows share one host', () => {
    const twinRow: HubRow = {
      ...savedHostRow,
      id: 'manual-localhost-11482-445',
      name: 'localhost:11482',
      saved: { ...(savedHostRow.saved as NonNullable<HubRow['saved']>), id: 'manual-localhost-11482-445' },
    }
    const withTwin = () =>
      createHubActions({
        getRows: () => [savedHostRow, twinRow],
        getVolumes: () => [],
        refreshSaved,
        openRow,
      })

    it('edits the row the menu was raised on', async () => {
      await withTwin().runHostAction(payload('edit', twinRow.id))
      expect(openEditServerSheet).toHaveBeenCalledWith(twinRow.saved)
    })

    it('names the host in its toasts the way its row does', async () => {
      await withTwin().runHostAction(payload('disconnect', twinRow.id))
      expect(addToast).toHaveBeenCalledWith('Disconnected from localhost:11482', { level: 'success' })
    })

    it('forgets the row the menu was raised on, and never its twin', async () => {
      await withTwin().runHostAction(payload('forget-server', twinRow.id))
      expect(forgetSavedSmbHost).toHaveBeenCalledExactlyOnceWith(twinRow.id)
    })
  })
})

describe('a saved share', () => {
  it('forgets only the share, after asking, and never the host', async () => {
    await actions().forget(shareRow)
    expect(confirmDialog).toHaveBeenCalledOnce()
    expect(forgetServer).toHaveBeenCalledWith('smb-container')
    expect(forgetSavedSmbHost).not.toHaveBeenCalled()
  })

  it('keeps a share the user said no to', async () => {
    confirmDialog.mockResolvedValueOnce(false)
    await actions().forget(shareRow)
    expect(forgetServer).not.toHaveBeenCalled()
  })

  it('offers Open, the pin, and Forget share, and no Disconnect: a share’s session is its mount', () => {
    const menu = actions().rowMenu(shareRow)
    expect(menu?.actions.map((entry) => entry.action)).toEqual(['open', 'unpin', 'forget-server'])
    expect(menu?.actions[2].label).toBe('Forget share')
  })

  it('opens a share the way Enter does, and forgets it through its own question', async () => {
    const menu = actions().rowMenu(shareRow)
    if (!menu) throw new Error('a share has a menu')
    await actions().runRowEntry(shareRow, menu.actions[0])
    expect(openRow).toHaveBeenCalledWith(shareRow)

    await actions().runRowEntry(shareRow, menu.actions[2])
    expect(forgetServer).toHaveBeenCalledWith('smb-container')
    expect(forgetSavedServer).not.toHaveBeenCalled()
  })
})

/**
 * Edit and Rename (F4, F2, ⇧F6 by default, whatever they're bound to) on a hub row are
 * "Edit server…". ❗ A row with nothing to edit says why, ❌ never does nothing.
 */
describe('editHubRow', () => {
  it('opens Edit server on a saved SMB host', async () => {
    await editHubRow(savedHostRow)
    expect(openEditServerSheet).toHaveBeenCalledExactlyOnceWith(savedHostRow.saved)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('opens Edit server on a one-place server through the menu’s own Edit', async () => {
    await editHubRow(placeRow)
    expect(runServerRowAction).toHaveBeenCalledExactlyOnceWith({
      action: 'edit',
      volumeId: 'sftp-nas.local-22-ada',
      volumeName: 'Naspolya',
    })
  })

  /** ❗ With an `id`, so F4 held down or pressed again replaces the hint instead of stacking copies. */
  it('says why a share row has nothing to edit', async () => {
    await editHubRow(shareRow)
    expect(openEditServerSheet).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledExactlyOnceWith(expect.stringContaining('server'), {
      level: 'info',
      id: 'servers-edit-hint',
    })
  })

  it('says why a host only mDNS knows has nothing to edit', async () => {
    await editHubRow(nearbyOnlyRow)
    expect(openEditServerSheet).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledExactlyOnceWith(expect.stringContaining('Attic NAS'), {
      level: 'info',
      id: 'servers-edit-hint',
    })
  })
})

/**
 * What's in view on the Servers volume: a hub row, or the host whose share list is up.
 * ❗ Every case answers something: a sheet, or a toast saying why not.
 */
describe('editServerInView', () => {
  it('edits the hub row under the cursor', async () => {
    await editServerInView({ row: savedHostRow, host: null })
    expect(openEditServerSheet).toHaveBeenCalledExactlyOnceWith(savedHostRow.saved)
  })

  it('edits the saved server whose share list is up, found by its host', async () => {
    if (!savedHostRow.saved) throw new Error('fixture')
    listSavedServers.mockResolvedValueOnce([savedHostRow.saved])
    await editServerInView({ row: null, host })
    expect(openEditServerSheet).toHaveBeenCalledExactlyOnceWith(savedHostRow.saved)
  })

  it('says a nearby host whose share list is up has nothing saved to edit', async () => {
    await editServerInView({ row: null, host: { ...host, id: 'h9', name: 'Printer', ipAddress: '10.0.0.9' } })
    expect(openEditServerSheet).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledExactlyOnceWith(expect.stringContaining('Printer'), {
      level: 'info',
      id: 'servers-edit-hint',
    })
  })

  it('asks for a server when the cursor is on Add server…', async () => {
    await editServerInView({ row: null, host: null })
    expect(addToast).toHaveBeenCalledExactlyOnceWith('Select a server to edit it.', {
      level: 'info',
      id: 'servers-edit-hint',
    })
  })
})

describe('an S3 account and its places', () => {
  const s3Place = (bucket: string) => ({
    volumeId: `s3-host-443-akia-${bucket}`,
    name: bucket,
    pinned: true,
    connected: false,
    appRoot: `s3://AKIA@host:443/${bucket}`,
    username: 'AKIA',
    autoReconnect: true,
  })
  const account: HubRow = {
    ...placeRow,
    id: 's3-host-443-akia-root',
    name: 'AKIA@host',
    protocol: 's3',
    volumeId: null,
    pinned: false,
    saved: {
      id: 's3-host-443-akia-root',
      protocol: 's3',
      displayName: 'AKIA@host',
      nameSource: 'fallback',
      address: 'host',
      username: 'AKIA',
      pinned: false,
      lastConnectedAt: null,
      autoReconnect: true,
      places: [s3Place('photos'), s3Place('scans')],
    },
  }
  const bucketRow: HubRow = {
    ...account,
    id: 'share:s3-host-443-akia-photos',
    kind: 'place',
    parentId: account.id,
    name: 'photos',
    volumeId: 's3-host-443-akia-photos',
    pinned: true,
    place: s3Place('photos'),
  }

  it('forgets a bucket on its own, through the servers family, like a one-place row', async () => {
    await actions().forget(bucketRow)
    expect(forgetSavedServer).toHaveBeenCalledExactlyOnceWith('s3-host-443-akia-photos', 'photos')
    expect(forgetServer).not.toHaveBeenCalled()
  })

  it('forgets the whole account: the shared secret FIRST, then every place under it', async () => {
    await actions().forget(account)
    expect(forgetServerSecret).toHaveBeenCalledExactlyOnceWith('s3-host-443-akia-photos')
    expect(forgetServer.mock.calls.map(([id]) => id)).toEqual(['s3-host-443-akia-photos', 's3-host-443-akia-scans'])
    expect(forgetServerSecret.mock.invocationCallOrder[0]).toBeLessThan(forgetServer.mock.invocationCallOrder[0])
  })

  it('keeps the secret when the box is cleared, and forgets nothing when the question is declined', async () => {
    confirmWithCheckbox.mockResolvedValueOnce({ confirmed: true, checked: false })
    await actions().forget(account)
    expect(forgetServerSecret).not.toHaveBeenCalled()
    expect(forgetServer).toHaveBeenCalledTimes(2)

    vi.clearAllMocks()
    confirmWithCheckbox.mockResolvedValueOnce({ confirmed: false, checked: true })
    await actions().forget(account)
    expect(forgetServer).not.toHaveBeenCalled()
  })

  it('edits a bucket through its own volume id, and the account row as the account', async () => {
    await editHubRow(bucketRow)
    expect(runServerRowAction).toHaveBeenCalledExactlyOnceWith({
      action: 'edit',
      volumeId: 's3-host-443-akia-photos',
      volumeName: 'photos',
    })
    await editHubRow(account)
    expect(openEditServerSheet).toHaveBeenCalledExactlyOnceWith(account.saved)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('shows and flips the bucket’s OWN "Reconnect automatically", ❌ not the account’s', async () => {
    const quietBucket: HubRow = { ...bucketRow, place: { ...s3Place('photos'), autoReconnect: false } }
    expect(actions().rowMenu(quietBucket)?.settings[0]).toMatchObject({ toggle: 'auto-reconnect', checked: false })
    await actions().runRowEntry(quietBucket, {
      type: 'toggle',
      toggle: 'auto-reconnect',
      label: 'Reconnect automatically',
      checked: false,
    })
    expect(setServerAutoReconnect).toHaveBeenCalledWith('s3-host-443-akia-photos', true)
  })

  it('gives a bucket the server-place menu, ❌ not a share’s', () => {
    const menu = actions().rowMenu(bucketRow)
    const picks = menu?.actions.map((entry) => entry.action) ?? []
    expect(picks).toContain('edit')
    expect(menu?.actions.some((entry) => entry.label === 'Forget share')).toBe(false)
  })
})
