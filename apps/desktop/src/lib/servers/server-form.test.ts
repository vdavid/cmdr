/**
 * The add form's two translations: a typed address in, a dial target out.
 *
 * These are the rules a wrong reading turns into a password sent to the wrong
 * port, so they live here rather than inside the component.
 */

import { describe, expect, it } from 'vitest'
import { parseServerAddress } from './address-parser'
import {
  applyParsedAddress,
  emptyServerForm,
  formFromSftpServer,
  isStartFolderUnderRoot,
  nextcloudAddress,
  serverTargetFrom,
} from './server-form'

/** A form as the sheet would hold it after someone typed `address`. */
function typed(address: string) {
  const form = { ...emptyServerForm(), address }
  return applyParsedAddress(form, parseServerAddress(address))
}

describe('applyParsedAddress', () => {
  it('flips the protocol and picks up the account the address carried', () => {
    expect(typed('ada@nas.local:2222')).toMatchObject({ protocol: 'sftp', username: 'ada' })
    expect(typed('https://cloud.example.com')).toMatchObject({ protocol: 'webdav' })
    expect(typed('naspolya')).toMatchObject({ protocol: 'smb' })
  })

  it('leaves everything alone when the address says nothing yet', () => {
    // A half-typed address is the normal state of a field someone is typing
    // into, so it must not wipe what they already put in the other fields.
    const form = { ...emptyServerForm(), username: 'ada', protocol: 'sftp' as const }
    expect(applyParsedAddress(form, parseServerAddress('ada@'))).toEqual(form)
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

describe('serverTargetFrom', () => {
  it('builds an SFTP target from the address and the advanced fields', () => {
    const form = { ...typed('ada@nas.local:2222/srv/data'), keyFile: ' ~/.ssh/id_ed25519 ', useAgent: false }
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
    const form = { ...typed('https://cloud.example.com/remote.php/dav/files/ada/'), username: 'ada' }
    expect(serverTargetFrom(form)).toMatchObject({
      protocol: 'webdav',
      url: 'https://cloud.example.com/remote.php/dav/files/ada',
      username: 'ada',
      remoteRoot: '/',
    })
    expect(serverTargetFrom({ ...typed('http://nas:8080/dav'), username: 'ada' })).toMatchObject({
      url: 'http://nas:8080/dav',
    })
  })

  it('lets the toggle win over the address, without carrying the other protocol’s port over', () => {
    // ❗ The toggle stays editable exactly so someone can type a bare host and
    // say "that one is SFTP". Carrying SMB's 445 into an SFTP dial would open a
    // socket nothing answers SSH on.
    expect(serverTargetFrom({ ...typed('naspolya'), protocol: 'sftp' })).toMatchObject({
      protocol: 'sftp',
      host: 'naspolya',
      port: 22,
    })
    expect(serverTargetFrom({ ...typed('naspolya'), protocol: 'webdav' })).toMatchObject({
      url: 'https://naspolya',
    })
  })

  it('names no target for SMB or for an address that says nothing', () => {
    expect(serverTargetFrom(typed('naspolya'))).toBeNull()
    expect(serverTargetFrom(typed('not a server!!'))).toBeNull()
  })

  it('reads all three root spellings as the volume root', () => {
    for (const remoteRoot of ['', ' ', '.']) {
      expect(serverTargetFrom({ ...typed('ada@nas.local'), remoteRoot })).toMatchObject({ remoteRoot: '/' })
    }
  })

  it('keeps an empty name empty, so the server is called by its account and host', () => {
    // ❗ Pre-fix an empty name fell back to the whole typed address, path and
    // all, which left the edit sheet with a name that looked exactly like the
    // address and sent a person to widen the root through the wrong field.
    expect(serverTargetFrom(typed('sftp://david@192.168.1.111:22/share/naspi/tmp'))).toMatchObject({ displayName: '' })
    expect(serverTargetFrom({ ...typed('https://cloud.example.com/dav'), username: 'ada' })).toMatchObject({
      displayName: '',
    })
  })

  it('trims a typed name', () => {
    expect(serverTargetFrom({ ...typed('ada@nas.local'), displayName: '  Naspolya ' })).toMatchObject({
      displayName: 'Naspolya',
    })
  })

  it('carries a typed start folder trimmed, and none when the field is empty', () => {
    expect(serverTargetFrom({ ...typed('ada@nas.local/srv/data'), startFolder: ' /srv/data/photos ' })).toMatchObject(
      { remoteRoot: '/srv/data', startFolder: '/srv/data/photos' },
    )
    expect(serverTargetFrom({ ...typed('ada@nas.local/srv/data'), startFolder: '  ' })).toMatchObject({
      startFolder: null,
    })
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
