/**
 * The SFTP wrappers, whose one real risk is argument order: `connectSftpVolume` and
 * `updateKnownSftpServer` both take seven positional arguments, and swapping two
 * strings compiles fine and connects to the wrong thing.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    connectSftpVolume: vi.fn(),
    disconnectSftpVolume: vi.fn(),
    approveSftpHostKey: vi.fn(),
    forgetSftpHostKey: vi.fn(),
    listTrustedSftpHostKeys: vi.fn(),
    saveSftpCredentials: vi.fn(),
    hasSftpCredentials: vi.fn(),
    deleteSftpCredentials: vi.fn(),
    getKnownSftpServers: vi.fn(),
    updateKnownSftpServer: vi.fn(),
    forgetKnownSftpServer: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import {
  approveSftpHostKey,
  connectSftpVolume,
  deleteSftpCredentials,
  disconnectSftpVolume,
  forgetKnownSftpServer,
  forgetSftpHostKey,
  getKnownSftpServers,
  hasSftpCredentials,
  listTrustedSftpHostKeys,
  saveSftpCredentials,
  updateKnownSftpServer,
  type SftpTarget,
} from './sftp'

const target: SftpTarget = {
  displayName: 'Naspolya',
  host: 'naspolya.local',
  port: 2222,
  username: 'ada',
  remoteRoot: '/srv/data',
  keyFile: '/Users/ada/.ssh/id_ed25519',
  useAgent: true,
}

const ok = { status: 'ok' as const, data: null }
const err = { status: 'error' as const, error: { type: 'access_denied' as const, message: 'nope' } }

beforeEach(() => {
  vi.clearAllMocks()
})

describe('connecting', () => {
  it('hands every field to the command in the order it expects', async () => {
    vi.mocked(commands.connectSftpVolume).mockResolvedValueOnce({ outcome: 'unreachable' })
    await connectSftpVolume(target)
    expect(commands.connectSftpVolume).toHaveBeenCalledWith(
      'Naspolya',
      'naspolya.local',
      2222,
      'ada',
      '/srv/data',
      '/Users/ada/.ssh/id_ed25519',
      true,
    )
  })

  it('sends an absent key file as null rather than undefined', async () => {
    // `undefined` drops out of the JSON payload entirely, and the Rust side would
    // then see a missing argument instead of "no key file".
    vi.mocked(commands.connectSftpVolume).mockResolvedValueOnce({ outcome: 'unreachable' })
    await connectSftpVolume({ ...target, keyFile: undefined })
    expect(vi.mocked(commands.connectSftpVolume).mock.calls[0]?.[5]).toBeNull()
  })

  it('passes the outcome straight through, so the caller switches on it', async () => {
    const prompt = {
      outcome: 'needs_host_key_approval' as const,
      host: 'naspolya.local',
      port: 2222,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:aaa',
      kind: 'changed' as const,
    }
    vi.mocked(commands.connectSftpVolume).mockResolvedValueOnce(prompt)
    expect(await connectSftpVolume(target)).toEqual(prompt)
  })

  it('disconnectSftpVolume forwards the volume id', async () => {
    vi.mocked(commands.disconnectSftpVolume).mockResolvedValueOnce(true)
    expect(await disconnectSftpVolume('sftp-naspolya-abc')).toBe(true)
    expect(commands.disconnectSftpVolume).toHaveBeenCalledWith('sftp-naspolya-abc')
  })
})

describe('host-key trust', () => {
  it('approve forwards the whole key, because the backend re-checks the fingerprint', async () => {
    vi.mocked(commands.approveSftpHostKey).mockResolvedValueOnce({ outcome: 'recorded' })
    await approveSftpHostKey({
      host: 'naspolya.local',
      port: 2222,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:aaa',
    })
    expect(commands.approveSftpHostKey).toHaveBeenCalledWith('naspolya.local', 2222, 'ssh-ed25519', 'SHA256:aaa')
  })

  it('forget is keyed by algorithm, not by host alone', async () => {
    vi.mocked(commands.forgetSftpHostKey).mockResolvedValueOnce(true)
    await forgetSftpHostKey('naspolya.local', 2222, 'ssh-ed25519')
    expect(commands.forgetSftpHostKey).toHaveBeenCalledWith('naspolya.local', 2222, 'ssh-ed25519')
  })

  it('listing passes the store through', async () => {
    vi.mocked(commands.listTrustedSftpHostKeys).mockResolvedValueOnce([])
    expect(await listTrustedSftpHostKeys()).toEqual([])
  })
})

describe('credentials', () => {
  it('saving forwards host, port, account, and secret', async () => {
    vi.mocked(commands.saveSftpCredentials).mockResolvedValueOnce(ok)
    await saveSftpCredentials('naspolya.local', 2222, 'ada', 'pa55')
    expect(commands.saveSftpCredentials).toHaveBeenCalledWith('naspolya.local', 2222, 'ada', 'pa55')
  })

  it('a refusing store throws rather than reporting success', async () => {
    vi.mocked(commands.saveSftpCredentials).mockResolvedValueOnce(err)
    await expect(saveSftpCredentials('naspolya.local', 2222, 'ada', 'pa55')).rejects.toThrow('nope')
  })

  it('deleting throws on refusal too', async () => {
    vi.mocked(commands.deleteSftpCredentials).mockResolvedValueOnce(err)
    await expect(deleteSftpCredentials('naspolya.local', 2222, 'ada')).rejects.toThrow('nope')
  })

  it('asking whether one is stored is keyed per account', async () => {
    vi.mocked(commands.hasSftpCredentials).mockResolvedValueOnce(true)
    expect(await hasSftpCredentials('naspolya.local', 2222, 'ada')).toBe(true)
    expect(commands.hasSftpCredentials).toHaveBeenCalledWith('naspolya.local', 2222, 'ada')
  })
})

describe('the saved-server list', () => {
  it('reads the list through', async () => {
    vi.mocked(commands.getKnownSftpServers).mockResolvedValueOnce([])
    expect(await getKnownSftpServers()).toEqual([])
  })

  it('update reorders the target into the argument order the command takes', async () => {
    // ❗ Not the same order as `connectSftpVolume`: the identity triple comes
    // first here, and getting it wrong would silently write a different server.
    vi.mocked(commands.updateKnownSftpServer).mockResolvedValueOnce(undefined)
    await updateKnownSftpServer(target)
    expect(commands.updateKnownSftpServer).toHaveBeenCalledWith(
      'naspolya.local',
      2222,
      'ada',
      'Naspolya',
      '/srv/data',
      '/Users/ada/.ssh/id_ed25519',
      true,
    )
  })

  it('forget is keyed by the same triple the volume id is', async () => {
    vi.mocked(commands.forgetKnownSftpServer).mockResolvedValueOnce(true)
    await forgetKnownSftpServer('naspolya.local', 2222, 'ada')
    expect(commands.forgetKnownSftpServer).toHaveBeenCalledWith('naspolya.local', 2222, 'ada')
  })
})
