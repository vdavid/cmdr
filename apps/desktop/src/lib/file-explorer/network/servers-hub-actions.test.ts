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

const removeManualServer = vi.fn(() => Promise.resolve())
const disconnectNetworkHost = vi.fn(() => Promise.resolve(['/Volumes/Public']))
const showNetworkHostContextMenu = vi.fn(() => Promise.resolve())
const forgetSavedServer = vi.fn(() => Promise.resolve())
const setServerAutoReconnect = vi.fn((_volumeId: string, _on: boolean) => Promise.resolve())
const runVolumeRowAction = vi.fn((_payload: unknown) => Promise.resolve())
const openServer = vi.fn()
const forgetCredentials = vi.fn(() => Promise.resolve())
const addToast = vi.fn()
const confirmDialog = vi.fn(() => Promise.resolve(true))

vi.mock('$lib/tauri-commands', () => ({
  removeManualServer: (...args: unknown[]) => removeManualServer(...(args as [])),
  disconnectNetworkHost: (...args: unknown[]) => disconnectNetworkHost(...(args as [])),
  showNetworkHostContextMenu: (...args: unknown[]) => showNetworkHostContextMenu(...(args as [])),
}))
vi.mock('./network-store.svelte', () => ({
  getCredentialStatus: () => 'has_creds',
  checkCredentialsForHost: vi.fn(() => Promise.resolve()),
  forgetCredentials: (...args: unknown[]) => forgetCredentials(...(args as [])),
}))
vi.mock('../navigation/server-row-actions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../navigation/server-row-actions')>()),
  forgetSavedServer: (...args: unknown[]) => forgetSavedServer(...(args as [])),
  setServerAutoReconnect: (volumeId: string, on: boolean) => setServerAutoReconnect(volumeId, on),
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
vi.mock('$lib/utils/confirm-dialog', () => ({ confirmDialog: (...args: unknown[]) => confirmDialog(...(args as [])) }))

import { createHubActions } from './servers-hub-actions'
import type { HubRow } from './servers-hub-rows'
import type { NetworkHostContextActionKind, VolumeContextActionKind } from '$lib/ipc/bindings'

const host: NetworkHost = { id: 'h1', name: 'Attic NAS', ipAddress: '10.0.0.4', port: 445, source: 'manual' }

/** A one-place server: the servers family speaks for it. */
const placeRow: HubRow = {
  id: 'sftp-nas.local-22-ada',
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
      },
    ],
  },
  host: null,
}

/** A saved SMB host: a manual-server entry, with no place to act on. */
const savedHostRow: HubRow = {
  id: 'manual-10-0-0-4-445',
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
    getHosts: () => [host],
    getVolumes: () => volumes,
    refreshSaved,
    openServer,
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
})

describe('forget', () => {
  it('sends a one-place server to the servers family, so the hub asks what the switcher asks', async () => {
    await actions().forget(placeRow)
    expect(forgetSavedServer).toHaveBeenCalledWith('sftp-nas.local-22-ada', 'Naspolya')
    expect(removeManualServer).not.toHaveBeenCalled()
  })

  it('removes a saved SMB host from the manual store instead, after asking', async () => {
    await actions().forget(savedHostRow)
    expect(confirmDialog).toHaveBeenCalledOnce()
    expect(removeManualServer).toHaveBeenCalledWith('manual-10-0-0-4-445')
    expect(forgetSavedServer).not.toHaveBeenCalled()
  })

  it('re-reads the saved list after removing a host, which nothing broadcasts', async () => {
    await actions().forget(savedHostRow)
    expect(refreshSaved).toHaveBeenCalledOnce()
  })

  it('keeps a host the user said no to', async () => {
    confirmDialog.mockResolvedValue(false)
    await actions().forget(savedHostRow)
    expect(removeManualServer).not.toHaveBeenCalled()
  })

  it('says a discovered host is not the user’s to remove, rather than doing nothing', async () => {
    await actions().forget(nearbyOnlyRow)
    expect(removeManualServer).not.toHaveBeenCalled()
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
    expect(openServer).toHaveBeenCalledWith(placeRow)
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
  it('raises the SMB host menu for a host row, with what it knows about its password', async () => {
    await actions().openHostMenu(savedHostRow)
    expect(showNetworkHostContextMenu).toHaveBeenCalledWith('h1', 'Attic NAS', true, true)
  })
})

describe('runHostAction', () => {
  // ❗ Typed, so a spelling that drifts from the Rust enum is a compile error
  // rather than a silently-dead menu item.
  const payload = (action: NetworkHostContextActionKind) => ({ action, hostId: 'h1', hostName: 'Attic NAS' })

  it('routes the host menu’s Forget back through the same branch F8 takes', async () => {
    await actions().runHostAction(payload('forget-server'))
    expect(removeManualServer).toHaveBeenCalledWith('manual-10-0-0-4-445')
  })

  it('forgets the host’s stored password without touching the host itself', async () => {
    await actions().runHostAction(payload('forget-secret'))
    expect(forgetCredentials).toHaveBeenCalledWith('Attic NAS')
    expect(removeManualServer).not.toHaveBeenCalled()
  })

  it('unmounts an SMB host’s shares, which is what its Disconnect means', async () => {
    await actions().runHostAction(payload('disconnect'))
    expect(disconnectNetworkHost).toHaveBeenCalledWith('h1', 'Attic NAS', '10.0.0.4')
    expect(addToast).toHaveBeenCalledWith('Disconnected from Attic NAS', { level: 'success' })
  })

  it('treats nothing-to-unmount as a normal answer, not a fault', async () => {
    disconnectNetworkHost.mockResolvedValueOnce([])
    await actions().runHostAction(payload('disconnect'))
    expect(addToast).toHaveBeenCalledWith('No mounted shares from Attic NAS')
  })

  it('ignores an action for a host the discovery store no longer has', async () => {
    await actions().runHostAction({ action: 'disconnect', hostId: 'gone', hostName: 'Gone' })
    expect(disconnectNetworkHost).not.toHaveBeenCalled()
  })
})
