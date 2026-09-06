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
const showVolumeRowContextMenu = vi.fn(() => Promise.resolve())
const addToast = vi.fn()
const confirmDialog = vi.fn(() => Promise.resolve(true))

vi.mock('$lib/tauri-commands', () => ({
  disconnectPlace: (...args: unknown[]) => disconnectPlace(...(args as [])),
  forgetServer: (...args: unknown[]) => forgetServer(...(args as [])),
  forgetServerSecret: (...args: unknown[]) => forgetServerSecret(...(args as [])),
  hasServerSecret: (...args: unknown[]) => hasServerSecret(...(args as [])),
  listSavedServers: () => listSavedServers(),
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

  it('takes the four not-yet-built items quietly', async () => {
    for (const action of ['open', 'pin', 'unpin', 'edit']) {
      await runServerRowAction(payload(action))
    }
    expect(addToast).not.toHaveBeenCalled()
  })
})
