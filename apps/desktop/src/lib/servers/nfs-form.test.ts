/**
 * The NFS half of the add form: the export, and the uid and gid every request acts as.
 *
 * ❗ The guess is a starting value, ❌ never the person's: a uid someone typed is
 * theirs, and a later guess (a new export picked) must not overwrite it.
 */

import { describe, expect, it } from 'vitest'
import { parseServerAddress } from './address-parser'
import {
  emptyNfsFields,
  nfsFieldsFromAppPath,
  nfsIdentityWarns,
  parseNfsId,
  readNfsOtherGids,
  withGuessedIdentity,
  withTypedIdentity,
} from './nfs-form'
import type { SavedNfsPlace } from '#lib/ipc/bindings.js'
import {
  applyParsedAddress,
  emptyServerForm,
  formFromNfsPlace,
  formFromPrefill,
  nameFallbackOf,
  serverTargetFrom,
} from './server-form'

describe('parseNfsId', () => {
  it.each([
    ['1000', 1000],
    ['0', 0],
    [' 501 ', 501],
    ['4294967295', 4294967295],
  ])('reads %j as %d', (text, id) => {
    expect(parseNfsId(text)).toBe(id)
  })

  it.each([[''], ['-1'], ['1.5'], ['1e3'], ['4294967296'], ['abc'], ['10 00']])('refuses %j', (text) => {
    expect(parseNfsId(text)).toBeNull()
  })
})

describe('readNfsOtherGids', () => {
  it('reads an empty field as no groups', () => {
    expect(readNfsOtherGids('')).toEqual({ kind: 'ids', ids: [] })
    expect(readNfsOtherGids('  , ')).toEqual({ kind: 'ids', ids: [] })
  })

  it('takes commas, spaces, or both, in the typed order, each id once', () => {
    expect(readNfsOtherGids('100, 2001 1000,2001')).toEqual({ kind: 'ids', ids: [100, 2001, 1000] })
  })

  it.each(['abc', '100, x', '-1', '1.5', '4294967296'])('refuses %s', (text) => {
    expect(readNfsOtherGids(text)).toEqual({ kind: 'not_ids' })
  })

  it('takes the RFC 5531 limit of 16 and refuses one more, counting each id once', () => {
    const ids = Array.from({ length: 16 }, (_, index) => 2000 + index)
    expect(readNfsOtherGids(ids.join(','))).toEqual({ kind: 'ids', ids })
    expect(readNfsOtherGids([...ids, 2000].join(','))).toEqual({ kind: 'ids', ids })
    expect(readNfsOtherGids([...ids, 3000].join(','))).toEqual({ kind: 'too_many', count: 17 })
  })
})

describe('the guessed identity', () => {
  const guess = { uid: 1000, gid: 100, otherGids: [], source: 'most_common_owner' as const }

  it('fills empty fields, and says where the guess came from', () => {
    expect(withGuessedIdentity(emptyNfsFields(), guess)).toMatchObject({
      uid: '1000',
      gid: '100',
      identityFromGuess: true,
      identitySource: 'most_common_owner',
    })
  })

  it('replaces an earlier guess, since nobody chose that one', () => {
    const first = withGuessedIdentity(emptyNfsFields(), { uid: 501, gid: 20, otherGids: [], source: 'this_mac' })
    expect(withGuessedIdentity(first, guess)).toMatchObject({ uid: '1000', identitySource: 'most_common_owner' })
  })

  it('fills the guessed other groups as a list to edit', () => {
    const fields = withGuessedIdentity(emptyNfsFields(), { ...guess, otherGids: [2001, 2002] })
    expect(fields.otherGids).toBe('2001, 2002')
  })

  it('makes every id the person’s once they type into the other groups', () => {
    const typed = withTypedIdentity(withGuessedIdentity(emptyNfsFields(), guess), { otherGids: '2001' })
    expect(typed).toMatchObject({ otherGids: '2001', identityFromGuess: false, identitySource: null })
    expect(withGuessedIdentity(typed, { ...guess, otherGids: [3000] }).otherGids).toBe('2001')
  })

  it('never overwrites what the person typed', () => {
    const typed = withTypedIdentity(emptyNfsFields(), { uid: '1026' })
    expect(typed).toMatchObject({ uid: '1026', identityFromGuess: false, identitySource: null })
    expect(withGuessedIdentity(typed, guess)).toEqual(typed)
  })

  it('warns about user ID 0, which most servers squash to nobody, and blocks nothing', () => {
    const root = withTypedIdentity(emptyNfsFields(), { uid: '0', gid: '0' })
    expect(nfsIdentityWarns(root)).toBe(true)
    expect(nfsIdentityWarns(withTypedIdentity(emptyNfsFields(), { uid: '1000' }))).toBe(false)
  })
})

