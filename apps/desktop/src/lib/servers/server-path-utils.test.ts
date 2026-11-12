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
  nfsAppRoot,
  parseServerPath,
  serverAddressOf,
  serverAppRoot,
  serverProtocolOfVolumeId,
} from './server-path-utils'

describe('parseServerPath', () => {
  it('reads a full SFTP path', () => {
    expect(parseServerPath('sftp://test_user_1@testnas.local:22/srv/data/photos')).toEqual({
      protocol: 'sftp',
      username: 'test_user_1',
      host: 'testnas.local',
      port: 22,
      path: 'srv/data/photos',
    })
  })

  it('reads a WebDAV path, and a bare root', () => {
    expect(parseServerPath('webdav://test_user_1@testnas.local:5006/remote.php')).toEqual({
      protocol: 'webdav',
      username: 'test_user_1',
      host: 'testnas.local',
      port: 5006,
      path: 'remote.php',
    })
    expect(parseServerPath('sftp://test_user_1@testnas.local:22')?.path).toBe('')
    expect(parseServerPath('sftp://test_user_1@testnas.local:22/')?.path).toBe('')
  })

  it('reads an account that is an email address, splitting at the LAST at sign the way Rust does', () => {
    // Email logins are common on Nextcloud, Fastmail, and other WebDAV hosts, and
    // Rust mints the app root with the username raw.
    // `cmdr_fs::volume::ids::server_of_path` takes the account as everything
    // before the authority's last `@`, so this side has to as well.
    expect(parseServerPath('webdav://test_user_1@example.com@cloud.example.com:443/remote.php/dav')).toEqual({
      protocol: 'webdav',
      username: 'test_user_1@example.com',
      host: 'cloud.example.com',
      port: 443,
      path: 'remote.php/dav',
    })
    expect(parseServerPath('sftp://test_user_1@corp.example@testnas.local:22')).toEqual({
      protocol: 'sftp',
      username: 'test_user_1@corp.example',
      host: 'testnas.local',
      port: 22,
      path: '',
    })
  })

  it('reads an IPv6 literal host, taking the port after the LAST colon the way Rust does', () => {
    expect(parseServerPath('sftp://test_user_1@::1:22/srv')).toEqual({
      protocol: 'sftp',
      username: 'test_user_1',
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
    expect(parseServerPath('smb://testnas')).toBeNull()
    expect(parseServerPath('sftp://testnas.local:22/srv')).toBeNull() // no account
    expect(parseServerPath('sftp://@testnas.local:22/srv')).toBeNull() // an empty account
    expect(parseServerPath('sftp://test_user_1@:22/srv')).toBeNull() // no host
    expect(parseServerPath('sftp://test_user_1@testnas.local/srv')).toBeNull() // no port
    expect(parseServerPath('sftp://test_user_1@testnas.local:notaport/srv')).toBeNull()
    expect(parseServerPath('sftp://test_user_1@testnas.local:0/srv')).toBeNull()
    expect(parseServerPath('sftp://test_user_1@testnas.local:70000/srv')).toBeNull()
  })
})

describe('constructServerPath', () => {
  it('spells the prefix the way Rust mints it, lowercasing the host and leaving the account alone', () => {
    // `cmdr_fs::volume::ids::remote_app_root` folds exactly this much. A path
    // that folded more (or less) would miss the volume its own id names.
    expect(
      constructServerPath({ protocol: 'sftp', username: 'Test_user_1', host: 'TESTNAS.local', port: 22 }, '/srv/data'),
    ).toBe('sftp://Test_user_1@testnas.local:22/srv/data')
  })

  it('round-trips through the parser', () => {
    const parsed = parseServerPath('webdav://test_user_1@testnas.local:5006/dav/files')
    if (!parsed) throw new Error('the parser refused a path it minted')
    expect(constructServerPath(parsed, parsed.path)).toBe('webdav://test_user_1@testnas.local:5006/dav/files')
  })

  it('answers the bare root for the three root spellings', () => {
    const account = { protocol: 'sftp' as const, username: 'test_user_1', host: 'testnas.local', port: 22 }
    expect(constructServerPath(account, '')).toBe('sftp://test_user_1@testnas.local:22')
    expect(constructServerPath(account, '/')).toBe('sftp://test_user_1@testnas.local:22')
    expect(serverAppRoot(account)).toBe('sftp://test_user_1@testnas.local:22')
  })
})

describe('isServerPath / isServerVolumeId', () => {
  it('recognizes the three schemes and nothing else', () => {
    expect(isServerPath('sftp://test_user_1@testnas.local:22/srv')).toBe(true)
    expect(isServerPath('s3://AKIAEXAMPLE@127.0.0.1:14480/bucket')).toBe(true)
    expect(isServerPath('s3:/not-a-path')).toBe(false)
    expect(isServerPath('webdav://test_user_1@testnas.local:5006/')).toBe(true)
    expect(isServerPath('smb://testnas')).toBe(false)
    expect(isServerPath('/Volumes/testshare')).toBe(false)
  })

  it('recognizes the ids the two volume-id minters produce', () => {
    expect(isServerVolumeId('sftp-nas-local-22-test_user_1')).toBe(true)
    expect(isServerVolumeId('webdav-nas-local-5006-test_user_1')).toBe(true)
    expect(isServerVolumeId('s3-127-0-0-1-14480-akiaexample-photos-1a2b')).toBe(true)
    expect(serverProtocolOfVolumeId('s3-127-0-0-1-14480-akiaexample-1a2b')).toBe('s3')
    expect(isServerVolumeId('root')).toBe(false)
    expect(isServerVolumeId('adb-pixel-9a3f')).toBe(false)
  })
})

describe('isUnderServerRoot', () => {
  it('matches by whole components, never a string prefix', () => {
    expect(isUnderServerRoot('sftp://test_user_1@nas:22/srv/data', 'sftp://test_user_1@nas:22/srv/data')).toBe(true)
    expect(isUnderServerRoot('sftp://test_user_1@nas:22/srv/data', 'sftp://test_user_1@nas:22/srv/data/x')).toBe(true)
    expect(isUnderServerRoot('sftp://test_user_1@nas:22/srv/data', 'sftp://test_user_1@nas:22/srv/data-1')).toBe(false)
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
    expect(getServerParentPath('sftp://test_user_1@testnas.local:22/srv/data/photos')).toBe(
      'sftp://test_user_1@testnas.local:22/srv/data',
    )
    expect(getServerParentPath('sftp://test_user_1@testnas.local:22/srv')).toBe('sftp://test_user_1@testnas.local:22')
    // ❗ Never above the root: the parent of the root is the root, never `/`.
    expect(getServerParentPath('sftp://test_user_1@testnas.local:22')).toBeNull()
    expect(getServerParentPath('/srv/data')).toBeNull()
  })

  it('joins a child name onto a folder', () => {
    expect(joinServerPath('sftp://test_user_1@testnas.local:22/srv', 'data')).toBe(
      'sftp://test_user_1@testnas.local:22/srv/data',
    )
    expect(joinServerPath('sftp://test_user_1@testnas.local:22', 'srv')).toBe('sftp://test_user_1@testnas.local:22/srv')
    expect(joinServerPath('/local/dir', 'child')).toBe('/local/dir')
  })

  it('walks and joins under an account that is an email address', () => {
    const root = 'webdav://test_user_1@example.com@cloud.example.com:443'
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
    expect(getServerDisplayPath('sftp://test_user_1@testnas.local:22/srv/data')).toBe('/srv/data')
    expect(getServerDisplayPath('sftp://test_user_1@testnas.local:22')).toBe('/')
    expect(getServerDisplayPath('/Volumes/testshare')).toBe('/Volumes/testshare')
  })
})

describe('serverAddressOf', () => {
  it('names the host and port a server path dials, for the pane that waits on it (ERR-VZ4CV)', () => {
    expect(serverAddressOf('sftp://test_user_1@100.64.0.7:22/srv/data')).toBe('100.64.0.7:22')
    expect(serverAddressOf('webdav://test_user_1@testnas.local:5006')).toBe('testnas.local:5006')
  })

  it('brackets an IPv6 literal so its port reads as a port', () => {
    expect(serverAddressOf('sftp://test_user_1@fd7a:115c::5:22')).toBe('[fd7a:115c::5]:22')
  })

  it('answers null off the server schemes', () => {
    expect(serverAddressOf('/Volumes/testshare')).toBeNull()
  })
})

/**
 * NFS: the export rides as ONE escaped segment, so a path alone names its export
 * even beside a nested one (`/share` and `/share/Public`). Every row below is one
 * of `crates/cmdr-fs/src/volume/remote_paths_test.rs`'s NFS cells, so the two
 * halves of the grammar answer the same examples.
 */
describe('NFS paths', () => {
  it('mints the app root with the export as one segment, as `nfs_app_root` does', () => {
    expect(nfsAppRoot({ host: 'TESTNAS.local', port: 2049, export: '/Public' })).toBe('nfs://testnas.local:2049/Public')
    expect(nfsAppRoot({ host: 'testnas.local', port: 2049, export: '/volume1/data/' })).toBe(
      'nfs://testnas.local:2049/volume1%2Fdata',
    )
    expect(nfsAppRoot({ host: 'testnas.local', port: 2049, export: '/' })).toBe('nfs://testnas.local:2049/%2F')
    expect(nfsAppRoot({ host: 'testnas.local', port: 2049, export: '/100%/x' })).toBe(
      'nfs://testnas.local:2049/100%25%2Fx',
    )
  })

  it('canonicalizes the export before escaping it, the way `canonical_nfs_export` does', () => {
    const root = (exportPath: string) => nfsAppRoot({ host: 'nas', port: 2049, export: exportPath })
    expect(root('/volume1//./data/')).toBe(root('/volume1/data'))
    expect(root('///')).toBe('nfs://nas:2049/%2F')
    expect(root('Public')).toBe('nfs://nas:2049/Public')
  })

  it('carries the canonical port, 2049 when the address named none', () => {
    expect(nfsAppRoot({ host: 'nas', port: null, export: '/Public' })).toBe('nfs://nas:2049/Public')
    expect(nfsAppRoot({ host: 'nas', port: 15481, export: '/Public' })).toBe('nfs://nas:15481/Public')
  })

  it('reads the export and the folder under it back out of an app path', () => {
    for (const [path, exportPath, folder] of [
      ['nfs://testnas.local:2049/Public/photos/a.jpg', '/Public', 'photos/a.jpg'],
      ['nfs://testnas.local:2049/Public', '/Public', ''],
      ['nfs://testnas.local:2049/volume1%2Fdata/photos', '/volume1/data', 'photos'],
      ['nfs://testnas.local:2049/%2F/etc', '/', 'etc'],
      ['nfs://testnas.local:2049/100%25%2Fx', '/100%/x', ''],
    ]) {
      expect(parseServerPath(path), path).toEqual({
        protocol: 'nfs',
        host: 'testnas.local',
        port: 2049,
        export: exportPath,
        path: folder,
      })
    }
  })

  it('reads an IPv6 host by the LAST colon, as `server_of_path` does', () => {
    expect(parseServerPath('nfs://fd7a::5:2049/Public')).toMatchObject({ host: 'fd7a::5', port: 2049 })
  })

  it('names nothing without an export or a port, or with an escape no root mints', () => {
    for (const path of [
      'nfs://testnas.local:2049',
      'nfs://testnas.local:2049/',
      'nfs://testnas.local/Public',
      'nfs://:2049/Public',
      'nfs://testnas.local:2049/a%2',
      'nfs://testnas.local:2049/a%zz',
      'nfs://testnas.local:2049/a%2f',
    ]) {
      expect(parseServerPath(path), path).toBeNull()
    }
  })

  it('round-trips every export through its app root', () => {
    for (const exportPath of ['/Public', '/volume1/data', '/', '/Caf\u00e9 & co', '/100%/x']) {
      const root = nfsAppRoot({ host: 'Testnas.Local', port: 15481, export: exportPath })
      expect(parseServerPath(`${root}/sub/file`), exportPath).toMatchObject({ export: exportPath, path: 'sub/file' })
      expect(serverAppRoot({ protocol: 'nfs', host: 'testnas.local', port: 15481, export: exportPath })).toBe(root)
    }
  })

  it('walks up to the export and no further, and joins under it', () => {
    const root = 'nfs://nas:2049/volume1%2Fdata'
    expect(getServerParentPath(`${root}/photos/2026`)).toBe(`${root}/photos`)
    expect(getServerParentPath(`${root}/photos`)).toBe(root)
    expect(getServerParentPath(root)).toBeNull()
    expect(joinServerPath(root, 'photos')).toBe(`${root}/photos`)
  })

  it('shows the export decoded, then the folder, which is what someone on the server reads', () => {
    expect(getServerDisplayPath('nfs://nas:2049/volume1%2Fdata/photos')).toBe('/volume1/data/photos')
    expect(getServerDisplayPath('nfs://nas:2049/volume1%2Fdata')).toBe('/volume1/data')
    expect(getServerDisplayPath('nfs://nas:2049/%2F/etc')).toBe('/etc')
    expect(getServerDisplayPath('nfs://nas:2049/%2F')).toBe('/')
  })

  it('is a server path with a server id, and names the address it dials', () => {
    expect(isServerPath('nfs://nas:2049/Public')).toBe(true)
    expect(isServerVolumeId('nfs-public-testnas-1a2b')).toBe(true)
    expect(serverProtocolOfVolumeId('nfs-public-testnas-1a2b')).toBe('nfs')
    expect(serverAddressOf('nfs://testnas.local:2049/Public/x')).toBe('testnas.local:2049')
  })

  it('holds each export apart from its parent by whole components', () => {
    expect(isUnderServerRoot('nfs://nas:2049/share', 'nfs://nas:2049/share%2FPublic/x')).toBe(false)
    expect(isUnderServerRoot('nfs://nas:2049/share', 'nfs://nas:2049/share/Public/x')).toBe(true)
  })
})
