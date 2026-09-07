/**
 * What a server row's menu items actually do.
 *
 * Two things are pinned here and nowhere else: the two Forgets ASK FIRST (both
 * are irreversible from the UI, and one of them takes a Keychain entry with it),
 * and no refusal reaches a person as `String(e)` — these commands answer a bool,
 * so anything thrown is IPC transport text.
 */
import { describe, it, expect, vi, beforeEach, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'

const disconnectPlace = vi.fn(() => Promise.resolve(true))
const forgetServer = vi.fn(() => Promise.resolve(true))
const forgetServerSecret = vi.fn(() => Promise.resolve(true))
const hasServerSecret = vi.fn(() => Promise.resolve(true))
const listSavedServers = vi.fn(() =>
  Promise.resolve([{ id: 'sftp-nas-local-22-ada', places: [{ volumeId: 'sftp-nas-local-22-ada' }] }]),
)
const setPlacePinned = vi.fn(() => Promise.resolve(true))
const showVolumeRowContextMenu = vi.fn(() => Promise.resolve())
const addToast = vi.fn()
const confirmDialog = vi.fn(() => Promise.resolve(true))
const openEditServerSheet = vi.fn(() => Promise.resolve({ kind: 'cancelled' as const }))

vi.mock('$lib/tauri-commands', () => ({
  disconnectPlace: (...args: unknown[]) => disconnectPlace(...(args as [])),
  forgetServer: (...args: unknown[]) => forgetServer(...(args as [])),
  forgetServerSecret: (...args: unknown[]) => forgetServerSecret(...(args as [])),
  hasServerSecret: (...args: unknown[]) => hasServerSecret(...(args as [])),
  listSavedServers: () => listSavedServers(),
  setPlacePinned: (...args: unknown[]) => setPlacePinned(...(args as [])),
  showVolumeRowContextMenu: (...args: unknown[]) => {
    void showVolumeRowContextMenu(...(args as []))
    return Promise.resolve()
  },
}))
vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    addToast(...(args as []))
  },
}))
vi.mock('$lib/utils/confirm-dialog', () => ({ confirmDialog: (...args: unknown[]) => confirmDialog(...(args as [])) }))
vi.mock('$lib/servers/open-sign-in', () => ({
  openEditServerSheet: (...args: unknown[]) => openEditServerSheet(...(args as [])),
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { isServerPlaceRow, openServerRowMenu, runServerRowAction } from './server-row-actions'
import type { VolumeInfo } from '../types'

const place: VolumeInfo = {
  id: 'sftp-nas-local-22-ada',
  name: 'Naspolya',
  path: 'sftp://ada@nas.local:22/srv/data',
  category: 'network',
  fsType: 'sftp',
  isEjectable: false,
  connectionState: 'direct',
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

describe('isServerPlaceRow', () => {
  it('claims SFTP and WebDAV places', () => {
    expect(isServerPlaceRow(place)).toBe(true)
    expect(isServerPlaceRow({ ...place, id: 'webdav-nas-local-5006-ada' })).toBe(true)
  })

  it('❌ leaves a mounted SMB share alone: its session is an OS mount', () => {
    // The failure this prevents: Disconnect on a share, calling a command that
    // doesn't speak SMB, doing nothing at all. SMB joins the family in M3.
    expect(isServerPlaceRow({ ...place, id: 'smb-naspi-media', fsType: 'smbfs' })).toBe(false)
    expect(isServerPlaceRow({ ...place, id: 'root', category: 'main_volume' })).toBe(false)
  })
})

describe('openServerRowMenu', () => {
  it('reads the row and both stores, and hands the answer to the native menu', async () => {
    await openServerRowMenu(place)
    expect(showVolumeRowContextMenu).toHaveBeenCalledWith('sftp-nas-local-22-ada', 'Naspolya', false, false, {
      showsDisconnect: true,
      isSaved: true,
      hasSavedSecret: true,
      pinned: false,
    })
  })

  it('a saved row shows no Disconnect: there is no session to end', async () => {
    await openServerRowMenu({ ...place, connectionState: 'saved' })
    expect(showVolumeRowContextMenu).toHaveBeenCalledWith(
      'sftp-nas-local-22-ada',
      'Naspolya',
      false,
      false,
      expect.objectContaining({ showsDisconnect: false }),
    )
  })

  it('a store that does not answer costs the row an item, never the menu', async () => {
    hasServerSecret.mockRejectedValueOnce(new Error('keychain busy'))
    listSavedServers.mockRejectedValueOnce(new Error('store busy'))
    await openServerRowMenu(place)
    expect(showVolumeRowContextMenu).toHaveBeenCalledWith(
      'sftp-nas-local-22-ada',
      'Naspolya',
      false,
      false,
      expect.objectContaining({ isSaved: false, hasSavedSecret: false }),
    )
  })
})

describe('runServerRowAction', () => {
  const payload = (action: string) => ({
    action: action as never,
    volumeId: 'sftp-nas-local-22-ada',
    volumeName: 'Naspolya',
  })

  it('disconnects without asking: the place stays saved, so nothing is lost', async () => {
    await runServerRowAction(payload('disconnect'))
    expect(confirmDialog).not.toHaveBeenCalled()
    expect(disconnectPlace).toHaveBeenCalledWith('sftp-nas-local-22-ada')
  })

  it('asks before forgetting a server, and before forgetting its password', async () => {
    await runServerRowAction(payload('forget-server'))
    await runServerRowAction(payload('forget-secret'))
    expect(confirmDialog).toHaveBeenCalledTimes(2)
    expect(forgetServer).toHaveBeenCalledWith('sftp-nas-local-22-ada')
    expect(forgetServerSecret).toHaveBeenCalledWith('sftp-nas-local-22-ada')
  })

  it('does nothing when the user says no', async () => {
    confirmDialog.mockResolvedValue(false)
    await runServerRowAction(payload('forget-server'))
    await runServerRowAction(payload('forget-secret'))
    expect(forgetServer).not.toHaveBeenCalled()
    expect(forgetServerSecret).not.toHaveBeenCalled()
  })

  it('words a refusal itself, and never puts the thrown value in front of a person', async () => {
    disconnectPlace.mockRejectedValueOnce(new Error('ipc channel closed: 0x8007'))
    await runServerRowAction(payload('disconnect'))
    expect(addToast).toHaveBeenCalledTimes(1)
    const [message] = addToast.mock.calls[0] as [string]
    expect(message).toContain('Naspolya')
    expect(message).not.toContain('0x8007')
  })

  it('leaves eject and the favorite actions to their owners', async () => {
    for (const action of ['eject', 'rename-favorite', 'remove-favorite']) {
      await runServerRowAction(payload(action))
    }
    expect(disconnectPlace).not.toHaveBeenCalled()
    expect(forgetServer).not.toHaveBeenCalled()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('moves the pin both ways, and says the server is still saved when it comes out', async () => {
    await runServerRowAction(payload('pin'))
    expect(setPlacePinned).toHaveBeenCalledWith('sftp-nas-local-22-ada', true)

    await runServerRowAction(payload('unpin'))
    expect(setPlacePinned).toHaveBeenLastCalledWith('sftp-nas-local-22-ada', false)
    // ❗ The unpin toast has to say the server survives: nothing was deleted, and
    // a person who reads "removed" will re-add a server they still have.
    expect(addToast).toHaveBeenLastCalledWith("Naspolya is out of your volume switcher. It's still saved.", {
      level: 'success',
    })
  })

  it('asks nothing before moving a pin, unlike the two Forgets', async () => {
    await runServerRowAction(payload('pin'))
    // Unpinning loses nothing and the same command puts it back, so a
    // confirmation here would be friction with nothing behind it.
    expect(confirmDialog).not.toHaveBeenCalled()
  })

  it('Open navigates through the hook the pane supplies, and is quiet without one', async () => {
    const onOpen = vi.fn()
    await runServerRowAction({ ...payload('open'), onOpen })
    expect(onOpen).toHaveBeenCalledWith('sftp-nas-local-22-ada')

    // The `navigate()` transaction lives in the pane. A menu raised where there
    // is no pane to move says nothing rather than pretending.
    await runServerRowAction(payload('open'))
    expect(addToast).not.toHaveBeenCalled()
  })

  it('Edit opens the sheet on the SAVED server, never on the row', async () => {
    await runServerRowAction(payload('edit'))
    // ❗ From the store: a `VolumeInfo` carries no key file, no remote folder,
    // and no auto-reconnect switch, so a form seeded from the row would save the
    // other half away.
    expect(openEditServerSheet).toHaveBeenCalledWith({
      id: 'sftp-nas-local-22-ada',
      places: [{ volumeId: 'sftp-nas-local-22-ada' }],
    })
  })

  it('Edit on a server a forget already took says nothing', async () => {
    listSavedServers.mockResolvedValueOnce([])
    await runServerRowAction(payload('edit'))
    expect(openEditServerSheet).not.toHaveBeenCalled()
    // The row is already gone from the switcher; a toast about it is noise.
    expect(addToast).not.toHaveBeenCalled()
  })
})