/** A form with NFS selected, an address typed, and an identity filled. */
function nfsForm(address: string) {
  const form = { ...emptyServerForm(), protocol: 'nfs' as const, address }
  const parsed = applyParsedAddress(form, parseServerAddress(address))
  return { ...parsed, nfs: withTypedIdentity(parsed.nfs, { uid: '1000', gid: '100' }) }
}

describe('an NFS address in the form', () => {
  it('fills the export from the path the address carried, whole', () => {
    expect(nfsForm('nfs://testnas.local/volume1/data/photos').nfs.exportPath).toBe('/volume1/data/photos')
    expect(nfsForm('testnas.local:/Public').nfs.exportPath).toBe('/Public')
  })

  it('keeps a picked export when the address carries no path', () => {
    const form = { ...emptyServerForm(), protocol: 'nfs' as const, nfs: { ...emptyNfsFields(), exportPath: '/Public' } }
    expect(applyParsedAddress(form, parseServerAddress('testnas.local')).nfs.exportPath).toBe('/Public')
  })

  it('takes no path from an address that names another protocol', () => {
    expect(nfsForm('sftp://testnas.local/srv').nfs.exportPath).toBe('')
  })
})

describe('serverTargetFrom: NFS', () => {
  it('sends the typed other groups, and nothing while they don’t read', () => {
    const form = nfsForm('nfs://testnas.local/Public')
    const typed = (otherGids: string) => ({ ...form, nfs: withTypedIdentity(form.nfs, { otherGids }) })
    expect(serverTargetFrom(typed('2001, 2002'))).toMatchObject({ otherGids: [2001, 2002] })
    expect(serverTargetFrom(typed('x'))).toBeNull()
    expect(serverTargetFrom(typed(Array.from({ length: 17 }, (_, index) => index + 1).join(',')))).toBeNull()
  })

  it('sends the host, the typed port or none, the whole export path, and the identity', () => {
    expect(serverTargetFrom(nfsForm('nfs://testnas.local/volume1/data/photos'))).toEqual({
      protocol: 'nfs',
      displayName: '',
      host: 'testnas.local',
      port: null,
      export: '/volume1/data/photos',
      uid: 1000,
      gid: 100,
      otherGids: [],
      startFolder: null,
      autoReconnect: true,
    })
    expect(serverTargetFrom(nfsForm('nfs://testnas.local:15481/insecure'))).toMatchObject({
      port: 15481,
      export: '/insecure',
    })
    expect(serverTargetFrom(nfsForm('testnas.local:2049/Public'))).toMatchObject({ port: 2049 })
  })

  it("drops a port another protocol's scheme named", () => {
    const form = { ...nfsForm('sftp://testnas.local:2222'), nfs: { ...nfsForm('x').nfs, exportPath: '/Public' } }
    expect(serverTargetFrom(form)).toMatchObject({ host: 'testnas.local', port: null })
  })

  it('names no target while the export or an id is missing', () => {
    expect(serverTargetFrom(nfsForm('nfs://testnas.local'))).toBeNull()
    const noUid = nfsForm('nfs://testnas.local/Public')
    expect(serverTargetFrom({ ...noUid, nfs: withTypedIdentity(noUid.nfs, { uid: '' }) })).toBeNull()
  })
})

