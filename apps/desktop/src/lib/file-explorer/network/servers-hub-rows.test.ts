/**
 * What the hub lists, and in what order.
 *
 * The merge is the part that can go quietly wrong: a manually-typed SMB host is
 * BOTH a saved server and a discovered host (adding one injects it into the
 * discovery state), so a naive concatenation shows the user's NAS twice.
 */

import { describe, it, expect } from 'vitest'
import { buildHubRows, isNearbyOnly, openMoveFor, savedSmbHostIds } from './servers-hub-rows'
import type { SavedServer } from '$lib/tauri-commands'
import type { NetworkHost, VolumeInfo } from '../types'
import type { SignedInAs } from './signed-in-as'

function sftpServer(overrides: Partial<SavedServer> = {}): SavedServer {
  const id = overrides.id ?? 'sftp-nas.local-22-ada'
  return {
    id,
    protocol: 'sftp',
    displayName: 'Naspolya',
    nameSource: 'user',
    address: 'nas.local:22',
    username: 'ada',
    pinned: true,
    lastConnectedAt: '2026-09-01T10:00:00Z',
    autoReconnect: true,
    places: [
      {
        volumeId: id,
        name: 'Naspolya',
        pinned: true,
        connected: false,
        appRoot: `sftp://ada@nas.local:22`,
        username: 'ada',
        autoReconnect: true,
      },
    ],
    ...overrides,
  }
}

function smbServer(overrides: Partial<SavedServer> = {}): SavedServer {
  return {
    id: 'manual-10-0-0-4-445',
    protocol: 'smb',
    displayName: 'Attic NAS',
    nameSource: 'fallback',
    address: '10.0.0.4',
    username: null,
    pinned: false,
    lastConnectedAt: null,
    autoReconnect: null,
    places: [],
    ...overrides,
  }
}

function host(overrides: Partial<NetworkHost> = {}): NetworkHost {
  return { id: 'h1', name: 'Attic NAS', port: 445, source: 'discovered', ...overrides }
}

