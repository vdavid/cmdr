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
  isUnderServerRoot,
  joinServerPath,
  parseServerPath,
  serverAppRoot,
  serverProtocolOfVolumeId,
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

  it('reads an account that is an email address, splitting at the LAST at sign the way Rust does', () => {
    // Email logins are common on Nextcloud, Fastmail, and other WebDAV hosts, and
    // Rust mints the app root with the username raw.
    // `cmdr_fs::volume::ids::server_of_path` takes the account as everything
    // before the authority's last `@`, so this side has to as well.
    expect(parseServerPath('webdav://ada@example.com@cloud.example.com:443/remote.php/dav')).toEqual({
      protocol: 'webdav',
      username: 'ada@example.com',
      host: 'cloud.example.com',
      port: 443,
      path: 'remote.php/dav',
    })
    expect(parseServerPath('sftp://ada@corp.example@nas.local:22')).toEqual({
      protocol: 'sftp',
      username: 'ada@corp.example',
      host: 'nas.local',
      port: 22,
      path: '',
    })
  })

  it('reads an IPv6 literal host, taking the port after the LAST colon the way Rust does', () => {
    expect(parseServerPath('sftp://ada@::1:22/srv')).toEqual({
      protocol: 'sftp',
      username: 'ada',
      host: '::1',
      port: 22,
      path: 'srv',
    })
  })

  it('reads an S3 path: the account is the access key id, the first segment the bucket', () => {
    // `cmdr_fs::volume::s3_app_root` mints the ACCOUNT as the prefix, and a place
    // hangs under it: `/` for the account root, `/<bucket>` for a bucket.
    expect(parseServerPath('s3://AKIAEXAMPLE@s3.eu-west-1.amazonaws.com:443/photos/2026/a.jpg')).toEqual({
      protocol: 's3',
      username: 'AKIAEXAMPLE',
      host: 's3.eu-west-1.amazonaws.com',
      port: 443,
      path: 'photos/2026/a.jpg',
    })
    // The account root's app root carries a trailing slash, and reads as the root.
    expect(parseServerPath('s3://AKIAEXAMPLE@127.0.0.1:14480/')?.path).toBe('')
  })

  it('refuses anything that is not a server path', () => {
    // ❗ Every one of these would otherwise be handed to a server as a request.
    expect(parseServerPath('/srv/data/photos')).toBeNull()
    expect(parseServerPath('adb://R58M12345/sdcard')).toBeNull()
    expect(parseServerPath('smb://naspolya')).toBeNull()
    expect(parseServerPath('sftp://nas.local:22/srv')).toBeNull() // no account
    expect(parseServerPath('sftp://@nas.local:22/srv')).toBeNull() // an empty account
    expect(parseServerPath('sftp://ada@:22/srv')).toBeNull() // no host
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
  it('recognizes the three schemes and nothing else', () => {
    expect(isServerPath('sftp://ada@nas.local:22/srv')).toBe(true)
    expect(isServerPath('s3://AKIAEXAMPLE@127.0.0.1:14480/bucket')).toBe(true)
    expect(isServerPath('s3:/not-a-path')).toBe(false)
    expect(isServerPath('webdav://ada@nas.local:5006/')).toBe(true)
    expect(isServerPath('smb://naspolya')).toBe(false)
    expect(isServerPath('/Volumes/naspi')).toBe(false)
  })

  it('recognizes the ids the two volume-id minters produce', () => {
    expect(isServerVolumeId('sftp-nas-local-22-ada')).toBe(true)
    expect(isServerVolumeId('webdav-nas-local-5006-ada')).toBe(true)
    expect(isServerVolumeId('s3-127-0-0-1-14480-akiaexample-photos-1a2b')).toBe(true)
    expect(serverProtocolOfVolumeId('s3-127-0-0-1-14480-akiaexample-1a2b')).toBe('s3')
    expect(isServerVolumeId('root')).toBe(false)
    expect(isServerVolumeId('adb-pixel-9a3f')).toBe(false)
  })
})

describe('isUnderServerRoot', () => {
  it('matches by whole components, never a string prefix', () => {
    expect(isUnderServerRoot('sftp://ada@nas:22/srv/data', 'sftp://ada@nas:22/srv/data')).toBe(true)
    expect(isUnderServerRoot('sftp://ada@nas:22/srv/data', 'sftp://ada@nas:22/srv/data/x')).toBe(true)
    expect(isUnderServerRoot('sftp://ada@nas:22/srv/data', 'sftp://ada@nas:22/srv/data-1')).toBe(false)
  })

  it('reads a root with a trailing slash the way Rust does, which is how an S3 account root is spelled', () => {
    // `server_volumes::path_is_under` trims the root's trailing `/` first: the
    // account root's app root is `s3://<key>@<host>:<port>/`.
    const accountRoot = 's3://AKIA@127.0.0.1:14480/'
    expect(isUnderServerRoot(accountRoot, 's3://AKIA@127.0.0.1:14480/photos/a.jpg')).toBe(true)
    expect(isUnderServerRoot(accountRoot, 's3://AKIA@127.0.0.1:14480')).toBe(true)
    expect(isUnderServerRoot(accountRoot, 's3://AKIA@127.0.0.1:144800/x')).toBe(false)
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

  it('walks and joins under an account that is an email address', () => {
    const root = 'webdav://ada@example.com@cloud.example.com:443'
    expect(getServerParentPath(`${root}/remote.php/dav`)).toBe(`${root}/remote.php`)
    expect(getServerParentPath(`${root}/remote.php`)).toBe(root)
    expect(joinServerPath(root, 'remote.php')).toBe(`${root}/remote.php`)
    expect(getServerDisplayPath(`${root}/remote.php`)).toBe('/remote.php')
  })

  it('walks an S3 bucket up to the account root, and no further', () => {
    const account = 's3://AKIAEXAMPLE@127.0.0.1:14480'
    expect(getServerParentPath(`${account}/photos/2026`)).toBe(`${account}/photos`)
    expect(getServerParentPath(`${account}/photos`)).toBe(account)
    expect(getServerParentPath(`${account}/`)).toBeNull()
    expect(joinServerPath(account, 'photos')).toBe(`${account}/photos`)
  })

  it('shows the server-side path, which is what a person on that server would type', () => {
    expect(getServerDisplayPath('sftp://ada@nas.local:22/srv/data')).toBe('/srv/data')
    expect(getServerDisplayPath('sftp://ada@nas.local:22')).toBe('/')
    expect(getServerDisplayPath('/Volumes/naspi')).toBe('/Volumes/naspi')
  })
})
