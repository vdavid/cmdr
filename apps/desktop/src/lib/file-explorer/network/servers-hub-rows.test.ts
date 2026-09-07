/**
 * What the hub lists, and in what order.
 *
 * The merge is the part that can go quietly wrong: a manually-typed SMB host is
 * BOTH a saved server and a discovered host (adding one injects it into the
 * discovery state), so a naive concatenation shows the user's NAS twice.
 */

import { describe, it, expect } from 'vitest'
import { buildHubRows } from './servers-hub-rows'
import type { SavedServer } from '$lib/tauri-commands'
import type { NetworkHost, VolumeInfo } from '../types'

function sftpServer(overrides: Partial<SavedServer> = {}): SavedServer {
  const id = overrides.id ?? 'sftp-nas.local-22-ada'
  return {
    id,
    protocol: 'sftp',
    displayName: 'Naspolya',
    address: 'nas.local:22',
    username: 'ada',
    pinned: true,
    lastConnectedAt: '2026-09-01T10:00:00Z',
    places: [{ volumeId: id, name: 'Naspolya', pinned: true, connected: false, appRoot: `sftp://ada@nas.local:22` }],
    ...overrides,
  }
}

function smbServer(overrides: Partial<SavedServer> = {}): SavedServer {
  return {
    id: 'manual-10-0-0-4-445',
    protocol: 'smb',
    displayName: 'Attic NAS',
    address: '10.0.0.4',
    username: null,
    pinned: false,
    lastConnectedAt: null,
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

  it('calls a saved SMB host that is answering right now Found nearby, not Saved', () => {
    const rows = buildHubRows({
      saved: [smbServer()],
      hosts: [host({ id: 'manual-10-0-0-4-445', source: 'manual' })],
      volumes: [],
    })
    expect(rows[0].status).toBe('found_nearby')
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

  it('prefers the discovered host’s resolved address over the saved spelling', () => {
    const rows = buildHubRows({
      saved: [smbServer({ address: 'Attic NAS' })],
      hosts: [host({ id: 'manual-10-0-0-4-445', ipAddress: '10.0.0.4' })],
      volumes: [],
    })
    expect(rows[0].address).toBe('10.0.0.4')
  })
})
