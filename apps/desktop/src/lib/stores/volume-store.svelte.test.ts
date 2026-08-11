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

import { initVolumeStore, cleanupVolumeStore, getVolumes } from './volume-store.svelte'

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
