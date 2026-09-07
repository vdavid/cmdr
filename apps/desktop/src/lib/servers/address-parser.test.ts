/**
 * The example table behind add mode's address field.
 *
 * People paste what they have: an `ssh` line off a wiki, a `user@host` from a
 * colleague, a Nextcloud URL out of a browser bar, an `smb://` off a Finder
 * dialog. Every one of those has to land on a protocol and an endpoint, and a
 * shape nobody recognizes has to say so rather than guess (a guess sends a
 * password to the wrong port).
 *
 * ❗ There is no property-testing library on the frontend, so this table IS the
 * contract. A shape that reaches the field and isn't here is a shape nobody
 * decided.
 */
import { describe, expect, it } from 'vitest'
import { parseServerAddress } from './address-parser'

describe('parseServerAddress: the SFTP shapes', () => {
  it('reads a full sftp URL', () => {
    expect(parseServerAddress('sftp://ada@nas.local:2222/srv/data')).toEqual({
      kind: 'parsed',
      protocol: 'sftp',
      host: 'nas.local',
      port: 2222,
      username: 'ada',
      path: '/srv/data',
    })
  })

  it('falls back to port 22 and no account', () => {
    expect(parseServerAddress('sftp://nas.local')).toEqual({
      kind: 'parsed',
      protocol: 'sftp',
      host: 'nas.local',
      port: 22,
      username: undefined,
      path: undefined,
    })
  })

  it('reads `ssh://` as the same thing', () => {
    const parsed = parseServerAddress('ssh://ada@nas.local:2222/srv')
    expect(parsed).toMatchObject({ protocol: 'sftp', port: 2222, username: 'ada', path: '/srv' })
  })

  it('reads a pasted `ssh` command line, with and without its port flag', () => {
    expect(parseServerAddress('ssh ada@nas.local')).toMatchObject({
      protocol: 'sftp',
      host: 'nas.local',
      port: 22,
      username: 'ada',
    })
    expect(parseServerAddress('ssh -p 2222 ada@nas.local')).toMatchObject({
      protocol: 'sftp',
      port: 2222,
      username: 'ada',
    })
    expect(parseServerAddress('ssh -p2222 ada@nas.local')).toMatchObject({ port: 2222 })
  })

  it('reads a bare `user@host`, because an account is what `user@` means', () => {
    expect(parseServerAddress('ada@nas.local')).toMatchObject({
      protocol: 'sftp',
      host: 'nas.local',
      port: 22,
      username: 'ada',
    })
  })

  it('reads `user@host:port` as a port, and `user@host:/path` as scp syntax', () => {
    expect(parseServerAddress('ada@nas.local:2222')).toMatchObject({ port: 2222 })
    expect(parseServerAddress('ada@nas.local:2222')).not.toHaveProperty('path')
    expect(parseServerAddress('ada@nas.local:/srv/data')).toMatchObject({
      port: 22,
      path: '/srv/data',
    })
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
    expect(parseServerAddress('https://cloud.example.com/remote.php/dav/files/ada/')).toMatchObject({
      protocol: 'webdav',
      host: 'cloud.example.com',
      port: 443,
      path: '/remote.php/dav/files/ada',
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
    expect(parseServerAddress('webdav://ada@nas:5006/dav')).toMatchObject({
      protocol: 'webdav',
      username: 'ada',
      port: 5006,
      secure: true,
    })
    expect(parseServerAddress('davs://nas/dav')).toMatchObject({ port: 443, secure: true })
    expect(parseServerAddress('dav://nas/dav')).toMatchObject({ port: 80, secure: false })
  })
})

describe('parseServerAddress: the SMB shapes', () => {
  it('reads an `smb://` address, account and share and all', () => {
    expect(parseServerAddress('smb://naspolya')).toEqual({
      kind: 'parsed',
      protocol: 'smb',
      host: 'naspolya',
      port: 445,
      username: undefined,
      path: undefined,
    })
    expect(parseServerAddress('smb://ada@naspolya/media')).toMatchObject({
      protocol: 'smb',
      host: 'naspolya',
      username: 'ada',
      path: '/media',
    })
  })

  it('reads a bare hostname as SMB', () => {
    // ❗ The one guess that costs nothing: SMB is the only protocol of the three
    // that browses with no account, so a wrong guess asks the user for nothing.
    // Guessing SFTP would put an account field in front of someone who typed a
    // NAS name off a sticker.
    expect(parseServerAddress('naspolya')).toMatchObject({
      protocol: 'smb',
      host: 'naspolya',
      port: 445,
    })
    expect(parseServerAddress('192.168.1.111')).toMatchObject({ protocol: 'smb', host: '192.168.1.111' })
    expect(parseServerAddress('naspolya.local:1445')).toMatchObject({ protocol: 'smb', port: 1445 })
  })
})

describe('parseServerAddress: folding and refusing', () => {
  it('folds the host to lowercase and leaves the account alone', () => {
    // The same fold `cmdr_fs::volume::ids` performs: DNS is case-insensitive, a
    // POSIX account is not, and `Ada` and `ada` can be two people.
    expect(parseServerAddress('SFTP://Ada@NAS.Local:22')).toMatchObject({
      protocol: 'sftp',
      host: 'nas.local',
      username: 'Ada',
    })
  })

  it('ignores whitespace around what was pasted', () => {
    expect(parseServerAddress('  sftp://nas.local  ')).toMatchObject({ host: 'nas.local' })
  })

  /**
   * ❗ A pasted URL may carry `user:password@`. The password must never reach the
   * Username field, the volume id, or the saved store: all three are plain text
   * a person reads, and the id is what the Keychain scope is keyed on.
   */
  it('takes the account out of a URL that carries a password, and drops the password', () => {
    expect(parseServerAddress('https://ada:hunter2@nas.local/dav')).toMatchObject({
      protocol: 'webdav',
      host: 'nas.local',
      username: 'ada',
    })
  })

  it('refuses a URL whose userinfo is a password with no account', () => {
    expect(parseServerAddress('sftp://:hunter2@nas.local')).toEqual({ kind: 'unparsed' })
  })

  it.each([
    ['', 'nothing typed yet'],
    ['   ', 'whitespace only'],
    ['not a server!!', 'punctuation no host can carry'],
    ['ftp://nas.local', 'a protocol Cmdr does not speak'],
    ['sftp://', 'a scheme with no host'],
    ['ada@', 'an account with no host'],
    ['sftp://nas.local:99999', 'a port outside the range'],
    ['sftp://nas.local:0', 'port zero'],
    ['sftp://[2001:db8::1]:2222', 'an IPv6 literal, which no remote path can spell yet'],
    ['/srv/data', 'a bare server-absolute path, which names no server at all'],
  ])('refuses %j (%s)', (input) => {
    expect(parseServerAddress(input)).toEqual({ kind: 'unparsed' })
  })
})