describe('row identity', () => {
  /**
   * ❗ The hub keys its `{#each}` on `row.id`, and Svelte THROWS
   * (`each_key_duplicate`) on a repeat — a pane that crashes rather than one that
   * shows a row twice. The sources can genuinely repeat an id: `listSavedServers`
   * unions three stores, and a server recorded in two of them arrives twice. So
   * uniqueness is this function's invariant, ❌ not its callers'.
   */
  it('never emits two rows under one id, whatever the sources repeat', () => {
    const rows = buildHubRows({
      saved: [
        smbServer({ id: 'dup', displayName: 'Attic NAS', address: '10.0.0.4' }),
        smbServer({ id: 'dup', displayName: 'Attic NAS (again)', address: '10.0.0.9' }),
        sftpServer({ id: 'dup' }),
      ],
      hosts: [host({ id: 'dup', name: 'Somewhere else' })],
      volumes: [],
    })

    const ids = rows.map((row) => row.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('keeps the FIRST row under a repeated id, so the merge order still decides', () => {
    const rows = buildHubRows({
      saved: [smbServer({ id: 'dup', displayName: 'The real one' }), smbServer({ id: 'dup', displayName: 'The echo' })],
      hosts: [],
      volumes: [],
    })

    expect(rows.map((row) => row.name)).toEqual(['The real one'])
  })
})

function volume(id: string, connectionState: VolumeInfo['connectionState']): VolumeInfo {
  return {
    id,
    name: 'Naspolya',
    path: `sftp://ada@nas.local:22`,
    category: 'network',
    isEjectable: false,
    fsType: 'sftp',
    connectionState,
  }
}

describe('buildHubRows: what appears', () => {
  it('lists one row per saved server and one per discovered host', () => {
    const rows = buildHubRows({
      saved: [sftpServer()],
      hosts: [host({ id: 'h9', name: 'Someone else' })],
      volumes: [],
    })
    expect(rows.map((r) => r.name)).toEqual(['Naspolya', 'Someone else'])
  })

  it('shows a manually-typed SMB host ONCE, not as a saved row and a discovered row', () => {
    // Adding a manual server injects it into the discovery state, so it comes
    // back from both sources with the same id.
    const rows = buildHubRows({
      saved: [smbServer({ id: 'manual-10-0-0-4-445' })],
      hosts: [host({ id: 'manual-10-0-0-4-445', name: 'Attic NAS', source: 'manual' })],
      volumes: [],
    })
    expect(rows).toHaveLength(1)
    expect(rows[0].host).not.toBeNull()
    expect(rows[0].saved).not.toBeNull()
  })

  it('matches a saved SMB host to a discovered one by name when the ids differ', () => {
    // `known_shares` files a host under the server name statfs reported; mDNS
    // files the same machine under its Bonjour name and its own id.
    const rows = buildHubRows({
      saved: [smbServer({ id: 'manual-naspolya-445', address: 'Naspolya', displayName: 'Naspolya' })],
      hosts: [host({ id: 'bonjour-1', name: 'naspolya' })],
      volumes: [],
    })
    expect(rows).toHaveLength(1)
  })

  it('keeps two servers that only share a protocol apart', () => {
    const rows = buildHubRows({
      saved: [sftpServer(), sftpServer({ id: 'sftp-other-22-bo', displayName: 'Jump box', address: 'other:22' })],
      hosts: [],
      volumes: [],
    })
    expect(rows).toHaveLength(2)
  })
})

describe('buildHubRows: whose name the Name column shows', () => {
  /** The label an SMB store carries: the mount's spelling, or a typed address. */
  const standIn = (overrides: Partial<SavedServer> = {}): SavedServer =>
    smbServer({
      id: 'manual-smb-consumer-guest-445',
      displayName: 'smb-consumer-guest',
      address: 'smb-consumer-guest',
      nameSource: 'fallback',
      ...overrides,
    })

  /**
   * Three ranks, and the top one is a fact the backend publishes (`nameSource`),
   * ❌ never a guess at the string's shape: a name a PERSON chose, then the
   * Bonjour name mDNS found, then the stand-in nobody chose.
   */
  it('keeps the name the user chose, even when mDNS spells the same host differently', () => {
    const rows = buildHubRows({
      saved: [
        smbServer({
          id: 'manual-naspolya-445',
          displayName: 'Naspolya',
          address: 'naspolya.local',
          nameSource: 'user',
        }),
      ],
      hosts: [host({ id: 'bonjour-1', name: 'Naspolya Media Server', hostname: 'naspolya.local' })],
      volumes: [],
    })
    expect(rows.map((r) => r.name)).toEqual(['Naspolya'])
  })

  it('shows the Bonjour name over the stand-in, which nobody chose', () => {
    // Opening a host writes a `known_shares` row named the way `statfs` spells
    // the server, and the friendly name a person recognizes must survive that.
    const rows = buildHubRows({
      saved: [standIn()],
      hosts: [host({ id: 'bonjour-1', name: 'SMB Test (Guest)', hostname: 'smb-consumer-guest' })],
      volumes: [],
    })
    expect(rows.map((r) => r.name)).toEqual(['SMB Test (Guest)'])
  })

  it('falls back to the stand-in when mDNS is seeing nothing', () => {
    const rows = buildHubRows({ saved: [standIn()], hosts: [], volumes: [] })
    expect(rows.map((r) => r.name)).toEqual(['smb-consumer-guest'])
  })

  /**
   * ❗ Typing a host that mDNS already found puts the machine in the discovery
   * list TWICE, under both spellings. One row, named the way a person would
   * recognize it.
   */
  it('claims both spellings of one machine, and takes the discovered name', () => {
    const rows = buildHubRows({
      saved: [standIn()],
      hosts: [
        host({ id: 'manual-smb-consumer-guest-445', name: 'smb-consumer-guest', source: 'manual' }),
        host({ id: 'bonjour-1', name: 'SMB Test (Guest)', hostname: 'smb-consumer-guest' }),
      ],
      volumes: [],
    })
    expect(rows.map((r) => r.name)).toEqual(['SMB Test (Guest)'])
  })
})

describe('buildHubRows: status', () => {
  it('calls a place with a live session Connected', () => {
    const server = sftpServer()
    const rows = buildHubRows({ saved: [server], hosts: [], volumes: [volume(server.id, 'direct')] })
    expect(rows[0].status).toBe('connected')
  })

  it('calls a place whose credential the server refused Signed out', () => {
    const server = sftpServer()
    const rows = buildHubRows({ saved: [server], hosts: [], volumes: [volume(server.id, 'needs_sign_in')] })
    expect(rows[0].status).toBe('signed_out')
  })

  it('calls a place waiting on a host-key decision Waiting for the key', () => {
    const server = sftpServer()
    const rows = buildHubRows({
      saved: [server],
      hosts: [],
      volumes: [volume(server.id, 'needs_host_key_approval')],
    })
    expect(rows[0].status).toBe('waiting_for_key')
  })

  it('calls a saved place with no session Saved, dropped session included', () => {
    const server = sftpServer()
    expect(buildHubRows({ saved: [server], hosts: [], volumes: [] })[0].status).toBe('saved')
    expect(buildHubRows({ saved: [server], hosts: [], volumes: [volume(server.id, 'disconnected')] })[0].status).toBe(
      'saved',
    )
  })

  it('calls a host only mDNS knows about Found nearby', () => {
    const rows = buildHubRows({ saved: [], hosts: [host()], volumes: [] })
    expect(rows[0].status).toBe('found_nearby')
  })

  it('calls a saved SMB host Found nearby while mDNS sees it', () => {
    const rows = buildHubRows({
      saved: [smbServer()],
      hosts: [host({ id: 'manual-10-0-0-4-445', source: 'manual' }), host({ id: 'attic-smb-tcp-local' })],
      volumes: [],
    })
    expect(rows[0].status).toBe('found_nearby')
  })

  /**
   * ❗ A typed-in host is in the discovery list because Cmdr put it there
   * (`load_manual_servers` injects every saved one at startup, reachable or not),
   * so its own entry says nothing about the network. Reading it as "found nearby"
   * called `localhost:11482` nearby the moment it was added (QA 2026-09-25).
   */
  it('calls a saved SMB host Saved when the only host behind it is its own manual entry', () => {
    const rows = buildHubRows({
      saved: [smbServer()],
      hosts: [host({ id: 'manual-10-0-0-4-445', source: 'manual' })],
      volumes: [],
    })
    expect(rows[0].status).toBe('saved')
  })

  it('reads the place’s standing off the volume list, not the saved entry’s stale flag', () => {
    // `SavedPlace.connected` is a snapshot from the moment the listing was built;
    // the volume list is what the switcher and the pane both read.
    const server = sftpServer({
      places: [
        {
          volumeId: 'sftp-nas.local-22-ada',
          name: 'n',
          pinned: true,
          connected: true,
          username: 'ada',
          autoReconnect: true,
          appRoot: 'sftp://ada@nas.local:22',
        },
      ],
    })
    const rows = buildHubRows({ saved: [server], hosts: [], volumes: [] })
    expect(rows[0].status).toBe('saved')
  })
})

describe('buildHubRows: order', () => {
  it('puts live sessions first, then the ones asking for you, then saved, then nearby', () => {
    const live = sftpServer({ id: 'sftp-a-22-a', displayName: 'A live' })
    const asking = sftpServer({ id: 'sftp-b-22-b', displayName: 'B asking' })
    const idle = sftpServer({ id: 'sftp-c-22-c', displayName: 'C idle' })
    const rows = buildHubRows({
      saved: [idle, asking, live],
      hosts: [host({ id: 'h2', name: 'D nearby' })],
      volumes: [volume(live.id, 'direct'), volume(asking.id, 'needs_sign_in')],
    })
    expect(rows.map((r) => r.name)).toEqual(['A live', 'B asking', 'C idle', 'D nearby'])
  })

  it('orders equals by most recently used, then by name', () => {
    const older = sftpServer({ id: 'sftp-a-22-a', displayName: 'Older', lastConnectedAt: '2026-01-01T00:00:00Z' })
    const newer = sftpServer({ id: 'sftp-b-22-b', displayName: 'Newer', lastConnectedAt: '2026-08-01T00:00:00Z' })
    const never = sftpServer({ id: 'sftp-c-22-c', displayName: 'Never', lastConnectedAt: null })
    const rows = buildHubRows({ saved: [never, older, newer], hosts: [], volumes: [] })
    expect(rows.map((r) => r.name)).toEqual(['Newer', 'Older', 'Never'])
  })

  it('sorts nearby hosts by name, case-insensitively', () => {
    const rows = buildHubRows({
      saved: [],
      hosts: [host({ id: '1', name: 'zeta' }), host({ id: '2', name: 'Alpha' })],
      volumes: [],
    })
    expect(rows.map((r) => r.name)).toEqual(['Alpha', 'zeta'])
  })

  /**
   * The hub folds the hosts nobody saved into one group at the end
   * (`servers-hub-items.ts`), so they have to be contiguous there. Both rows
   * below read "Found nearby" and neither was ever used, so the status rank and
   * the recency tie, and the name alone would put the unsaved one first.
   */
  it('puts a host nobody saved after every saved server, even one mDNS also sees', () => {
    const rows = buildHubRows({
      saved: [smbServer({ id: 'manual-zed', displayName: 'Zed', address: 'zed.local' })],
      hosts: [host({ id: 'h-alpha', name: 'Alpha' }), host({ id: 'h-zed', name: 'Zed', hostname: 'zed.local' })],
      volumes: [],
    })
    expect(rows.map((r) => r.name)).toEqual(['Zed', 'Alpha'])
    expect(rows.map(isNearbyOnly)).toEqual([false, true])
  })
})

describe('savedSmbHostIds', () => {
  it('names every discovered host a saved SMB server claims, and no other', () => {
    const ids = savedSmbHostIds(
      [smbServer({ id: 'manual-10-0-0-4-445', displayName: 'Attic NAS', address: '10.0.0.4' }), sftpServer()],
      [
        host({ id: 'manual-10-0-0-4-445', name: '10.0.0.4', source: 'manual' }),
        host({ id: 'bonjour-attic', name: 'Attic NAS' }),
        host({ id: 'bonjour-printer', name: 'Printer' }),
        // Same name as the SFTP server, which claims no SMB host.
        host({ id: 'bonjour-naspolya', name: 'Naspolya' }),
      ],
    )
    expect([...ids].sort()).toEqual(['bonjour-attic', 'manual-10-0-0-4-445'])
  })
})

describe('buildHubRows: what a row carries', () => {
  it('gives a one-place protocol its place’s volume id and pin, so a command can act on it', () => {
    const rows = buildHubRows({ saved: [sftpServer()], hosts: [], volumes: [] })
    expect(rows[0].volumeId).toBe('sftp-nas.local-22-ada')
    expect(rows[0].pinned).toBe(true)
  })

  it('gives an SMB host no volume id: its places are mounted shares, not one place', () => {
    const rows = buildHubRows({ saved: [smbServer()], hosts: [], volumes: [] })
    expect(rows[0].volumeId).toBeNull()
    expect(rows[0].pinned).toBe(false)
  })

  it('shows a host’s port in the Address column when it isn’t 445', () => {
    const rows = buildHubRows({
      saved: [smbServer({ id: 'manual-localhost-11482', address: 'localhost:11482' })],
      hosts: [
        host({
          id: 'manual-localhost-11482',
          name: 'localhost:11482',
          hostname: 'localhost',
          port: 11482,
          source: 'manual',
        }),
      ],
      volumes: [],
    })
    expect(rows[0].address).toBe('localhost:11482')
  })

  it('prefers the discovered host’s resolved address over the saved spelling', () => {
    const rows = buildHubRows({
      saved: [smbServer({ address: 'Attic NAS' })],
      hosts: [host({ id: 'manual-10-0-0-4-445', ipAddress: '10.0.0.4' })],
      volumes: [],
    })
    expect(rows[0].address).toBe('10.0.0.4')
  })
})

/**
 * ❗ cmdr-reports#7: a saved SMB share is "user + server + share", a row of its
 * own right under its server, so the list says what opening it will do.
 */
describe('saved SMB shares', () => {
  const withShares = smbServer({
    displayName: "Sven's NAS",
    nameSource: 'user',
    places: [
      {
        volumeId: 'smb-scans',
        name: 'Scans',
        pinned: false,
        connected: false,
        appRoot: 'smb://10.0.0.4/Scans',
        username: null,
        autoReconnect: null,
      },
      {
        volumeId: 'smb-container',
        name: 'Container',
        pinned: true,
        connected: false,
        appRoot: '/Volumes/Container',
        username: 'sven',
        autoReconnect: null,
      },
    ],
  })

  it('lists each share right under its server, by name, with the account it opens as', () => {
    const rows = buildHubRows({ saved: [withShares], hosts: [], volumes: [] })

    expect(rows.map((row) => [row.kind, row.name, row.account])).toEqual([
      ['server', "Sven's NAS", null],
      ['place', 'Container', { kind: 'user', username: 'sven' }],
      ['place', 'Scans', null],
    ])
    const container = rows[1]
    expect(container.volumeId).toBe('smb-container')
    expect(container.pinned).toBe(true)
    expect(container.parentId).toBe(rows[0].id)
    expect(container.place?.appRoot).toBe('/Volumes/Container')
    // ❗ The host itself still has no place to act on: its shares do.
    expect(rows[0].volumeId).toBeNull()
  })

  it('says a share is connected while its volume is mounted, and saved otherwise', () => {
    const mounted: VolumeInfo = {
      id: 'smb-container',
      name: 'Container on Sven',
      path: '/Volumes/Container',
      category: 'network',
      isEjectable: false,
      connectionState: 'os_mount',
    }
    const rows = buildHubRows({ saved: [withShares], hosts: [], volumes: [mounted] })

    expect(rows.find((row) => row.name === 'Container')?.status).toBe('connected')
    expect(rows.find((row) => row.name === 'Scans')?.status).toBe('saved')
  })

  /**
   * ❗ While a share is connected its row names the account the live mount signed in
   * as. The saved account is for the NEXT connect: after an Add as otheruser over a
   * mount signed in as testuser, the row read "Connected … as otheruser" (QA round 7).
   */
  it('names the live mount’s account while connected, and the saved one otherwise', () => {
    const live = (mountAccount: string | null): VolumeInfo => ({
      id: 'smb-container',
      name: 'Container on Sven',
      path: '/Volumes/Container',
      category: 'attached_volume',
      isEjectable: false,
      connectionState: 'direct',
      mountAccount,
    })
    const account = (volumes: VolumeInfo[]) =>
      buildHubRows({ saved: [withShares], hosts: [], volumes }).find((row) => row.name === 'Container')?.account

    expect(account([live('testuser')])).toEqual({ kind: 'user', username: 'testuser' })
    expect(account([live('GUEST')]), 'a guest mount is signed in as guest').toEqual({ kind: 'guest' })
    expect(account([live(null)]), 'a mount that says nothing leaves the saved one').toEqual({
      kind: 'user',
      username: 'sven',
    })
    expect(account([]), 'not connected: the account the next connect uses').toEqual({
      kind: 'user',
      username: 'sven',
    })
  })

  /**
   * ❗ An SMB SERVER row names the SERVER-level account, the same one its share list's
   * header names: the account its listing signed in as, else the one it's set to be used
   * with. ❌ Never a share's mount: "My NAS as guest" (from the one live mount, public, as
   * guest) disagreed with the header's "as testuser" (final QA). Share rows keep their
   * own mount's account.
   */
  describe('the account a server row is signed in as', () => {
    const live = (id: string, mountAccount: string): VolumeInfo => ({
      id,
      name: id,
      path: `/Volumes/${id}`,
      category: 'attached_volume',
      isEjectable: false,
      connectionState: 'direct',
      mountAccount,
    })
    const rowsFor = (volumes: VolumeInfo[], listed?: SignedInAs, preference: string | null = null) =>
      buildHubRows({
        saved: [{ ...withShares, username: preference }],
        hosts: [],
        volumes,
        listedAs: (hostId) => (hostId === withShares.id ? listed : undefined),
      })
    const serverAccount = (volumes: VolumeInfo[], listed?: SignedInAs, preference: string | null = null) =>
      rowsFor(volumes, listed, preference)[0].account

    it('names the account its share list signed in as', () => {
      expect(serverAccount([], { kind: 'guest' })).toEqual({ kind: 'guest' })
      expect(serverAccount([], { kind: 'user', username: 'ada' })).toEqual({ kind: 'user', username: 'ada' })
    })

    it('ignores its shares’ mounts, which keep their own account on their own rows', () => {
      const rows = rowsFor([live('smb-container', 'GUEST')], { kind: 'user', username: 'testuser' })
      expect(rows[0].account).toEqual({ kind: 'user', username: 'testuser' })
      expect(rows.find((row) => row.name === 'Container')?.account).toEqual({ kind: 'guest' })
    })

    it('falls back to the account it is set to be used with when no listing said', () => {
      expect(serverAccount([live('smb-container', 'GUEST')], undefined, 'sven')).toEqual({
        kind: 'user',
        username: 'sven',
      })
    })

    it('says nothing when nothing is known', () => {
      expect(serverAccount([live('smb-container', 'testuser')])).toBeNull()
    })

    it('names a nearby host’s listing account too', () => {
      const rows = buildHubRows({
        saved: [],
        hosts: [host()],
        volumes: [],
        listedAs: (hostId) => (hostId === 'h1' ? { kind: 'guest' } : undefined),
      })
      expect(rows[0].account).toEqual({ kind: 'guest' })
    })
  })

  it('keeps shares under their own server when the servers sort around them', () => {
    const rows = buildHubRows({
      saved: [withShares, sftpServer({ lastConnectedAt: '2026-09-20T00:00:00Z' })],
      hosts: [],
      volumes: [],
    })

    const names = rows.map((row) => row.name)
    expect(names.indexOf('Container')).toBe(names.indexOf("Sven's NAS") + 1)
    expect(names.indexOf('Scans')).toBe(names.indexOf("Sven's NAS") + 2)
  })
})

/** What Enter does, per row kind: the hub's one decision about where a row leads. */
describe('openMoveFor', () => {
  const withShares = smbServer({
    places: [
      {
        volumeId: 'smb-scans',
        name: 'Scans',
        pinned: false,
        connected: false,
        appRoot: 'smb://10.0.0.4/Scans',
        username: null,
        autoReconnect: null,
      },
      {
        volumeId: 'smb-container',
        name: 'Container',
        pinned: true,
        connected: false,
        appRoot: '/Volumes/Container',
        username: 'sven',
        autoReconnect: null,
      },
    ],
  })
  const saved: VolumeInfo = {
    id: 'smb-container',
    name: 'Container on Attic NAS',
    path: '/Volumes/Container',
    category: 'network',
    isEjectable: false,
    connectionState: 'saved',
  }

  it('opens a server host into its share list, never a share', () => {
    const rows = buildHubRows({ saved: [withShares], hosts: [], volumes: [] })
    const move = openMoveFor(rows[0], rows, [])
    expect(move?.kind).toBe('host')
  })

  it('takes the pane to a share the volume list has a place for', () => {
    const rows = buildHubRows({ saved: [withShares], hosts: [], volumes: [saved] })
    const container = rows.find((row) => row.name === 'Container')
    if (!container) throw new Error('no Container row')
    expect(openMoveFor(container, rows, [saved])).toEqual({ kind: 'place', row: container })
  })

  it('opens a share no mount went through via its host, naming the share', () => {
    const rows = buildHubRows({ saved: [withShares], hosts: [], volumes: [] })
    const scans = rows.find((row) => row.name === 'Scans')
    if (!scans) throw new Error('no Scans row')
    expect(openMoveFor(scans, rows, [])).toMatchObject({
      kind: 'share_via_host',
      share: 'Scans',
      host: { id: 'manual-10-0-0-4-445', hostname: '10.0.0.4' },
    })
  })

  it('opens a saved host mDNS isn’t seeing on the port its address names', () => {
    const offPort = smbServer({ id: 'manual-localhost-11482', displayName: 'Both box', address: 'localhost:11482' })
    const rows = buildHubRows({ saved: [offPort], hosts: [], volumes: [] })
    expect(openMoveFor(rows[0], rows, [])).toMatchObject({
      kind: 'host',
      host: { id: 'manual-localhost-11482', hostname: 'localhost', port: 11482 },
    })
  })
})
