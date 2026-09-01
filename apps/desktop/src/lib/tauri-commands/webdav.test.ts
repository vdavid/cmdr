/**
 * The WebDAV wrappers, whose one real risk is argument order: `connectWebdavVolume`
 * and `updateKnownWebdavServer` both take several positional strings, and swapping
 * two compiles fine and connects to the wrong thing.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    connectWebdavVolume: vi.fn(),
    cancelWebdavConnect: vi.fn(),
    disconnectWebdavVolume: vi.fn(),
    saveWebdavCredentials: vi.fn(),
    hasWebdavCredentials: vi.fn(),
    deleteWebdavCredentials: vi.fn(),
    getKnownWebdavServers: vi.fn(),
    updateKnownWebdavServer: vi.fn(),
    forgetKnownWebdavServer: vi.fn(),
    getWebdavUnattendedReconnect: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import {
  cancelWebdavConnect,
  connectWebdavVolume,
  deleteWebdavCredentials,
  disconnectWebdavVolume,
  forgetKnownWebdavServer,
  getKnownWebdavServers,
  getWebdavUnattendedReconnect,
  hasWebdavCredentials,
  newWebdavAttemptId,
  saveWebdavCredentials,
  updateKnownWebdavServer,
  type WebdavTarget,
} from './webdav'

const URL = 'https://dav.example.test/remote.php/dav/'

const target: WebdavTarget = {
  displayName: 'Example',
  url: URL,
  username: 'ada',
  remoteRoot: '/Photos',
  autoReconnect: true,
}

const ok = { status: 'ok' as const, data: null }
const err = { status: 'error' as const, error: { type: 'access_denied' as const, message: 'nope' } }

beforeEach(() => {
  vi.clearAllMocks()
})

describe('connecting', () => {
  it('hands every field to the command in the order it expects', async () => {
    vi.mocked(commands.connectWebdavVolume).mockResolvedValueOnce({ outcome: 'unreachable' })
    await connectWebdavVolume(target, 'attempt-1')
    expect(commands.connectWebdavVolume).toHaveBeenCalledWith('Example', URL, 'ada', '/Photos', true, 'attempt-1')
  })

  it('passes the outcome straight through, so the caller switches on it', async () => {
    const connected = { outcome: 'connected' as const, volumeId: 'webdav-dav-example-test-abc' }
    vi.mocked(commands.connectWebdavVolume).mockResolvedValueOnce(connected)
    expect(await connectWebdavVolume(target, 'attempt-2')).toEqual(connected)
  })

  it('cancelWebdavConnect forwards the attempt id, so a dialog can call its own dial off', async () => {
    vi.mocked(commands.cancelWebdavConnect).mockResolvedValueOnce(true)
    expect(await cancelWebdavConnect('attempt-1')).toBe(true)
    expect(commands.cancelWebdavConnect).toHaveBeenCalledWith('attempt-1')
  })

  it('newWebdavAttemptId makes a fresh id every time, so two dialogs never cancel each other', () => {
    expect(newWebdavAttemptId()).not.toBe(newWebdavAttemptId())
  })

  it('disconnectWebdavVolume forwards the volume id', async () => {
    vi.mocked(commands.disconnectWebdavVolume).mockResolvedValueOnce(true)
    expect(await disconnectWebdavVolume('webdav-dav-example-test-abc')).toBe(true)
    expect(commands.disconnectWebdavVolume).toHaveBeenCalledWith('webdav-dav-example-test-abc')
  })
})

describe('credentials', () => {
  it('saving forwards url, account, and secret', async () => {
    vi.mocked(commands.saveWebdavCredentials).mockResolvedValueOnce(ok)
    await saveWebdavCredentials(URL, 'ada', 'pa55')
    expect(commands.saveWebdavCredentials).toHaveBeenCalledWith(URL, 'ada', 'pa55')
  })

  it('a refusing store throws rather than reporting success', async () => {
    vi.mocked(commands.saveWebdavCredentials).mockResolvedValueOnce(err)
    await expect(saveWebdavCredentials(URL, 'ada', 'pa55')).rejects.toThrow('nope')
  })

  it('deleting throws on refusal too', async () => {
    vi.mocked(commands.deleteWebdavCredentials).mockResolvedValueOnce(err)
    await expect(deleteWebdavCredentials(URL, 'ada')).rejects.toThrow('nope')
  })

  it('asking whether one is stored is keyed per account', async () => {
    vi.mocked(commands.hasWebdavCredentials).mockResolvedValueOnce(true)
    expect(await hasWebdavCredentials(URL, 'ada')).toBe(true)
    expect(commands.hasWebdavCredentials).toHaveBeenCalledWith(URL, 'ada')
  })
})

describe('the saved-server list', () => {
  it('reads the list through', async () => {
    vi.mocked(commands.getKnownWebdavServers).mockResolvedValueOnce([])
    expect(await getKnownWebdavServers()).toEqual([])
  })

  it('fills in the auto-reconnect switch for a server saved before it existed', async () => {
    const saved = {
      url: URL,
      username: 'ada',
      displayName: 'Example',
      remoteRoot: '/',
      lastConnectedAt: '2026-09-01T10:00:00Z',
    }
    vi.mocked(commands.getKnownWebdavServers).mockResolvedValueOnce([saved])

    const servers = await getKnownWebdavServers()

    expect(servers.map((s) => s.autoReconnect)).toEqual([true])
  })

  it('leaves a switch the user turned off turned off', async () => {
    const saved = {
      url: URL,
      username: 'ada',
      displayName: 'Example',
      remoteRoot: '/',
      autoReconnect: false,
      lastConnectedAt: '2026-09-01T10:00:00Z',
    }
    vi.mocked(commands.getKnownWebdavServers).mockResolvedValueOnce([saved])

    const servers = await getKnownWebdavServers()

    expect(servers.map((s) => s.autoReconnect)).toEqual([false])
  })

  it('reads the unattended-reconnect answer straight through, so nothing derives it', async () => {
    vi.mocked(commands.getWebdavUnattendedReconnect).mockResolvedValueOnce('no_stored_secret')
    expect(await getWebdavUnattendedReconnect('webdav-dav-example-test-abc')).toBe('no_stored_secret')
    expect(commands.getWebdavUnattendedReconnect).toHaveBeenCalledWith('webdav-dav-example-test-abc')
  })

  it('update reorders the target into the argument order the command takes', async () => {
    // ❗ Not the same order as `connectWebdavVolume`: the identity pair comes
    // first here, and getting it wrong would silently write a different server.
    vi.mocked(commands.updateKnownWebdavServer).mockResolvedValueOnce(undefined)
    await updateKnownWebdavServer(target)
    expect(commands.updateKnownWebdavServer).toHaveBeenCalledWith(URL, 'ada', 'Example', '/Photos', true)
  })

  it('forget is keyed by the same pair the store is', async () => {
    vi.mocked(commands.forgetKnownWebdavServer).mockResolvedValueOnce(true)
    await forgetKnownWebdavServer(URL, 'ada')
    expect(commands.forgetKnownWebdavServer).toHaveBeenCalledWith(URL, 'ada')
  })
})