describe('nameFallbackOf: NFS', () => {
  it('names the SERVER by its host, as `nfs_known_places::server_label` does, whatever the export', () => {
    // The Name field names the server; each export reads as its own path under it.
    expect(nameFallbackOf(nfsForm('nfs://testnas.local/Public'))).toBe('testnas.local')
    expect(nameFallbackOf(nfsForm('nfs://testnas.local:15481/insecure'))).toBe('testnas.local')
    expect(nameFallbackOf(nfsForm('nfs://testnas.local'))).toBe('testnas.local')
  })
})

describe('an NFS prefill', () => {
  it('opens on NFS with the URL as typed, its path the export to split', () => {
    const form = formFromPrefill('nfs://testnas.local/volume1/data/photos')
    expect(form).toMatchObject({ protocol: 'nfs', address: 'nfs://testnas.local/volume1/data/photos' })
    expect(form.nfs.exportPath).toBe('/volume1/data/photos')
  })

  it('reads an app path whose export is escaped as its export and the folder under it', () => {
    expect(nfsFieldsFromAppPath('nfs://testnas.local:2049/volume1%2Fdata/photos/2026')).toEqual({
      address: 'nfs://testnas.local:2049',
      exportPath: '/volume1/data',
      startFolder: '/photos/2026',
    })
    const form = formFromPrefill('nfs://testnas.local:2049/%2F/etc')
    expect(form).toMatchObject({ protocol: 'nfs', address: 'nfs://testnas.local:2049', startFolder: '/etc' })
    expect(form.nfs.exportPath).toBe('/')
  })

  it('leaves an unescaped one to the address reading: a person may have typed it', () => {
    // `nfs://nas:2049/share/Public/x` could be export `/share/Public` typed by hand,
    // so only the server's export list can say where the export ends.
    expect(nfsFieldsFromAppPath('nfs://nas:2049/share/Public/x')).toBeNull()
    expect(nfsFieldsFromAppPath('sftp://test_user_1@nas:22/srv')).toBeNull()
  })
})

describe('formFromNfsPlace', () => {
  const place: SavedNfsPlace = {
    volumeId: 'nfs-data-nas-1a2b',
    host: 'testnas.local',
    port: null,
    export: '/volume1/data',
    uid: 1026,
    gid: 100,
    otherGids: [2001, 2002],
    displayName: 'Testnas',
    startFolder: '/photos',
    autoReconnect: false,
    pinned: true,
  }

  it('reads the saved export back, the identity included', () => {
    expect(formFromNfsPlace(place)).toMatchObject({
      protocol: 'nfs',
      address: 'nfs://testnas.local',
      displayName: 'Testnas',
      startFolder: '/photos',
      autoReconnect: false,
      nfs: { exportPath: '/volume1/data', uid: '1026', gid: '100', otherGids: '2001, 2002' },
    })
    expect(formFromNfsPlace({ ...place, port: 15481 }).address).toBe('nfs://testnas.local:15481')
  })

  it('opens the ids as the person’s, so no guess lands over the saved ones', () => {
    const form = formFromNfsPlace(place)
    expect(form.nfs.identityFromGuess).toBe(false)
    expect(withGuessedIdentity(form.nfs, { uid: 0, gid: 0, otherGids: [], source: 'this_mac' }).uid).toBe('1026')
  })

  it('saves back the target it opened on', () => {
    expect(serverTargetFrom(formFromNfsPlace(place))).toEqual({
      protocol: 'nfs',
      displayName: 'Testnas',
      host: 'testnas.local',
      port: null,
      export: '/volume1/data',
      uid: 1026,
      gid: 100,
      otherGids: [2001, 2002],
      startFolder: '/photos',
      autoReconnect: false,
    })
  })
})
