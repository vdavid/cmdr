/**
 * The add form's two translations: a typed address in, a dial target out.
 *
 * These are the rules a wrong reading turns into a password sent to the wrong
 * port, so they live here rather than inside the component.
 */

import { describe, expect, it } from 'vitest'
import type { SavedServer, ServerProtocol } from '$lib/ipc/bindings'
import { parseServerAddress } from './address-parser'
import {
  applyParsedAddress,
  emptyServerForm,
  formFromPrefill,
  formFromS3Place,
  formFromSftpServer,
  isStartFolderUnderRoot,
  nameFallbackOf,
  withSavedAccount,
  nextcloudAddress,
  serverTargetFrom,
  smbAddressFrom,
} from './server-form'

/** A form as the sheet would hold it after someone picked `protocol` and typed `address`. */
function typed(address: string, protocol: ServerProtocol = 'smb') {
  const form = { ...emptyServerForm(), protocol, address }
  return applyParsedAddress(form, parseServerAddress(address))
}

describe('applyParsedAddress', () => {
  /**
   * ❗ The toggle is the person's, and typing never moves it. `user@host` once
   * flipped it to SFTP under someone typing their SMB NAS's address, and the
   * sheet dialed SSH without them clicking SFTP (cmdr-reports#8).
   */
  it('never moves the protocol toggle, whatever the address says', () => {
    expect(typed('sven@192.168.0.153')).toMatchObject({ protocol: 'smb' })
    expect(typed('sftp://ada@nas.local')).toMatchObject({ protocol: 'smb' })
    expect(typed('https://cloud.example.com', 'sftp')).toMatchObject({ protocol: 'sftp' })
    expect(typed('smb://naspolya', 'webdav')).toMatchObject({ protocol: 'webdav' })
  })

  it('picks up the account the address carried', () => {
    expect(typed('ada@nas.local:2222', 'sftp')).toMatchObject({ username: 'ada' })
    // Kept on SMB too, where the field is hidden, so switching the toggle later
    // shows what was typed.
    expect(typed('ada@nas.local:2222')).toMatchObject({ username: 'ada' })
  })

  it('fills the SFTP root folder from a path, and only when the address means SFTP too', () => {
    expect(typed('ada@nas.local:/srv/data', 'sftp')).toMatchObject({ remoteRoot: '/srv/data' })
    expect(typed('sftp://ada@nas.local/srv/data', 'sftp')).toMatchObject({ remoteRoot: '/srv/data' })
    // An SMB path is a share, and another protocol's path is not this one's folder.
    expect(typed('ada@nas.local/media')).toMatchObject({ remoteRoot: '' })
    expect(typed('smb://nas.local/media', 'sftp')).toMatchObject({ remoteRoot: '' })
  })

  it('leaves everything alone when the address says nothing yet', () => {
    // A half-typed address is the normal state of a field someone is typing
    // into, so it must not wipe what they already put in the other fields.
    const form = { ...emptyServerForm(), username: 'ada', protocol: 'sftp' as const }
    expect(applyParsedAddress(form, parseServerAddress('ada@'))).toEqual(form)
  })

  /**
   * ❗ A username the ADDRESS filled follows the address: `smb://x@nas/share` changed
   * to `nas.local:2222` kept "x" (QA round 2, m6). One the person typed stays.
   */
  it('drops a username the address filled once the address stops naming it', () => {
    const filled = typed('smb://x@nas/share')
    expect(filled.username).toBe('x')
    const next = applyParsedAddress({ ...filled, address: 'nas.local:2222' }, parseServerAddress('nas.local:2222'))
    expect(next.username).toBe('')
  })

  /** ❗ Typed "typed", then `x@nas` filled "x", then `nas2` left it empty (QA round 3). */
  it('gives back the username the person typed once the address stops naming one', () => {
    const own = { ...emptyServerForm(), username: 'typed' }
    const filled = applyParsedAddress({ ...own, address: 'x@nas' }, parseServerAddress('x@nas'))
    expect(filled.username).toBe('x')
    const refilled = applyParsedAddress({ ...filled, address: 'y@nas' }, parseServerAddress('y@nas'))
    expect(refilled.username).toBe('y')
    const next = applyParsedAddress({ ...refilled, address: 'nas2' }, parseServerAddress('nas2'))
    expect(next.username).toBe('typed')
  })

  /** ❗ `\\sven@nas\share` edited toward something that doesn't parse kept "sven" (QA round 3). */
  it('drops a username the address filled once the address stops parsing', () => {
    const filled = typed('\\\\sven@nas\\share')
    expect(filled.username).toBe('sven')
    const next = applyParsedAddress({ ...filled, address: 'sven@' }, parseServerAddress('sven@'))
    expect(next.username).toBe('')
  })

  it('keeps a username the address did not carry', () => {
    const form = { ...emptyServerForm(), username: 'ada' }
    expect(applyParsedAddress(form, parseServerAddress('nas.local')).username).toBe('ada')
  })
})

