import { describe, it, expect, vi, beforeEach } from 'vitest'

const { getVolumesMock, getOptInMock } = vi.hoisted(() => ({
  getVolumesMock: vi.fn(),
  getOptInMock: vi.fn(),
}))

vi.mock('$lib/indexing', () => ({ ROOT_VOLUME_ID: 'root' }))
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: getVolumesMock }))
vi.mock('$lib/media-index/network-volume-prefs', () => ({ getNetworkOptInVolumes: getOptInMock }))

import { getEnabledMediaIndexVolumeIds } from './enabled-volumes'

beforeEach(() => {
  getVolumesMock.mockReset()
  getOptInMock.mockReset()
})

describe('getEnabledMediaIndexVolumeIds', () => {
  it('always includes the local root and adds opted-in network volumes', () => {
    getVolumesMock.mockReturnValue([
      { id: 'nas1', category: 'network' },
      { id: 'nas2', category: 'network' },
      { id: 'usb', category: 'removable' },
    ])
    getOptInMock.mockReturnValue(['nas1'])
    expect(getEnabledMediaIndexVolumeIds()).toEqual(['root', 'nas1'])
  })

  it('is just the local root when no network volume is opted in', () => {
    getVolumesMock.mockReturnValue([{ id: 'nas1', category: 'network' }])
    getOptInMock.mockReturnValue([])
    expect(getEnabledMediaIndexVolumeIds()).toEqual(['root'])
  })

  it('excludes a non-network volume even if its id is opted in', () => {
    getVolumesMock.mockReturnValue([{ id: 'usb', category: 'removable' }])
    getOptInMock.mockReturnValue(['usb'])
    expect(getEnabledMediaIndexVolumeIds()).toEqual(['root'])
  })
})
