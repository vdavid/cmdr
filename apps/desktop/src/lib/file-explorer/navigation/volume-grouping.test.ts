/**
 * The switcher's Network group: what it holds, and what it must never invent.
 *
 * The three-things rule (`docs/specs/servers-hub-plan.md` § "The four rules"):
 * the group holds the hub row, every place connected right now, and every
 * pinned place greyed out. ❗ The PIN half is decided in Rust
 * (`server_volumes.rs::append_server_volumes`), which is the only side that can
 * read a pin: `VolumeInfo` carries none. So the frontend's half of the contract
 * is that it renders the listing and synthesizes exactly ONE row of its own —
 * the hub. A future "helpful" fetch of `listSavedServers()` here would put
 * unpinned servers back on screen, and the last cell is what catches it.
 */
import { describe, expect, it, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { groupByCategory } from './volume-grouping'
import type { VolumeInfo } from '../types'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

function serverRow(overrides: Partial<VolumeInfo>): VolumeInfo {
  return {
    id: 'sftp-nas-22-ada',
    name: 'Naspolya',
    path: 'sftp://ada@nas:22/srv/data',
    category: 'network',
    isEjectable: false,
    fsType: 'sftp',
    connectionState: 'direct',
    ...overrides,
  }
}

/** The rows of the Network group, in order. */
function networkItems(vols: VolumeInfo[]): VolumeInfo[] {
  const group = groupByCategory(vols).find((g) => g.category === 'network')
  return group?.items ?? []
}

describe('groupByCategory: the Network group', () => {
  it('always leads with the hub row, even with nothing else on the network', () => {
    const items = networkItems([])
    expect(items.map((v) => v.id)).toEqual(['network'])
    expect(items[0].path).toBe('smb://')
  })

  it('shows a connected place, pinned or not', () => {
    // A registered volume earns its row through its session, never through a pin
    // (`server_volumes.rs`: `place.is_registered() || place.pinned`).
    const items = networkItems([serverRow({ connectionState: 'direct' })])
    expect(items.map((v) => v.id)).toEqual(['network', 'sftp-nas-22-ada'])
  })

  it('shows a pinned place that has no session, as its `saved` row', () => {
    const items = networkItems([serverRow({ connectionState: 'saved' })])
    expect(items.map((v) => v.id)).toEqual(['network', 'sftp-nas-22-ada'])
    expect(items[1].connectionState).toBe('saved')
  })

  it('invents no row for a saved server the listing left out', () => {
    // An UNPINNED saved server is absent from the listing on purpose. The
    // frontend must not put it back: pins are the user's cap on how many saved
    // things crowd their own disks.
    const items = networkItems([serverRow({ connectionState: 'direct' })])
    expect(items).toHaveLength(2)
    expect(items.some((v) => v.id === 'webdav-unpinned-server')).toBe(false)
  })

  it('keeps mounted shares beside the server rows', () => {
    const mounted = serverRow({
      id: 'smb-naspi-media',
      name: 'media',
      path: '/Volumes/media',
      fsType: 'smbfs',
      connectionState: 'os_mount',
    })
    const items = networkItems([mounted, serverRow({ connectionState: 'saved' })])
    expect(items.map((v) => v.id)).toEqual(['network', 'smb-naspi-media', 'sftp-nas-22-ada'])
  })
})
