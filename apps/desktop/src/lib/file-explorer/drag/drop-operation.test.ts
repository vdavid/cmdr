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
})
