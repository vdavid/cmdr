import { describe, expect, it } from 'vitest'
import type { VolumeInfo } from '$lib/file-explorer/types'
import { findVolumeIdForPath, isSameVolume, pickDropOperation } from './drop-operation'

const root: VolumeInfo = { id: 'boot', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }
const usb: VolumeInfo = {
  id: 'usb',
  name: 'MyDrive',
  path: '/Volumes/MyDrive',
  category: 'attached_volume',
  isEjectable: true,
}
const usbPrefix: VolumeInfo = {
  id: 'usbprefix',
  name: 'MyDrive2',
  path: '/Volumes/MyDrive2',
  category: 'attached_volume',
  isEjectable: true,
}
const volumes: VolumeInfo[] = [root, usb, usbPrefix]

// Favorites are picker-only pseudo-volumes living on the local fs (their root is
// a real local path). They must never be treated as their own volume for
// same-volume detection — a Desktop→Documents drag is local→local (Move), not
// cross-volume (Copy).
const favDesktop: VolumeInfo = {
  id: 'fav-desktop',
  name: 'Desktop',
  path: '/Users/me/Desktop',
  category: 'favorite',
  isEjectable: false,
}
const favDocuments: VolumeInfo = {
  id: 'fav-documents',
  name: 'Documents',
  path: '/Users/me/Documents',
  category: 'favorite',
  isEjectable: false,
}
const volumesWithFavorites: VolumeInfo[] = [root, favDesktop, favDocuments, usb]

const noMods = { altHeld: false, cmdHeld: false, shiftHeld: false }

describe('findVolumeIdForPath', () => {
  it('matches the longest-prefix volume', () => {
    expect(findVolumeIdForPath('/Volumes/MyDrive/foo.txt', volumes)).toBe('usb')
  })

  it('falls back to root for paths not under any mount', () => {
    expect(findVolumeIdForPath('/Users/dave/notes.md', volumes)).toBe('boot')
  })

  it('does not match a sibling whose name is a prefix', () => {
    // /Volumes/MyDrive2/foo must not match the /Volumes/MyDrive volume
    expect(findVolumeIdForPath('/Volumes/MyDrive2/foo', volumes)).toBe('usbprefix')
  })

  it('returns null when no volume matches', () => {
    expect(findVolumeIdForPath('/Users/dave', [usb])).toBeNull()
  })

  it('matches a path equal to the volume path itself', () => {
    expect(findVolumeIdForPath('/Volumes/MyDrive', volumes)).toBe('usb')
  })
})

describe('isSameVolume', () => {
  it('returns true for two paths on the boot volume', () => {
    expect(isSameVolume('/Users/a', '/Users/b/c', volumes)).toBe(true)
  })

  it('returns false for cross-volume paths', () => {
    expect(isSameVolume('/Users/a', '/Volumes/MyDrive/x', volumes)).toBe(false)
  })

  it('returns false when the source resolves to no volume', () => {
    expect(isSameVolume('/orphan', '/Users/a', [usb])).toBe(false)
  })

  it('treats two local favorites as the SAME volume (both resolve to root, not fav-*)', () => {
    // Desktop→Documents: both live on the local root. A naive longest-prefix
    // match would pick fav-desktop vs fav-documents → cross-volume → Copy. The
    // badge (and the actual drop) must show Move: it's a local→local move.
    expect(isSameVolume('/Users/me/Desktop/photo.jpg', '/Users/me/Documents', volumesWithFavorites)).toBe(true)
  })

  it('treats a favorite path and a plain root path as the same volume', () => {
    expect(isSameVolume('/Users/me/Desktop/x', '/Users/me/notes.md', volumesWithFavorites)).toBe(true)
  })
})

describe('pickDropOperation', () => {
  const baseOpts = {
    sourcePath: '/Users/a/file.txt',
    targetPath: '/Users/b',
    volumes,
  }

  it('defaults to Move when source and target share a volume', () => {
    expect(pickDropOperation({ ...baseOpts, modifiers: noMods })).toBe('move')
  })

  it('defaults to Copy when source and target are on different volumes', () => {
    expect(
      pickDropOperation({
        ...baseOpts,
        targetPath: '/Volumes/MyDrive/dst',
        modifiers: noMods,
      }),
    ).toBe('copy')
  })

  it('Alt forces Copy even on same-volume drops', () => {
    expect(
      pickDropOperation({
        ...baseOpts,
        modifiers: { altHeld: true, cmdHeld: false, shiftHeld: false },
      }),
    ).toBe('copy')
  })

  it('Cmd forces Move even on cross-volume drops', () => {
    expect(
      pickDropOperation({
        ...baseOpts,
        targetPath: '/Volumes/MyDrive/dst',
        modifiers: { altHeld: false, cmdHeld: true, shiftHeld: false },
      }),
    ).toBe('move')
  })

  it('Shift forces Move even on cross-volume drops', () => {
    expect(
      pickDropOperation({
        ...baseOpts,
        targetPath: '/Volumes/MyDrive/dst',
        modifiers: { altHeld: false, cmdHeld: false, shiftHeld: true },
      }),
    ).toBe('move')
  })

  it('Alt beats Cmd when both are held (force-Copy wins)', () => {
    expect(
      pickDropOperation({
        ...baseOpts,
        modifiers: { altHeld: true, cmdHeld: true, shiftHeld: false },
      }),
    ).toBe('copy')
  })

  it('falls back to Copy when source path is missing', () => {
    expect(
      pickDropOperation({
        sourcePath: null,
        targetPath: '/Users/b',
        volumes,
        modifiers: noMods,
      }),
    ).toBe('copy')
  })

  it('falls back to Copy when target path is missing', () => {
    expect(
      pickDropOperation({
        sourcePath: '/Users/a',
        targetPath: null,
        volumes,
        modifiers: noMods,
      }),
    ).toBe('copy')
  })

  it('picks Move for a Desktop→Documents drag (both local, favorites must not read as cross-volume)', () => {
    expect(
      pickDropOperation({
        sourcePath: '/Users/me/Desktop/photo.jpg',
        targetPath: '/Users/me/Documents',
        volumes: volumesWithFavorites,
        modifiers: noMods,
      }),
    ).toBe('move')
  })
})
