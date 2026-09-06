/**
 * The switcher's Network group: what it holds, and what it must never invent.
 *
 * The three-things rule (`docs/specs/servers-hub-plan.md` § "The four rules"):
 * the group holds the hub row, every place connected right now, and every
 * pinned place greyed out. ❗ The listing hands over EVERY saved place, pin and
 * all, because a volume id with no row is one the app denies exists: a hub Enter
 * and a restored tab both land on an id. So applying the cap is the switcher's
 * job, and it is this module's half of the contract. The one row it synthesizes
 * is the hub; a "helpful" fetch of `listSavedServers()` here would be a second
 * source of truth, and the last cell is what catches it.
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
    pinned: true,
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
    // A live session earns its row through the session, never through a pin.
    const items = networkItems([serverRow({ connectionState: 'direct', pinned: false })])
    expect(items.map((v) => v.id)).toEqual(['network', 'sftp-nas-22-ada'])
  })

  it('shows a pinned place that has no session, as its `saved` row', () => {
    const items = networkItems([serverRow({ connectionState: 'saved', pinned: true })])
    expect(items.map((v) => v.id)).toEqual(['network', 'sftp-nas-22-ada'])
    expect(items[1].connectionState).toBe('saved')
  })

  it('hides a saved place nobody pinned', () => {
    // The cap the user holds: 12 saved buckets must not push their own disks off
    // the switcher. The row is still in the LISTING, so Enter in the hub and a
    // restored tab both find the volume.
    const items = networkItems([serverRow({ connectionState: 'saved', pinned: false })])
    expect(items.map((v) => v.id)).toEqual(['network'])
  })

  it('keeps an unpinned place whose session is still being recovered', () => {
    // A dropped session is mid-recovery, and a row that vanishes while the
    // backoff loop runs takes the user's Disconnect control with it.
    const items = networkItems([serverRow({ connectionState: 'disconnected', pinned: false })])
    expect(items.map((v) => v.id)).toEqual(['network', 'sftp-nas-22-ada'])
  })

  it('keeps a row that carries no pin at all', () => {
    // ❗ Only a server PLACE carries a pin. A mounted SMB share never was subject
    // to the cap, and on Linux it also carries no connection state, so a rule
    // written as "live or pinned" would drop it off the switcher entirely.
    const mounted = serverRow({
      id: 'smb-naspi-media',
      name: 'media',
      path: '/Volumes/media',
      fsType: 'smbfs',
      connectionState: undefined,
      pinned: undefined,
    })
    const items = networkItems([mounted])
    expect(items.map((v) => v.id)).toEqual(['network', 'smb-naspi-media'])
  })

  it('invents no row of its own beyond the hub', () => {
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
      pinned: undefined,
    })
    const items = networkItems([mounted, serverRow({ connectionState: 'saved', pinned: true })])
    expect(items.map((v) => v.id)).toEqual(['network', 'smb-naspi-media', 'sftp-nas-22-ada'])
  })
})
