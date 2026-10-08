/**
 * The WebDAV wrappers.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    saveWebdavCredentials: vi.fn(),
    getKnownWebdavServers: vi.fn(),
    getWebdavUnattendedReconnect: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { getKnownWebdavServers, getWebdavUnattendedReconnect, saveWebdavCredentials } from './webdav'

const URL = 'https://dav.example.test/remote.php/dav/'

const ok = { status: 'ok' as const, data: null }
const err = { status: 'error' as const, error: { type: 'access_denied' as const, message: 'nope' } }

beforeEach(() => {
  vi.clearAllMocks()
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
})
