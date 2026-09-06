/**
 * The frontend's half of the remote-path spelling.
 *
 * The Rust twin is `cmdr_fs::volume::remote_paths` plus `ids::sftp_app_root`,
 * and the two have to agree character for character: a path minted here is
 * handed to `resolve_path_to_volume`, which matches it against the prefix Rust
 * minted. The host-folding cell below is that agreement.
 */
import { describe, expect, it } from 'vitest'
import {
  constructServerPath,
  getServerDisplayPath,
  getServerParentPath,
  isServerPath,
  isServerVolumeId,
  joinServerPath,
  parseServerPath,
  serverAppRoot,
} from './server-path-utils'

describe('parseServerPath', () => {
  it('reads a full SFTP path', () => {
    expect(parseServerPath('sftp://ada@nas.local:22/srv/data/photos')).toEqual({
      protocol: 'sftp',
      username: 'ada',
      host: 'nas.local',
      port: 22,
      path: 'srv/data/photos',
    })
  })

  it('reads a WebDAV path, and a bare root', () => {
    expect(parseServerPath('webdav://ada@nas.local:5006/remote.php')).toEqual({
      protocol: 'webdav',
      username: 'ada',
      host: 'nas.local',
      port: 5006,
      path: 'remote.php',
    })
    expect(parseServerPath('sftp://ada@nas.local:22')?.path).toBe('')
    expect(parseServerPath('sftp://ada@nas.local:22/')?.path).toBe('')
  })

  it('refuses anything that is not a server path', () => {
    // ❗ Every one of these would otherwise be handed to a server as a request.
    expect(parseServerPath('/srv/data/photos')).toBeNull()
    expect(parseServerPath('adb://R58M12345/sdcard')).toBeNull()
    expect(parseServerPath('smb://naspolya')).toBeNull()
    expect(parseServerPath('sftp://nas.local:22/srv')).toBeNull() // no account
    expect(parseServerPath('sftp://ada@nas.local/srv')).toBeNull() // no port
    expect(parseServerPath('sftp://ada@nas.local:notaport/srv')).toBeNull()
    expect(parseServerPath('sftp://ada@nas.local:0/srv')).toBeNull()
    expect(parseServerPath('sftp://ada@nas.local:70000/srv')).toBeNull()
  })
})

describe('constructServerPath', () => {
  it('spells the prefix the way Rust mints it, lowercasing the host and leaving the account alone', () => {
    // `cmdr_fs::volume::ids::remote_app_root` folds exactly this much. A path
    // that folded more (or less) would miss the volume its own id names.
    expect(constructServerPath({ protocol: 'sftp', username: 'Ada', host: 'NAS.local', port: 22 }, '/srv/data')).toBe(
      'sftp://Ada@nas.local:22/srv/data',
    )
  })

  it('round-trips through the parser', () => {
    const parsed = parseServerPath('webdav://ada@nas.local:5006/dav/files')
    if (!parsed) throw new Error('the parser refused a path it minted')
    expect(constructServerPath(parsed, parsed.path)).toBe('webdav://ada@nas.local:5006/dav/files')
  })

  it('answers the bare root for the three root spellings', () => {
    const account = { protocol: 'sftp' as const, username: 'ada', host: 'nas.local', port: 22 }
    expect(constructServerPath(account, '')).toBe('sftp://ada@nas.local:22')
    expect(constructServerPath(account, '/')).toBe('sftp://ada@nas.local:22')
    expect(serverAppRoot(account)).toBe('sftp://ada@nas.local:22')
  })
})

describe('isServerPath / isServerVolumeId', () => {
  it('recognizes both schemes and nothing else', () => {
    expect(isServerPath('sftp://ada@nas.local:22/srv')).toBe(true)
    expect(isServerPath('webdav://ada@nas.local:5006/')).toBe(true)
    expect(isServerPath('smb://naspolya')).toBe(false)
    expect(isServerPath('/Volumes/naspi')).toBe(false)
  })

  it('recognizes the ids the two volume-id minters produce', () => {
    expect(isServerVolumeId('sftp-nas-local-22-ada')).toBe(true)
    expect(isServerVolumeId('webdav-nas-local-5006-ada')).toBe(true)
    expect(isServerVolumeId('root')).toBe(false)
    expect(isServerVolumeId('adb-pixel-9a3f')).toBe(false)
  })
})

describe('walking a server tree', () => {
  it('goes up one level, stopping at the volume root', () => {
    expect(getServerParentPath('sftp://ada@nas.local:22/srv/data/photos')).toBe('sftp://ada@nas.local:22/srv/data')
    expect(getServerParentPath('sftp://ada@nas.local:22/srv')).toBe('sftp://ada@nas.local:22')
    // ❗ Never above the root: the parent of the root is the root, never `/`.
    expect(getServerParentPath('sftp://ada@nas.local:22')).toBeNull()
    expect(getServerParentPath('/srv/data')).toBeNull()
  })

  it('joins a child name onto a folder', () => {
    expect(joinServerPath('sftp://ada@nas.local:22/srv', 'data')).toBe('sftp://ada@nas.local:22/srv/data')
    expect(joinServerPath('sftp://ada@nas.local:22', 'srv')).toBe('sftp://ada@nas.local:22/srv')
    expect(joinServerPath('/local/dir', 'child')).toBe('/local/dir')
  })

  it('shows the server-side path, which is what a person on that server would type', () => {
    expect(getServerDisplayPath('sftp://ada@nas.local:22/srv/data')).toBe('/srv/data')
    expect(getServerDisplayPath('sftp://ada@nas.local:22')).toBe('/')
    expect(getServerDisplayPath('/Volumes/naspi')).toBe('/Volumes/naspi')
  })
})
