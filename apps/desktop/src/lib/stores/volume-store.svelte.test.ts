/**
 * Unit tests for the volume store's duplicate-ID defense.
 *
 * A volume ID is identity, and several consumers feed the list straight into a
 * keyed `{#each}` (the transfer dialog's destination picker, the tab bar's name
 * map). Svelte throws `each_key_duplicate` during flush on a repeated key, and a
 * dialog that throws mid-render leaves the pane's keyboard suppressed with
 * nothing on screen. The backend collapses double mounts, so this is the second
 * line of defense at the ONE place the frontend's list is built.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { VolumeInfo } from '$lib/file-explorer/types'

type VolumesPayload = { data: VolumeInfo[]; timedOut: boolean }

// Hoisted mocks: must run before importing the module under test.
const mockListVolumes = vi.fn<() => Promise<VolumesPayload>>()
let lastVolumesHandler: ((payload: VolumesPayload) => void) | null = null
const mockUnlisten = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  listVolumes: () => mockListVolumes(),
  refreshVolumes: () => Promise.resolve(),
  onVolumesChanged: (handler: (payload: VolumesPayload) => void) => {
    lastVolumesHandler = handler
    return Promise.resolve(mockUnlisten)
  },
  onVolumeConnectionChanged: () => Promise.resolve(mockUnlisten),
}))

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => seenFlag[id] ?? false,
  setSetting: (id: string, value: boolean) => {
    seenFlag[id] = value
  },
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: (content: unknown, options: { props?: Record<string, unknown> }) => {
    raisedToasts.push(options.props ?? {})
    return 'toast-id'
  },
}))

const seenFlag: Record<string, boolean> = {}
const raisedToasts: Record<string, unknown>[] = []

import { initVolumeStore, cleanupVolumeStore, getVolumes, toConnectionState } from './volume-store.svelte'

/** A share mounted twice: two paths, one volume ID. */
function doublyMountedShare(): VolumeInfo[] {
  return [
    {
      id: 'smb-naspi-a1b2c3',
      name: 'naspi on Naspolya',
      path: '/Volumes/naspi',
      category: 'attached_volume',
      isEjectable: false,
    },
    {
      id: 'smb-naspi-a1b2c3',
      name: 'naspi on Naspolya',
      path: '/Volumes/naspi-1',
      category: 'attached_volume',
      isEjectable: false,
    },
    {
      id: 'root',
      name: 'Macintosh HD',
      path: '/',
      category: 'main_volume',
      isEjectable: false,
    },
  ]
}

describe('volume-store duplicate IDs', () => {
  beforeEach(() => {
    mockListVolumes.mockReset()
    mockUnlisten.mockReset()
    lastVolumesHandler = null
    cleanupVolumeStore()
  })

  afterEach(() => {
    cleanupVolumeStore()
  })

  it('drops a duplicate ID from the bootstrap listing, keeping the first', async () => {
    mockListVolumes.mockResolvedValue({ data: doublyMountedShare(), timedOut: false })
    await initVolumeStore()

    const volumes = getVolumes()
    expect(volumes.map((v) => v.id)).toEqual(['smb-naspi-a1b2c3', 'root'])
    expect(volumes[0].path).toBe('/Volumes/naspi')
  })

  it('drops a duplicate ID from a pushed volumes-changed event', async () => {
    mockListVolumes.mockResolvedValue({ data: [], timedOut: false })
    await initVolumeStore()
    if (!lastVolumesHandler) throw new Error("init() didn't install a listener")
    lastVolumesHandler({ data: doublyMountedShare(), timedOut: false })

    expect(getVolumes().map((v) => v.id)).toEqual(['smb-naspi-a1b2c3', 'root'])
  })

  it('leaves a list with distinct IDs untouched', async () => {
    const distinct = doublyMountedShare().filter((v) => v.path !== '/Volumes/naspi-1')
    mockListVolumes.mockResolvedValue({ data: distinct, timedOut: false })
    await initVolumeStore()

    expect(getVolumes()).toEqual(distinct)
  })
})

describe('toConnectionState — the wire enum the picker renders', () => {
  it('maps every wire variant, so a dropped session reaches the dot before the next volumes-changed', () => {
    // `needs_credentials` and `needs_host_key_approval` used to fall to `null`, so
    // a live server that stopped retrying kept rendering as connected until the
    // backend happened to republish the whole list. The pane's `signed_out` state
    // rides on this mapping.
    expect(toConnectionState('connected')).toBe('direct')
    expect(toConnectionState('disconnected')).toBe('disconnected')
    expect(toConnectionState('needs_credentials')).toBe('needs_sign_in')
    expect(toConnectionState('needs_host_key_approval')).toBe('needs_host_key_approval')
  })
})

/** A pinned server place, as the servers arm publishes one. */
function pinnedPlace(index: number): VolumeInfo {
  return {
    id: `sftp-nas${String(index)}.local-22-ada`,
    name: `NAS ${String(index)}`,
    path: `sftp://ada@nas${String(index)}.local:22`,
    category: 'network',
    isEjectable: false,
    pinned: true,
  }
}

function favorite(index: number): VolumeInfo {
  return {
    id: `favorite-${String(index)}`,
    name: `Folder ${String(index)}`,
    path: `/Users/ada/folder-${String(index)}`,
    category: 'favorite',
    isEjectable: false,
  }
}

/**
 * The switcher's Network group can only grow past what fits by the user pinning
 * things, so the store that publishes the list is where the count is noticed.
 */
describe('the pin hint, raised where the pinned count is observed', () => {
  beforeEach(() => {
    mockListVolumes.mockReset()
    mockListVolumes.mockResolvedValue({ data: [], timedOut: false })
    lastVolumesHandler = null
    raisedToasts.length = 0
    for (const key of Object.keys(seenFlag)) delete seenFlag[key]
    cleanupVolumeStore()
  })

  afterEach(() => {
    cleanupVolumeStore()
  })

  it('says nothing while four servers are pinned', async () => {
    await initVolumeStore()
    lastVolumesHandler?.({ data: [pinnedPlace(1), pinnedPlace(2), pinnedPlace(3), pinnedPlace(4)], timedOut: false })

    expect(raisedToasts).toHaveLength(0)
    expect(seenFlag['behavior.serversPinHintSeen']).toBeUndefined()
  })

  it('raises the hint at the fifth, and never again', async () => {
    const five = [1, 2, 3, 4, 5].map(pinnedPlace)
    await initVolumeStore()
    lastVolumesHandler?.({ data: five, timedOut: false })
    lastVolumesHandler?.({ data: [...five, pinnedPlace(6)], timedOut: false })

    expect(raisedToasts).toHaveLength(1)
    expect(raisedToasts[0]).toEqual({ mentionFavorites: false })
    expect(seenFlag['behavior.serversPinHintSeen']).toBe(true)
  })

  it('adds the favorites line when those are piling up too', async () => {
    await initVolumeStore()
    lastVolumesHandler?.({
      data: [...[1, 2, 3, 4, 5].map(pinnedPlace), favorite(1), favorite(2), favorite(3)],
      timedOut: false,
    })

    expect(raisedToasts[0]).toEqual({ mentionFavorites: true })
  })

  it('notices a list that arrives through the bootstrap, not the event', async () => {
    mockListVolumes.mockResolvedValue({ data: [1, 2, 3, 4, 5].map(pinnedPlace), timedOut: false })
    await initVolumeStore()

    expect(raisedToasts).toHaveLength(1)
  })
})
