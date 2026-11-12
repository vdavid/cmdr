/**
 * The example table behind add mode's address field.
 *
 * People paste what they have: an `ssh` line off a wiki, a `user@host` from a
 * colleague, a Nextcloud URL out of a browser bar, an `smb://` off a Finder
 * dialog. Every one of those has to land on an endpoint, and a shape nobody
 * recognizes has to say so rather than guess (a guess sends a password to the
 * wrong port).
 *
 * ❗ The protocol is the TOGGLE's, never the address's. The parser reports a
 * protocol only where the string spells one out (a scheme, an `ssh` line), and
 * the sheet uses that for a warning and nothing else.
 *
 * ❗ There is no property-testing library on the frontend, so this table IS the
 * contract. A shape that reaches the field and isn't here is a shape nobody
 * decided.
 */
import { describe, expect, it } from 'vitest'
import { addressLooksLike, parseServerAddress } from './address-parser'

describe('parseServerAddress: the SFTP shapes', () => {
  it('reads a full sftp URL', () => {
    expect(parseServerAddress('sftp://test_user_1@testnas.local:2222/srv/data')).toEqual({
      kind: 'parsed',
      protocol: 'sftp',
      host: 'testnas.local',
      port: 2222,
      username: 'test_user_1',
      path: '/srv/data',
    })
  })

  it('falls back to port 22 and no account', () => {
    expect(parseServerAddress('sftp://testnas.local')).toEqual({
      kind: 'parsed',
      protocol: 'sftp',
      host: 'testnas.local',
      port: 22,
      username: undefined,
      path: undefined,
    })
  })

  it('reads `ssh://` as the same thing', () => {
    const parsed = parseServerAddress('ssh://test_user_1@testnas.local:2222/srv')
    expect(parsed).toMatchObject({ protocol: 'sftp', port: 2222, username: 'test_user_1', path: '/srv' })
  })

  it('reads a pasted `ssh` command line, with and without its port flag', () => {
    expect(parseServerAddress('ssh test_user_1@testnas.local')).toMatchObject({
      protocol: 'sftp',
      host: 'testnas.local',
      port: 22,
      username: 'test_user_1',
    })
    expect(parseServerAddress('ssh -p 2222 test_user_1@testnas.local')).toMatchObject({
      protocol: 'sftp',
      port: 2222,
      username: 'test_user_1',
    })
    expect(parseServerAddress('ssh -p2222 test_user_1@testnas.local')).toMatchObject({ port: 2222 })
  })
})

describe('parseServerAddress: no scheme names no protocol', () => {
  /**
   * ❗ `user@host` with no scheme names an account and a machine, and NOTHING
   * about the protocol. It is SMB's natural spelling for a NAS share that needs
   * a user just as much as it is SFTP's. Reading it as SFTP once flipped the
   * toggle on a person typing `test_user_6@192.168.50.20` for their SMB NAS and dialed
   * SSH on port 22 without them ever clicking SFTP (cmdr-reports#8). So it
   * answers no protocol and no port: the toggle, which the person set, decides.
   */
  it('reads a bare `user@host` as an account on a host, with no protocol and no port', () => {
    expect(parseServerAddress('test_user_6@192.168.50.20')).toEqual({
      kind: 'parsed',
      host: '192.168.50.20',
      username: 'test_user_6',
    })
    expect(parseServerAddress('test_user_1@testnas.local')).not.toHaveProperty('protocol')
  })

  it('reads a bare hostname with no protocol and no port', () => {
    // Which protocol a NAS name off a sticker speaks is the toggle's call.
    expect(parseServerAddress('testnas')).toEqual({ kind: 'parsed', host: 'testnas' })
    expect(parseServerAddress('192.168.50.10')).toEqual({ kind: 'parsed', host: '192.168.50.10' })
  })

  it('keeps a port and a path the address named, for whichever protocol the toggle says', () => {
    expect(parseServerAddress('testnas.local:1445')).toEqual({
      kind: 'parsed',
      host: 'testnas.local',
      port: 1445,
    })
    expect(parseServerAddress('test_user_1@testnas/media')).toEqual({
      kind: 'parsed',
      host: 'testnas',
      username: 'test_user_1',
      path: '/media',
    })
  })

  it('reads `user@host:port` as a port, and `user@host:/path` as scp syntax', () => {
    expect(parseServerAddress('test_user_1@testnas.local:2222')).toMatchObject({ port: 2222 })
    expect(parseServerAddress('test_user_1@testnas.local:2222')).not.toHaveProperty('path')
    const scp = parseServerAddress('test_user_1@testnas.local:/srv/data')
    expect(scp).toMatchObject({ path: '/srv/data' })
    expect(scp).not.toHaveProperty('port')
  })
})