describe('emptyServerForm', () => {
  it('starts Remember ON, because someone typing a password into a NEW server means to come back to it', () => {
    // ❗ Add mode is the one place the box proposes rather than reports: there is
    // no stored secret to report yet. Sign-in mode seeds from the store instead
    // (`open-sign-in.ts`), where a default-on box would seed one the user
    // already declined.
    expect(emptyServerForm().remember).toBe(true)
  })
})

describe('formFromPrefill', () => {
  it('opens on the protocol a pasted URL spells out, since that is what the person asked to open', () => {
    // Go to path hands over only addresses with a scheme, and opening a sheet on
    // `sftp://…` with SMB selected would make the person say it twice.
    expect(formFromPrefill('sftp://ada@nas.local/srv')).toMatchObject({
      protocol: 'sftp',
      address: 'sftp://ada@nas.local/srv',
      username: 'ada',
      remoteRoot: '/srv',
    })
    expect(formFromPrefill('https://cloud.example.com')).toMatchObject({ protocol: 'webdav' })
  })

  it('stays on the default for an address that names no protocol', () => {
    expect(formFromPrefill('ada@nas.local')).toMatchObject({ protocol: 'smb', username: 'ada' })
  })
})

describe('serverTargetFrom', () => {
  /** ❗ cmdr-reports#8: what gets dialed is the toggle's protocol, and the toggle is the person's. */
  it('never dials a protocol the person did not select', () => {
    // SMB is the default and has no target here (its connect is a share mount),
    // so an address that looks like SFTP still dials nothing over SSH.
    expect(serverTargetFrom(typed('sven@192.168.0.153'))).toBeNull()
    expect(serverTargetFrom(typed('sftp://ada@nas.local'))).toBeNull()
    expect(serverTargetFrom(typed('ssh ada@nas.local'))).toBeNull()
    expect(serverTargetFrom(typed('https://cloud.example.com', 'sftp'))).toMatchObject({ protocol: 'sftp' })
  })

  it('builds an SFTP target from the address and the advanced fields', () => {
    const form = {
      ...typed('ada@nas.local:2222/srv/data', 'sftp'),
      keyFile: ' ~/.ssh/id_ed25519 ',
      useAgent: false,
    }
    expect(serverTargetFrom(form)).toEqual({
      protocol: 'sftp',
      displayName: '',
      host: 'nas.local',
      port: 2222,
      username: 'ada',
      remoteRoot: '/srv/data',
      startFolder: null,
      keyFile: '~/.ssh/id_ed25519',
      useAgent: false,
      autoReconnect: true,
    })
  })

  it('builds a WebDAV target whose URL keeps the pasted path and drops a default port', () => {
    const form = { ...typed('https://cloud.example.com/remote.php/dav/files/ada/', 'webdav'), username: 'ada' }
    expect(serverTargetFrom(form)).toMatchObject({
      protocol: 'webdav',
      url: 'https://cloud.example.com/remote.php/dav/files/ada',
      username: 'ada',
      remoteRoot: '/',
    })
    expect(serverTargetFrom({ ...typed('http://nas:8080/dav', 'webdav'), username: 'ada' })).toMatchObject({
      url: 'http://nas:8080/dav',
    })
  })

  it('dials a port the address named with no scheme on whichever protocol is selected', () => {
    expect(serverTargetFrom(typed('ada@nas.local:2222', 'sftp'))).toMatchObject({ port: 2222 })
    expect(serverTargetFrom(typed('nas:5006/dav', 'webdav'))).toMatchObject({ url: 'https://nas:5006/dav' })
  })

  it('falls back to the protocol’s own port, and never carries another protocol’s over', () => {
    // ❗ A scheme's port belongs to that scheme: SMB's 445 in an SFTP dial opens a
    // socket nothing answers SSH on.
    expect(serverTargetFrom(typed('naspolya', 'sftp'))).toMatchObject({
      protocol: 'sftp',
      host: 'naspolya',
      port: 22,
    })
    expect(serverTargetFrom(typed('smb://naspolya:1445/media', 'sftp'))).toMatchObject({ port: 22 })
    expect(serverTargetFrom(typed('naspolya', 'webdav'))).toMatchObject({ url: 'https://naspolya' })
    expect(serverTargetFrom(typed('sftp://naspolya:2222/srv', 'webdav'))).toMatchObject({ url: 'https://naspolya' })
  })

  it('names no target for SMB or for an address that says nothing', () => {
    expect(serverTargetFrom(typed('naspolya'))).toBeNull()
    expect(serverTargetFrom(typed('not a server!!', 'sftp'))).toBeNull()
  })

  it('reads all three root spellings as the volume root', () => {
    for (const remoteRoot of ['', ' ', '.']) {
      expect(serverTargetFrom({ ...typed('ada@nas.local', 'sftp'), remoteRoot })).toMatchObject({
        remoteRoot: '/',
      })
    }
  })

  it('keeps an empty name empty, so the server is called by its account and host', () => {
    // ❗ Pre-fix an empty name fell back to the whole typed address, path and
    // all, which left the edit sheet with a name that looked exactly like the
    // address and sent a person to widen the root through the wrong field.
    expect(serverTargetFrom(typed('sftp://david@192.168.1.111:22/share/naspi/tmp', 'sftp'))).toMatchObject({
      displayName: '',
    })
    expect(serverTargetFrom({ ...typed('https://cloud.example.com/dav', 'webdav'), username: 'ada' })).toMatchObject({
      displayName: '',
    })
  })

  it('trims a typed name', () => {
    expect(serverTargetFrom({ ...typed('ada@nas.local', 'sftp'), displayName: '  Naspolya ' })).toMatchObject({
      displayName: 'Naspolya',
    })
  })

  it('carries a typed start folder trimmed, and none when the field is empty', () => {
    const form = typed('ada@nas.local/srv/data', 'sftp')
    expect(serverTargetFrom({ ...form, startFolder: ' /srv/data/photos ' })).toMatchObject({
      remoteRoot: '/srv/data',
      startFolder: '/srv/data/photos',
    })
    expect(serverTargetFrom({ ...form, startFolder: '  ' })).toMatchObject({ startFolder: null })
  })
})

