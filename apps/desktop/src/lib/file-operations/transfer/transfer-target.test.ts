/** The single-item Copy/Move target the dialog resolves before preflight, on Windows paths (#1118). */
import { describe, it, expect, beforeEach, onTestFinished } from 'vitest'
import type { VolumeInfo } from '#lib/file-explorer/types.js'
import { withPlatform } from '#lib/platform/test-platform.js'
import { resolveTransferTarget } from './transfer-target'

const volumes: VolumeInfo[] = [
  { id: 'root', name: 'Local Disk (C:)', path: 'C:/', category: 'main_volume', isEjectable: false },
  { id: 'local-d', name: 'Backup (D:)', path: 'D:/', category: 'attached_volume', isEjectable: true },
  { id: 'mtp-1:65537', name: 'Phone', path: 'mtp://phone/65537', category: 'mobile_device', isEjectable: true },
]

function target(enteredPath: string, selectedVolumeId = 'root') {
  return resolveTransferTarget({
    enteredPath,
    sourceFolderPath: 'C:/Users/ada',
    sourcePath: 'C:/Users/ada/notes.txt',
    sourceVolumeId: 'root',
    selectedVolumeId,
    volumes,
    homePath: 'C:/Users/ada',
  })
}

describe('a Windows destination', () => {
  beforeEach(() => {
    onTestFinished(withPlatform('windows'))
  })

  it('lands a typed drive path on its own drive, never under the source folder', () => {
    expect(target('D:\\backup\\notes.txt')).toEqual({
      parent: 'D:/backup',
      name: 'notes.txt',
      volumeId: 'local-d',
      path: '/backup',
      fullPath: 'D:/backup/notes.txt',
    })
  })

  it('reads a drive path the same with either separator', () => {
    expect(target('C:/Users/ada/Documents/notes.txt')?.fullPath).toBe('C:/Users/ada/Documents/notes.txt')
    expect(target('c:\\Users\\ada\\Documents\\notes.txt')?.fullPath).toBe('C:/Users/ada/Documents/notes.txt')
  })

  it('takes a drive path while a phone is the picked volume', () => {
    expect(target('D:\\notes.txt', 'mtp-1:65537')).toMatchObject({ volumeId: 'local-d', fullPath: 'D:/notes.txt' })
  })

  it('takes a drive path for an item copied off a phone', () => {
    const fromPhone = resolveTransferTarget({
      enteredPath: 'D:\\photos\\IMG_1.JPG',
      sourceFolderPath: 'mtp://phone/65537/DCIM',
      sourcePath: 'mtp://phone/65537/DCIM/IMG_1.JPG',
      sourceVolumeId: 'mtp-1:65537',
      selectedVolumeId: 'mtp-1:65537',
      volumes,
      homePath: 'C:/Users/ada',
    })
    expect(fromPhone).toMatchObject({ volumeId: 'local-d', path: '/photos', fullPath: 'D:/photos/IMG_1.JPG' })
  })

  it('resolves a relative path from the source folder', () => {
    expect(target('..\\notes copy.txt')).toMatchObject({ volumeId: 'root', fullPath: 'C:/Users/notes copy.txt' })
  })

  it('puts the item into a folder the path ends in a separator at', () => {
    expect(target('D:\\backup\\')).toMatchObject({ name: 'notes.txt', fullPath: 'D:/backup/notes.txt' })
  })
})
