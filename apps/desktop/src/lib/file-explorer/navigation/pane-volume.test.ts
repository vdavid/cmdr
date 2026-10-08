import { describe, it, expect } from 'vitest'
import type { VolumeInfo } from '../types'
import { paneVolumeOf, volumeMountedAt } from './pane-volume'

const root: VolumeInfo = { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }
const share: VolumeInfo = {
  id: 'smb-p',
  name: 'public on localhost:11480',
  path: '/Volumes/public-1',
  category: 'attached_volume',
  isEjectable: false,
  connectionState: 'direct',
}

describe('paneVolumeOf', () => {
  it('takes a live share by id while the pane stands in it', () => {
    expect(paneVolumeOf([root, share], 'smb-p', '/Volumes/public-1/docs', 'root')?.id).toBe('smb-p')
  })

  it('asks the path when the pane stands outside its own volume', () => {
    // A pane whose path left its share's mount point says where it really is.
    expect(paneVolumeOf([root, share], 'smb-p', '/Volumes/public', 'root')?.id).toBe('root')
  })

  it('asks the path for the boot disk, which holds every other drive', () => {
    const usb: VolumeInfo = {
      id: 'usb',
      name: 'USB',
      path: '/Volumes/USB',
      category: 'attached_volume',
      isEjectable: true,
    }
    expect(paneVolumeOf([root, usb], 'root', '/Volumes/USB/a', 'usb')?.id).toBe('usb')
  })
})

describe('volumeMountedAt', () => {
  it('skips a favorite pointing at the same mount root', () => {
    // Favorites list first, so a plain path lookup answers with the favorite
    // and an eject never moves the pane off the drive.
    const favorite: VolumeInfo = {
      id: 'fav-1',
      name: 'public',
      path: '/Volumes/public-1',
      category: 'favorite',
      isEjectable: false,
    }
    expect(volumeMountedAt([favorite, root, share], '/Volumes/public-1')?.id).toBe('smb-p')
  })

  it('answers nothing for a path no volume is mounted at', () => {
    expect(volumeMountedAt([root, share], '/Volumes/gone')).toBeUndefined()
  })
})