/**
 * What SMB's add hands `connect_to_server`. The backend reads a bare host,
 * `host:port`, or an `smb://` URL, and refuses everything else.
 */
describe('smbAddressFrom', () => {
  it('turns a UNC path into the smb:// address the backend reads', () => {
    expect(smbAddressFrom('\\\\nas\\share')).toBe('smb://nas/share')
  })

  it('turns the macOS mount table`s `//nas/share` into the smb:// address it means', () => {
    expect(smbAddressFrom('//nas/share')).toBe('smb://nas/share')
    expect(smbAddressFrom('//testuser@localhost:11480/public')).toBe('smb://testuser@localhost:11480/public')
    // The Name field's placeholder names the host, as for any SMB address.
    expect(nameFallbackOf({ ...emptyServerForm(), address: '//nas/share' })).toBe('nas')
  })

  it('spells an address with no scheme as an SMB URL, so `user@host` and a share path reach the backend', () => {
    // ❗ cmdr-reports#8's shape: the backend's bare-host reader refuses the `@`,
    // so `sven@192.168.0.153` has to travel as the SMB URL it means.
    expect(smbAddressFrom('sven@192.168.0.153')).toBe('smb://sven@192.168.0.153')
    expect(smbAddressFrom('  naspolya:1445/media ')).toBe('smb://naspolya:1445/media')
  })

  it('passes an SMB URL through as typed', () => {
    expect(smbAddressFrom('smb://Ada@NAS/media')).toBe('smb://Ada@NAS/media')
  })

  it('keeps only the host of an address that names another protocol', () => {
    // SMB is selected, so SMB is what gets dialed; another scheme's port and
    // path mean nothing to it.
    expect(smbAddressFrom('sftp://ada@nas.local:2222/srv')).toBe('smb://nas.local')
    expect(smbAddressFrom('ssh -p 2222 ada@nas.local')).toBe('smb://nas.local')
  })

  it('leaves an address it can’t read to the backend, which says what is wrong with it', () => {
    expect(smbAddressFrom('not a server!!')).toBe('not a server!!')
    expect(smbAddressFrom('[2001:db8::1]')).toBe('[2001:db8::1]')
  })
})

