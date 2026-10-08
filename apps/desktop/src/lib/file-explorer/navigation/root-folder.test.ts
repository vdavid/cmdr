import { describe, it, expect, vi } from 'vitest'
import { resolvePathVolume } from '$lib/tauri-commands'
import type { VolumeInfo } from '../types'
import { goToRootFolder, rootFolderOf } from './root-folder'

vi.mock('$lib/tauri-commands', () => ({ resolvePathVolume: vi.fn() }))

const root: VolumeInfo = { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }
const usb: VolumeInfo = { id: 'usb', name: 'USB', path: '/Volumes/USB', category: 'attached_volume', isEjectable: true }
const phone: VolumeInfo = {
  id: 'mtp-1',
  name: 'Pixel',
  path: 'mtp://pixel/65537',
  category: 'mobile_device',
  isEjectable: true,
}
const volumes = [root, usb, phone]

describe('rootFolderOf', () => {
  it('takes the boot disk to /', () => {
    expect(rootFolderOf({ volumes, volumeId: 'root', path: '/Users/david/Library', containingVolumeId: 'root' })).toBe(
      '/',
    )
  })

  it('takes a drive the pane walked into from the boot disk to that drive’s root', () => {
    // The header says "USB" here, so / would land somewhere the user didn't see.
    expect(rootFolderOf({ volumes, volumeId: 'root', path: '/Volumes/USB/photos', containingVolumeId: 'usb' })).toBe(
      '/Volumes/USB',
    )
  })

  it('takes a phone to its own root', () => {
    expect(rootFolderOf({ volumes, volumeId: 'mtp-1', path: 'mtp://pixel/65537/DCIM', containingVolumeId: null })).toBe(
      'mtp://pixel/65537',
    )
  })

  it('falls back to the pane’s own volume while the containing one is unknown', () => {
    expect(rootFolderOf({ volumes, volumeId: 'root', path: '/Users/david', containingVolumeId: null })).toBe('/')
  })

  it('takes a pane inside an archive to the archive’s root, not the drive’s', () => {
    expect(
      rootFolderOf({ volumes, volumeId: 'root', path: '/Users/david/a.zip/inner/deep', containingVolumeId: 'root' }),
    ).toBe('/Users/david/a.zip')
  })

  it('resolves a nested archive against the outer one', () => {
    expect(
      rootFolderOf({ volumes, volumeId: 'usb', path: '/Volumes/USB/x.tar/y.zip/z', containingVolumeId: 'usb' }),
    ).toBe('/Volumes/USB/x.tar')
  })

  it('has no root for the servers hub or a search-results snapshot', () => {
    expect(rootFolderOf({ volumes, volumeId: 'network', path: 'smb://nas', containingVolumeId: null })).toBeNull()
    expect(
      rootFolderOf({ volumes, volumeId: 'search-results', path: 'search-results://abc', containingVolumeId: null }),
    ).toBeNull()
  })

  it('keeps a share pane that wandered off its mount on the share’s own root', () => {
    // `/` isn't on the share's volume, so the pane would drop that listing as foreign.
    const share: VolumeInfo = {
      id: 'smb-p',
      name: 'public',
      path: '/Volumes/public-1',
      category: 'attached_volume',
      isEjectable: false,
      connectionState: 'direct',
    }
    expect(
      rootFolderOf({ volumes: [root, share], volumeId: 'smb-p', path: '/Volumes/public', containingVolumeId: 'root' }),
    ).toBe('/Volumes/public-1')
  })

  it('has no root for a volume it can’t find', () => {
    expect(rootFolderOf({ volumes, volumeId: 'gone', path: '/Volumes/Gone/a', containingVolumeId: null })).toBeNull()
  })
})

describe('goToRootFolder', () => {
  function setup(pane: { volumeId: string; path: string }) {
    const navigate = vi.fn()
    const deps = {
      getVolumes: () => volumes,
      getPaneVolumeId: () => pane.volumeId,
      getPanePath: () => pane.path,
      navigate,
    }
    return { deps, navigate }
  }

  it('opens the root in place, on the pane’s own volume', async () => {
    vi.mocked(resolvePathVolume).mockResolvedValue({ volume: root, timedOut: false })
    const { deps, navigate } = setup({ volumeId: 'root', path: '/Users/david' })
    await goToRootFolder(deps, 'right')
    expect(navigate).toHaveBeenCalledWith({
      pane: 'right',
      to: { goTo: { volumeId: 'root', path: '/' } },
      source: 'user',
    })
  })

  it('does nothing at the root already', async () => {
    vi.mocked(resolvePathVolume).mockResolvedValue({ volume: root, timedOut: false })
    const { deps, navigate } = setup({ volumeId: 'root', path: '/' })
    await goToRootFolder(deps, 'left')
    expect(navigate).not.toHaveBeenCalled()
  })

  it('does nothing when the pane moved on while the backend answered', async () => {
    const pane = { volumeId: 'root', path: '/Users/david' }
    vi.mocked(resolvePathVolume).mockImplementation(() => {
      pane.path = '/Users/david/Downloads'
      return Promise.resolve({ volume: root, timedOut: false })
    })
    const { deps, navigate } = setup(pane)
    await goToRootFolder(deps, 'left')
    expect(navigate).not.toHaveBeenCalled()
  })
})