describe('parseServerAddress: the WebDAV shapes', () => {
  it('reads a bare https origin as WebDAV on 443', () => {
    expect(parseServerAddress('https://nas:5006/')).toEqual({
      kind: 'parsed',
      protocol: 'webdav',
      host: 'nas',
      port: 5006,
      username: undefined,
      path: undefined,
      secure: true,
    })
    expect(parseServerAddress('https://cloud.example.com')).toMatchObject({
      protocol: 'webdav',
      port: 443,
      secure: true,
    })
  })

  it('keeps a Nextcloud URL whole, path and all', () => {
    // ❗ Nobody can tell where the base URL ends and the collection begins, and
    // the backend resolves the remote root relative to the base anyway. So the
    // whole path stays with the address and the remote folder starts at the root.
    expect(parseServerAddress('https://cloud.example.com/remote.php/dav/files/test_user_1/')).toMatchObject({
      protocol: 'webdav',
      host: 'cloud.example.com',
      port: 443,
      path: '/remote.php/dav/files/test_user_1',
    })
  })

  it('reads plain http, and says it is not secure', () => {
    expect(parseServerAddress('http://nas:8080/dav')).toMatchObject({
      protocol: 'webdav',
      port: 8080,
      secure: false,
      path: '/dav',
    })
  })

  it('reads the four WebDAV schemes, `davs` secure and `dav` not', () => {
    expect(parseServerAddress('webdav://test_user_1@nas:5006/dav')).toMatchObject({
      protocol: 'webdav',
      username: 'test_user_1',
      port: 5006,
      secure: true,
    })
    expect(parseServerAddress('davs://nas/dav')).toMatchObject({ port: 443, secure: true })
    expect(parseServerAddress('dav://nas/dav')).toMatchObject({ port: 80, secure: false })
  })
})

describe('parseServerAddress: the SMB shapes', () => {
  it('reads an `smb://` address, account and share and all', () => {
    expect(parseServerAddress('smb://testnas')).toEqual({
      kind: 'parsed',
      protocol: 'smb',
      host: 'testnas',
      port: 445,
      username: undefined,
      path: undefined,
    })
    expect(parseServerAddress('smb://test_user_1@testnas/media')).toMatchObject({
      protocol: 'smb',
      host: 'testnas',
      username: 'test_user_1',
      path: '/media',
    })
    expect(parseServerAddress('cifs://testnas')).toMatchObject({ protocol: 'smb', port: 445 })
  })

  /** A UNC path off Windows or a colleague's email is SMB spelled with backslashes (QA round 2). */
  it('reads a UNC path as SMB, share and all', () => {
    expect(parseServerAddress('\\\\nas\\share\\docs')).toMatchObject({
      kind: 'parsed',
      protocol: 'smb',
      host: 'nas',
      port: 445,
      path: '/share/docs',
    })
    expect(parseServerAddress('\\\\nas')).toMatchObject({ protocol: 'smb', host: 'nas' })
  })

  /** `//nas/share` is how the macOS mount table (`mount`, Get Info) prints an SMB share (QA round 3). */
  it('reads the mount table`s //host/share as SMB', () => {
    expect(parseServerAddress('//nas/share')).toMatchObject({
      kind: 'parsed',
      protocol: 'smb',
      host: 'nas',
      port: 445,
      path: '/share',
    })
    expect(parseServerAddress('//testuser@localhost:11480/public')).toMatchObject({
      protocol: 'smb',
      host: 'localhost',
      port: 11480,
      username: 'testuser',
    })
  })
})