describe('formFromSftpServer', () => {
  it('opens an unnamed server with an empty name field, never its label or address', () => {
    const form = formFromSftpServer({
      host: 'nas.local',
      port: 22,
      username: 'ada',
      displayName: '',
      remoteRoot: '/srv/data',
      startFolder: '/srv/data/photos',
      keyFile: null,
      useAgent: true,
      autoReconnect: true,
      pinned: true,
      lastConnectedAt: '2026-09-06T00:00:00Z',
    })
    expect(form.displayName).toBe('')
    expect(form.remoteRoot).toBe('/srv/data')
    expect(form.startFolder).toBe('/srv/data/photos')
  })
})

/**
 * The sheet's inline mirror of the backend's "at or under the root" rule
 * (`saved_server_fields::start_folder_under_root`). The backend stays
 * authoritative; this only answers before a round-trip.
 */
describe('isStartFolderUnderRoot', () => {
  it('accepts an empty start folder, which means the root', () => {
    expect(isStartFolderUnderRoot('/srv/data', '')).toBe(true)
    expect(isStartFolderUnderRoot('/srv/data', '   ')).toBe(true)
  })

  it('accepts the root itself and anything below it', () => {
    expect(isStartFolderUnderRoot('/srv/data', '/srv/data')).toBe(true)
    expect(isStartFolderUnderRoot('/srv/data', '/srv/data/photos/2024')).toBe(true)
    expect(isStartFolderUnderRoot('/srv/data/', '/srv/data/photos/')).toBe(true)
  })

  it('refuses a sibling that shares the root as a string prefix', () => {
    expect(isStartFolderUnderRoot('/srv/data', '/srv/data-1')).toBe(false)
    expect(isStartFolderUnderRoot('/srv/data', '/srv/database/x')).toBe(false)
  })

  it('refuses a folder above or beside the root', () => {
    expect(isStartFolderUnderRoot('/srv/data', '/srv')).toBe(false)
    expect(isStartFolderUnderRoot('/srv/data', '/home/ada')).toBe(false)
  })

  it('resolves `.` and `..` the way the backend does before comparing', () => {
    expect(isStartFolderUnderRoot('/srv/data', '/srv/data/../etc')).toBe(false)
    expect(isStartFolderUnderRoot('/srv/data', '/srv/./data/photos')).toBe(true)
    expect(isStartFolderUnderRoot('/srv/data/tmp/..', '/srv/data/photos')).toBe(true)
  })

  it('reads a relative path from `/`, the way the backend reads a relative root', () => {
    expect(isStartFolderUnderRoot('/srv/data', 'photos')).toBe(false)
    expect(isStartFolderUnderRoot('/srv/data', 'srv/data/photos')).toBe(true)
  })

  it('treats every root spelling of the server root as holding everything', () => {
    for (const root of ['', '.', '/']) {
      expect(isStartFolderUnderRoot(root, '/home/ada')).toBe(true)
    }
  })
})

