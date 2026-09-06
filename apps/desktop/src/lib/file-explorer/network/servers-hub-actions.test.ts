/**
 * What F8 and a right-click do to each kind of hub row.
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
const openServerRowMenu = vi.fn((_volume: VolumeInfo) => Promise.resolve())
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
vi.mock('../navigation/server-row-actions', () => ({
  forgetSavedServer: (...args: unknown[]) => forgetSavedServer(...(args as [])),
  openServerRowMenu: (volume: VolumeInfo) => openServerRowMenu(volume),
}))
vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    addToast(...(args as []))
  },
}))
vi.mock('$lib/utils/confirm-dialog', () => ({ confirmDialog: (...args: unknown[]) => confirmDialog(...(args as [])) }))

import { createHubActions } from './servers-hub-actions'
import type { HubRow } from './servers-hub-rows'

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
    address: 'nas.local:22',
    username: 'ada',
    pinned: true,
    lastConnectedAt: null,
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
    address: '10.0.0.4',
    username: null,
    pinned: false,
    lastConnectedAt: null,
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
    expect(addToast).toHaveBeenCalledWith("Can't remove discovered hosts", { level: 'warn' })
  })
})

describe('openMenu', () => {
  it('raises the servers menu for a one-place row, the same one the switcher raises', async () => {
    await actions([liveVolume]).openMenu(placeRow)
    expect(openServerRowMenu).toHaveBeenCalledWith(liveVolume)
    expect(showNetworkHostContextMenu).not.toHaveBeenCalled()
  })

  it('stands in for a place the volume list has no row for, rather than skipping the menu', async () => {
    // A saved server that is neither pinned nor connected isn't in the listing.
    await actions([]).openMenu(placeRow)
    const volume = openServerRowMenu.mock.calls[0][0]
    expect(volume.id).toBe('sftp-nas.local-22-ada')
    expect(volume.path).toBe('sftp://ada@nas.local:22')
    expect(volume.connectionState).toBeNull()
  })

  it('raises the SMB host menu for a host row, with what it knows about its password', async () => {
    await actions().openMenu(savedHostRow)
    expect(showNetworkHostContextMenu).toHaveBeenCalledWith('h1', 'Attic NAS', true, true)
    expect(openServerRowMenu).not.toHaveBeenCalled()
  })
})

describe('runHostAction', () => {
  const payload = (action: string) => ({ action, hostId: 'h1', hostName: 'Attic NAS' })

  it('routes the host menu’s Forget back through the same branch F8 takes', async () => {
    await actions().runHostAction(payload('forget-server'))
    expect(removeManualServer).toHaveBeenCalledWith('manual-10-0-0-4-445')
  })

  it('forgets the host’s stored password without touching the host itself', async () => {
    await actions().runHostAction(payload('forget-password'))
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
