/**
 * The NFS wrappers.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    listNfsExports: vi.fn(),
    guessNfsIdentity: vi.fn(),
    getKnownNfsPlaces: vi.fn(),
  },
}))

import { commands, type SavedNfsPlace } from '#lib/ipc/bindings.js'
import { guessNfsIdentity, knownNfsPlaceOf, listNfsExports } from './nfs'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('asking a server before a place exists', () => {
  it('lists exports for the host and port as typed, a missing port staying null for the portmapper', async () => {
    vi.mocked(commands.listNfsExports).mockResolvedValueOnce({ outcome: 'listed', exports: ['/volume1/data'] })
    expect(await listNfsExports('testnas.local', null)).toEqual({ outcome: 'listed', exports: ['/volume1/data'] })
    expect(commands.listNfsExports).toHaveBeenCalledWith('testnas.local', null)
  })

  it('passes a refused listing through as an outcome, so the form keeps the export field open', async () => {
    vi.mocked(commands.listNfsExports).mockResolvedValueOnce({ outcome: 'listing_refused' })
    expect(await listNfsExports('testnas.local', 2049)).toEqual({ outcome: 'listing_refused' })
    expect(commands.listNfsExports).toHaveBeenCalledWith('testnas.local', 2049)
  })

  it('guesses the identity for that export, keeping host, port, and export in order', async () => {
    vi.mocked(commands.guessNfsIdentity).mockResolvedValueOnce({
      uid: 1026,
      gid: 100,
      otherGids: [],
      source: 'most_common_owner',
    })
    expect(await guessNfsIdentity('testnas.local', 2049, '/volume1/data')).toEqual({
      uid: 1026,
      gid: 100,
      otherGids: [],
      source: 'most_common_owner',
    })
    expect(commands.guessNfsIdentity).toHaveBeenCalledWith('testnas.local', 2049, '/volume1/data')
  })
})

describe('the saved places', () => {
  const data: SavedNfsPlace = {
    volumeId: 'nfs-data',
    host: 'testnas.local',
    port: null,
    export: '/volume1/data',
    uid: 1026,
    gid: 100,
    otherGids: [],
    displayName: '',
    startFolder: null,
    autoReconnect: true,
    pinned: false,
  }
  const photos: SavedNfsPlace = { ...data, volumeId: 'nfs-photos', export: '/volume1/photos', startFolder: '2026' }

  it('finds the place a volume id names by the id the backend published, ❌ never one minted here', async () => {
    vi.mocked(commands.getKnownNfsPlaces).mockResolvedValue([data, photos])
    const found = await knownNfsPlaceOf('nfs-photos')
    expect(found?.export).toBe('/volume1/photos')
    expect(found?.startFolder).toBe('2026')
  })

  it('answers null for an id no saved place carries, or when nothing is saved', async () => {
    vi.mocked(commands.getKnownNfsPlaces).mockResolvedValueOnce([data, photos])
    expect(await knownNfsPlaceOf('nfs-nothing')).toBeNull()
    vi.mocked(commands.getKnownNfsPlaces).mockResolvedValueOnce([])
    expect(await knownNfsPlaceOf('nfs-data')).toBeNull()
  })
})