describe('nextcloudAddress', () => {
  it('appends the collection path nobody knows', () => {
    expect(nextcloudAddress('https://cloud.example.com/', 'ada')).toBe(
      'https://cloud.example.com/remote.php/dav/files/ada/',
    )
  })

  it('changes nothing without an account, because the path is per-account', () => {
    expect(nextcloudAddress('https://cloud.example.com', '  ')).toBe('https://cloud.example.com')
  })
})

/**
 * ❗ Go to path or ⌘K opening the Add sheet on an address that's already saved
 * under a name the person gave it: the placeholder named the address, while the
 * hub calls it "My NAS" (QA round 4).
 */
describe('nameFallbackOf, for an address already saved', () => {
  const myNas: SavedServer = {
    id: 'manual-localhost-11482',
    protocol: 'smb',
    displayName: 'My NAS',
    nameSource: 'user',
    address: 'localhost:11482',
    username: null,
    pinned: false,
    lastConnectedAt: null,
    autoReconnect: null,
    places: [],
  }

  it('names the server by the name it is saved under', () => {
    const form = { ...emptyServerForm(), address: 'smb://LOCALHOST:11482/public' }
    expect(nameFallbackOf(form, [myNas])).toBe('My NAS')
  })

  it('keeps the address for another server on the same machine, or a name nobody chose', () => {
    expect(nameFallbackOf({ ...emptyServerForm(), address: 'localhost:11480' }, [myNas])).toBe('localhost:11480')
    const unnamed = { ...myNas, nameSource: 'fallback' as const, displayName: 'localhost:11482' }
    expect(nameFallbackOf({ ...emptyServerForm(), address: 'localhost:11482' }, [unnamed])).toBe('localhost:11482')
  })
})

/**
 * ❗ The account follows the name: an Add sheet opened on an address saved "as
 * testuser" showed the saved name but an empty Username (QA round 5).
 */
describe('withSavedAccount', () => {
  const saved: SavedServer = {
    id: 'manual-localhost-11481',
    protocol: 'smb',
    displayName: 'localhost:11481',
    nameSource: 'fallback',
    address: 'localhost:11481',
    username: 'testuser',
    pinned: false,
    lastConnectedAt: null,
    autoReconnect: null,
    places: [],
  }

  it('fills the account the address is saved with, as one the address supplied', () => {
    const form = withSavedAccount({ ...emptyServerForm(), address: 'smb://localhost:11481/private' }, [saved])
    expect(form.username).toBe('testuser')
    // It follows the address: another server's address takes it away again.
    const moved = applyParsedAddress({ ...form, address: 'localhost:11480' }, parseServerAddress('localhost:11480'))
    expect(moved.username).toBe('')
  })

  it('leaves an account the person or the address already named', () => {
    const typed = { ...emptyServerForm(), address: 'localhost:11481', username: 'ada' }
    expect(withSavedAccount(typed, [saved]).username).toBe('ada')
    const inAddress = typed_('smb://sven@localhost:11481/private')
    expect(withSavedAccount(inAddress, [saved]).username).toBe('sven')
  })
})

function typed_(address: string) {
  return applyParsedAddress({ ...emptyServerForm(), address }, parseServerAddress(address))
}

