/**
 * The sheets' per-protocol store reads and writes: each protocol reaches its own
 * store, and S3's are the ACCOUNT's (secret) or the PLACE's (edit form).
 */
import { describe, expect, it, vi, beforeEach, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { SavedServer, ServerTarget } from '$lib/ipc/bindings'

const m = vi.hoisted(() => ({
  getKnownSftpServers: vi.fn(),
  getKnownWebdavServers: vi.fn(),
  knownS3PlaceOf: vi.fn(),
  saveSftpCredentials: vi.fn(),
  saveWebdavCredentials: vi.fn(),
  saveS3Credentials: vi.fn(),
  getSftpUnattendedReconnect: vi.fn(),
  getWebdavUnattendedReconnect: vi.fn(),
  getS3UnattendedReconnect: vi.fn(),
}))
vi.mock('$lib/tauri-commands', () => m)

import { saveTargetSecret, savedEditForm, unattendedReconnectWarning } from './saved-server-io'

const server = (protocol: SavedServer['protocol'], address: string): SavedServer => ({
  id: 'id',
  protocol,
  displayName: 'x',
  nameSource: 'fallback',
  address,
  username: 'ada',
  pinned: false,
  lastConnectedAt: null,
  autoReconnect: true,
  places: [],
})

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})
beforeEach(() => {
  vi.clearAllMocks()
})

describe('savedEditForm', () => {
  it('matches SFTP and WebDAV on the published address and account', async () => {
    m.getKnownSftpServers.mockResolvedValue([
      { host: 'nas', port: 22, username: 'ada', displayName: '', remoteRoot: '/', autoReconnect: true, useAgent: true },
    ])
    expect((await savedEditForm(server('sftp', 'nas:22'), 'id'))?.protocol).toBe('sftp')
    m.getKnownWebdavServers.mockResolvedValue([])
    expect(await savedEditForm(server('webdav', 'https://dav/'), 'id')).toBeNull()
  })

  it('reads an S3 PLACE by its volume id, and nothing for an SMB host', async () => {
    m.knownS3PlaceOf.mockResolvedValue({
      volumeId: 's3-photos',
      provider: { kind: 'hetzner', location: 'nbg1' },
      accessKeyId: 'AKIA',
      bucket: 'photos',
      displayName: '',
      autoReconnect: true,
      pinned: true,
    })
    expect((await savedEditForm(server('s3', 'nbg1.your-objectstorage.com'), 's3-photos'))?.s3.bucket).toBe('photos')
    expect(m.knownS3PlaceOf).toHaveBeenCalledWith('s3-photos')
    expect(await savedEditForm(server('smb', 'nas'), 'id')).toBeNull()
  })
})

describe('saveTargetSecret', () => {
  it('files each protocol’s secret under the tuple its volume id is minted from', async () => {
    const base = { displayName: '', autoReconnect: true }
    await saveTargetSecret(
      {
        ...base,
        protocol: 'sftp',
        host: 'nas',
        port: 22,
        username: 'ada',
        remoteRoot: '/',
        startFolder: null,
        keyFile: null,
        useAgent: true,
      },
      's',
    )
    await saveTargetSecret(
      { ...base, protocol: 'webdav', url: 'https://dav/', username: 'ada', remoteRoot: '/', startFolder: null },
      's',
    )
    const s3: ServerTarget = {
      ...base,
      protocol: 's3',
      provider: { kind: 'aws', region: 'eu-west-1' },
      accessKeyId: 'AKIA',
      bucket: null,
    }
    await saveTargetSecret(s3, 's')
    expect(m.saveSftpCredentials).toHaveBeenCalledWith('nas', 22, 'ada', 's')
    expect(m.saveWebdavCredentials).toHaveBeenCalledWith('https://dav/', 'ada', 's')
    expect(m.saveS3Credentials).toHaveBeenCalledWith({ kind: 'aws', region: 'eu-west-1' }, 'AKIA', 's')
  })
})

describe('unattendedReconnectWarning', () => {
  it('warns only on each backend’s own spelling of "on, and nothing stored"', async () => {
    m.getSftpUnattendedReconnect.mockResolvedValue('needs_stored_secret')
    m.getWebdavUnattendedReconnect.mockResolvedValue('possible')
    m.getS3UnattendedReconnect.mockResolvedValue('no_stored_secret')
    expect(await unattendedReconnectWarning('id', 'sftp')).not.toBeNull()
    expect(await unattendedReconnectWarning('id', 'webdav')).toBeNull()
    expect(await unattendedReconnectWarning('id', 's3')).not.toBeNull()
    expect(await unattendedReconnectWarning('id', 'smb')).toBeNull()
  })
})
