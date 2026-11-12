/**
 * The switcher's Network group: what it holds, and what it must never invent.
 *
 * The three-things rule (`DETAILS.md` § "The three-things rule, and which side
 * enforces it"):
 * the group holds the hub row, every place connected right now, and every
 * pinned place greyed out. ❗ The listing hands over EVERY saved place, pin and
 * all, because a volume id with no row is one the app denies exists: a hub Enter
 * and a restored tab both land on an id. So applying the cap is the switcher's
 * job, and it is this module's half of the contract. The one row it synthesizes
 * is the hub; a "helpful" fetch of `listSavedServers()` here would be a second
 * source of truth, and the last cell is what catches it.
 */
import { describe, expect, it, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { groupByCategory } from './volume-grouping'
import type { VolumeInfo } from '../types'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('the Mobile group', () => {
  const waiting = { kind: 'waiting_for_authorization' } as const

  function mobileRow(overrides: Partial<VolumeInfo>): VolumeInfo {
    return {
      id: 'mtp-testserial0001-abc:65537',
      name: 'Pixel 9 Pro XL',
      path: 'mtp://mtp-testserial0001-abc/65537',
      category: 'mobile_device',
      isEjectable: true,
      fsType: 'mtp',
      deviceReadiness: { kind: 'ready' },
      ...overrides,
    }
  }

  const adbRow = (overrides: Partial<VolumeInfo>): VolumeInfo =>
    mobileRow({ id: 'adb-testserial0001-def', path: 'adb://TESTSERIAL0001', fsType: 'adb', ...overrides })

  function mobileIds(vols: VolumeInfo[]): string[] {
    return (groupByCategory(vols).find((g) => g.category === 'mobile_device')?.items ?? []).map((v) => v.id)
  }

  it('shows a phone once while its ADB half still owes the Allow tap', () => {
    const mtp = mobileRow({})
    expect(mobileIds([mtp, adbRow({ deviceReadiness: waiting })])).toEqual([mtp.id])
  })

  it('shows both halves once both can be opened: one row per protocol', () => {
    const mtp = mobileRow({})
    const adb = adbRow({})
    expect(mobileIds([mtp, adb])).toEqual([mtp.id, adb.id])
  })

  it('keeps the waiting row when it is the only way to see the phone', () => {
    const adb = adbRow({ deviceReadiness: waiting })
    expect(mobileIds([adb])).toEqual([adb.id])
  })
})

function serverRow(overrides: Partial<VolumeInfo>): VolumeInfo {
  return {
    id: 'sftp-nas-22-test_user_1',
    name: 'Testnas',
    path: 'sftp://test_user_1@nas:22/srv/data',
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
    expect(items.map((v) => v.id)).toEqual(['network', 'sftp-nas-22-test_user_1'])
  })

  it('shows a pinned place that has no session, as its `saved` row', () => {
    const items = networkItems([serverRow({ connectionState: 'saved', pinned: true })])
    expect(items.map((v) => v.id)).toEqual(['network', 'sftp-nas-22-test_user_1'])
    expect(items[1].connectionState).toBe('saved')
  })

  it('shows a pinned SMB share that is not mounted, as its `saved` row', () => {
    // A saved share's row carries the volume id its last mount had, so the pin
    // and the mounted share are one id and never two rows.
    const share = serverRow({
      id: 'smb-testshare-media',
      name: 'media',
      path: '/Volumes/media',
      fsType: 'smbfs',
      connectionState: 'saved',
      pinned: true,
    })
    const items = networkItems([share])
    expect(items.map((v) => v.id)).toEqual(['network', 'smb-testshare-media'])
    expect(items[1].connectionState).toBe('saved')
  })

  it('hides an unmounted SMB share nobody pinned', () => {
    const share = serverRow({
      id: 'smb-testshare-media',
      name: 'media',
      path: '/Volumes/media',
      fsType: 'smbfs',
      connectionState: 'saved',
      pinned: false,
    })
    expect(networkItems([share]).map((v) => v.id)).toEqual(['network'])
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
    expect(items.map((v) => v.id)).toEqual(['network', 'sftp-nas-22-test_user_1'])
  })

  it('keeps a row that carries no pin at all', () => {
    // ❗ Only a server PLACE carries a pin. A mounted SMB share never was subject
    // to the cap, and on Linux it also carries no connection state, so a rule
    // written as "live or pinned" would drop it off the switcher entirely.
    const mounted = serverRow({
      id: 'smb-testshare-media',
      name: 'media',
      path: '/Volumes/media',
      fsType: 'smbfs',
      connectionState: undefined,
      pinned: undefined,
    })
    const items = networkItems([mounted])
    expect(items.map((v) => v.id)).toEqual(['network', 'smb-testshare-media'])
  })

  it('invents no row of its own beyond the hub', () => {
    const items = networkItems([serverRow({ connectionState: 'direct' })])
    expect(items).toHaveLength(2)
    expect(items.some((v) => v.id === 'webdav-unpinned-server')).toBe(false)
  })

  it('keeps mounted shares beside the server rows', () => {
    const mounted = serverRow({
      id: 'smb-testshare-media',
      name: 'media',
      path: '/Volumes/media',
      fsType: 'smbfs',
      connectionState: 'os_mount',
      pinned: undefined,
    })
    const items = networkItems([mounted, serverRow({ connectionState: 'saved', pinned: true })])
    expect(items.map((v) => v.id)).toEqual(['network', 'smb-testshare-media', 'sftp-nas-22-test_user_1'])
  })
})