/**
 * NFS: the server, an optional port, and a path that is the export, or an export
 * and a folder under it. ❗ Where the export ends and the folder begins is
 * unknowable from text (QNAP nests `/share` and `/share/Public`), so the path stays
 * whole, and the backend splits it against the server's own export list.
 */
describe('parseServerAddress: the NFS shapes', () => {
  it('reads an `nfs://` URL, export and folder as one path, with no port when it names none', () => {
    expect(parseServerAddress('nfs://testnas.local/volume1/data/photos')).toEqual({
      kind: 'parsed',
      protocol: 'nfs',
      host: 'testnas.local',
      path: '/volume1/data/photos',
    })
  })

  it('keeps a port the URL named: it is the NFS port, and asking portmap is skipped', () => {
    expect(parseServerAddress('nfs://testnas.local:15481/insecure')).toMatchObject({
      protocol: 'nfs',
      port: 15481,
      path: '/insecure',
    })
  })

  it('reads a bare `nfs://host`, an export still to pick', () => {
    expect(parseServerAddress('nfs://TESTNAS.local/')).toEqual({
      kind: 'parsed',
      protocol: 'nfs',
      host: 'testnas.local',
    })
  })

  it('reads the classic `host:/export/path` as a host and a path, naming no protocol', () => {
    // ❗ Not evidence of NFS: `host:/path` is scp's spelling too, and an SFTP root
    // typed that way must keep landing in the root folder.
    expect(parseServerAddress('testnas.local:/volume1/data')).toEqual({
      kind: 'parsed',
      host: 'testnas.local',
      path: '/volume1/data',
    })
  })

  it('splits the classic spelling at the FIRST `:/`, so a folder may hold a colon', () => {
    expect(parseServerAddress('testnas.local:/share/10:30 meeting')).toMatchObject({
      host: 'testnas.local',
      path: '/share/10:30 meeting',
    })
  })

  it.each([
    ['[fe80::1]:/share', 'an IPv6 literal, refused as for every protocol'],
    ['nfs://[fe80::1]/share', 'and in a URL'],
    ['nfs://nas:0/share', 'port zero'],
  ])('refuses %j (%s)', (input) => {
    expect(parseServerAddress(input)).toEqual({ kind: 'unparsed' })
  })
})

describe('parseServerAddress: folding and refusing', () => {
  it('folds the host to lowercase and leaves the account alone', () => {
    // The same fold `cmdr_fs::volume::ids` performs: DNS is case-insensitive, a
    // POSIX account is not, and `Test_user_1` and `test_user_1` can be two people.
    expect(parseServerAddress('SFTP://Test_user_1@TESTNAS.Local:22')).toMatchObject({
      protocol: 'sftp',
      host: 'testnas.local',
      username: 'Test_user_1',
    })
  })

  it('ignores whitespace around what was pasted', () => {
    expect(parseServerAddress('  sftp://testnas.local  ')).toMatchObject({ host: 'testnas.local' })
  })

  /**
   * ❗ A pasted URL may carry `user:password@`. The password must never reach the
   * Username field, the volume id, or the saved store: all three are plain text
   * a person reads, and the id is what the Keychain scope is keyed on.
   */
  it('takes the account out of a URL that carries a password, and drops the password', () => {
    expect(parseServerAddress('https://test_user_1:hunter2@testnas.local/dav')).toMatchObject({
      protocol: 'webdav',
      host: 'testnas.local',
      username: 'test_user_1',
    })
  })

  it('refuses a URL whose userinfo is a password with no account', () => {
    expect(parseServerAddress('sftp://:hunter2@testnas.local')).toEqual({ kind: 'unparsed' })
  })

  it.each([
    ['', 'nothing typed yet'],
    ['   ', 'whitespace only'],
    ['not a server!!', 'punctuation no host can carry'],
    ['ftp://testnas.local', 'a protocol Cmdr does not speak'],
    ['sftp://', 'a scheme with no host'],
    ['test_user_1@', 'an account with no host'],
    ['sftp://testnas.local:99999', 'a port outside the range'],
    ['sftp://testnas.local:0', 'port zero'],
    ['sftp://[2001:db8::1]:2222', 'an IPv6 literal, which no remote path can spell yet'],
    ['/srv/data', 'a bare server-absolute path, which names no server at all'],
  ])('refuses %j (%s)', (input) => {
    expect(parseServerAddress(input)).toEqual({ kind: 'unparsed' })
  })
})