describe('the S3 form', () => {
  /** An S3 form as the sheet would hold it once someone picked a preset and typed a key. */
  function s3Form(s3: Partial<ReturnType<typeof emptyServerForm>['s3']>, username = 'AKIAEXAMPLE') {
    const form = emptyServerForm()
    return { ...form, protocol: 's3' as const, username, s3: { ...form.s3, ...s3 } }
  }

  it('names a target with no address at all: the preset makes the endpoint', () => {
    expect(serverTargetFrom({ ...s3Form({ provider: 'aws', region: 'eu-west-1' }), displayName: ' Photos ' })).toEqual({
      protocol: 's3',
      displayName: 'Photos',
      provider: { kind: 'aws', region: 'eu-west-1' },
      accessKeyId: 'AKIAEXAMPLE',
      bucket: null,
      autoReconnect: true,
    })
  })

  it('sends a typed bucket trimmed, and an empty one as the account root', () => {
    expect(serverTargetFrom(s3Form({ provider: 'hetzner', location: 'nbg1', bucket: ' photos ' }))).toMatchObject({
      bucket: 'photos',
    })
    expect(serverTargetFrom(s3Form({ provider: 'hetzner', location: 'nbg1', bucket: '  ' }))).toMatchObject({
      bucket: null,
    })
  })

  it('trims the access key ID, which is part of the identity', () => {
    expect(serverTargetFrom(s3Form({ provider: 'r2', accountId: 'abc' }, ' AKIA1 '))).toMatchObject({
      accessKeyId: 'AKIA1',
    })
  })

  it('opens a saved S3 place in the edit form with its preset, key, bucket, raw name, and switch', () => {
    const form = formFromS3Place({
      provider: { kind: 'other', endpoint: 'http://nas:9000', region: null, pathStyle: false },
      accessKeyId: 'AKIA1',
      bucket: 'photos',
      displayName: '',
      autoReconnect: false,
      pinned: true,
      volumeId: 's3-photos',
    })
    expect(form).toMatchObject({
      protocol: 's3',
      username: 'AKIA1',
      displayName: '',
      autoReconnect: false,
      remember: false,
    })
    expect(form.s3).toMatchObject({
      provider: 'other',
      endpoint: 'http://nas:9000',
      pathStyle: false,
      bucket: 'photos',
    })
    // The target it saves back is the place it opened on: same identity, so the save edits rather than duplicates.
    expect(serverTargetFrom(form)).toMatchObject({
      provider: { kind: 'other', endpoint: 'http://nas:9000', region: null, pathStyle: false },
      accessKeyId: 'AKIA1',
      bucket: 'photos',
    })
  })

  it('lets no address left over from another protocol fill the access key ID', () => {
    const sftp = typed('ada@nas.local', 'sftp')
    expect(sftp.username).toBe('ada')
    const switched = applyParsedAddress({ ...sftp, protocol: 's3' }, parseServerAddress(sftp.address))
    expect(switched.username).toBe('')
  })

  it('opens a pasted `s3://` path on S3, with the key, the preset, and the bucket filled in', () => {
    const form = formFromPrefill('s3://AKIAEXAMPLE@s3.eu-west-1.amazonaws.com:443/photos/2026')
    expect(form.protocol).toBe('s3')
    expect(form.username).toBe('AKIAEXAMPLE')
    expect(form.s3).toMatchObject({ provider: 'aws', region: 'eu-west-1', bucket: 'photos' })
  })

  it('promises the name the backend gives an unnamed ACCOUNT: the key at the endpoint host, whatever the bucket', () => {
    // The Name field names the account; a bucket reads as its own name under it.
    expect(nameFallbackOf(s3Form({ provider: 'aws', region: 'eu-west-1', bucket: 'photos' }))).toBe(
      'AKIAEXAMPLE@s3.eu-west-1.amazonaws.com',
    )
    expect(nameFallbackOf(s3Form({ provider: 'aws', region: 'eu-west-1' }))).toBe(
      'AKIAEXAMPLE@s3.eu-west-1.amazonaws.com',
    )
    expect(nameFallbackOf(s3Form({ provider: 'aws', region: '' }))).toBeNull()
  })
})
