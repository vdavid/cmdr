import { describe, it, expect } from 'vitest'
import type { VolumeInfo } from '$lib/file-explorer/types'
import { resolveImageSearchVolume } from './active-media-volume'

function vol(overrides: Partial<VolumeInfo> & Pick<VolumeInfo, 'id' | 'path' | 'category'>): VolumeInfo {
  return {
    name: overrides.id,
    isEjectable: false,
    ...overrides,
  }
}

const ROOT = vol({ id: 'root', path: '/', category: 'main_volume' })
const NAS = vol({ id: 'smb-naspi', path: '/Volumes/naspi', category: 'network' })
const USB = vol({ id: 'volumesusb', path: '/Volumes/USB', category: 'attached_volume' })

describe('resolveImageSearchVolume', () => {
  it('targets the local root volume with mount root "/" and no network voice', () => {
    expect(resolveImageSearchVolume([ROOT, NAS], 'root')).toEqual({
      volumeId: 'root',
      mountRoot: '/',
      isNetwork: false,
    })
  })

  it('targets a network (SMB) volume: its id, its mount root, and the network voice', () => {
    // The NAS case the whole feature exists for: a user browsing /Volumes/naspi
    // must search the NAS's media index, and hits must resolve under its mount root.
    expect(resolveImageSearchVolume([ROOT, NAS], 'smb-naspi')).toEqual({
      volumeId: 'smb-naspi',
      mountRoot: '/Volumes/naspi',
      isNetwork: true,
    })
  })

  it('targets a local attached (USB) volume with its mount root but the local voice', () => {
    // An external local drive enriches by default (not opt-in), so it must NOT get the
    // network coverage voice, but its hits are still mount-relative to its mount point.
    expect(resolveImageSearchVolume([ROOT, USB], 'volumesusb')).toEqual({
      volumeId: 'volumesusb',
      mountRoot: '/Volumes/USB',
      isNetwork: false,
    })
  })

  it('falls back to the local root when the focused volume is not in the list', () => {
    // A `search-results://` snapshot pane (or a since-unmounted volume) has no
    // `media.db`, so the local root is the sensible default.
    expect(resolveImageSearchVolume([ROOT, NAS], 'search-results')).toEqual({
      volumeId: 'root',
      mountRoot: '/',
      isNetwork: false,
    })
  })

  it('falls back to the local root when the volume list is empty', () => {
    expect(resolveImageSearchVolume([], 'smb-naspi')).toEqual({
      volumeId: 'root',
      mountRoot: '/',
      isNetwork: false,
    })
  })
})