/**
 * The warning under the address field: which protocol the address looks like,
 * when that isn't the one selected. It never moves the toggle; the person reads
 * it and decides.
 *
 * ❗ Only two things count as evidence. A scheme (or an `ssh` line) is the
 * address saying outright which protocol it is. A well-known port on an address
 * with no scheme is strong enough to mention, since `nas:22` with SMB selected
 * is almost certainly a mix-up. Nothing else is: an account, a path, or a bare
 * host fits all three protocols.
 */
describe('addressLooksLike', () => {
  it.each([
    ['sftp://test_user_1@testnas.local', 'smb', 'sftp', 'an sftp URL'],
    ['ssh://testnas.local', 'webdav', 'sftp', 'an ssh URL'],
    ['ssh -p 2222 test_user_1@testnas.local', 'smb', 'sftp', 'a pasted ssh line'],
    ['smb://test_user_6@192.168.50.20/test_user_6', 'sftp', 'smb', 'an smb URL'],
    ['cifs://nas', 'webdav', 'smb', 'a cifs URL'],
    ['https://cloud.example.com/remote.php/dav', 'smb', 'webdav', 'a web address'],
    ['davs://nas/dav', 'sftp', 'webdav', 'a davs URL'],
    ['testnas.local:22', 'smb', 'sftp', "SSH's port on an address with no scheme"],
    ['test_user_1@testnas.local:445', 'sftp', 'smb', "SMB's port"],
    ['testnas.local:139', 'webdav', 'smb', "NetBIOS SMB's port"],
    ['testnas.local:443', 'sftp', 'webdav', "HTTPS's port"],
    ['testnas.local:80', 'smb', 'webdav', "HTTP's port"],
    ['nfs://testnas.local/share', 'smb', 'nfs', 'an nfs URL'],
    ['testnas.local:2049', 'smb', 'nfs', "NFS's port"],
  ] as const)('with %j and %s selected, says it looks like %s (%s)', (input, selected, looksLike, _why) => {
    expect(addressLooksLike(input, selected)).toBe(looksLike)
  })

  it.each([
    ['test_user_6@192.168.50.20', 'smb', 'the issue #8 shape: `user@host` is a fine SMB address'],
    ['test_user_6@192.168.50.20', 'sftp', 'and a fine SFTP one'],
    ['test_user_6@192.168.50.20', 'webdav', 'and a fine WebDAV one'],
    ['testnas', 'sftp', 'a bare host fits every protocol'],
    ['test_user_1@testnas.local/srv/data', 'webdav', 'so does a path'],
    ['testnas.local:2222', 'smb', 'a port nobody owns'],
    ['sftp://testnas.local', 'sftp', 'a scheme that agrees'],
    ['https://nas:22/dav', 'webdav', 'a scheme that agrees wins over the port it names'],
    ['', 'smb', 'nothing typed'],
    ['test_user_1@', 'sftp', 'a half-typed address, which is the normal state of a field being typed into'],
    ['ftp://testnas.local', 'smb', 'a protocol Cmdr does not speak, which Connect refuses on its own'],
    ['testnas.local:/share', 'sftp', 'the classic NFS spelling, which is scp spelling too'],
    ['nfs://testnas.local/share', 'nfs', 'an nfs URL with NFS selected'],
  ] as const)('says nothing about %j with %s selected (%s)', (input, selected, _why) => {
    expect(addressLooksLike(input, selected)).toBeNull()
  })
})
