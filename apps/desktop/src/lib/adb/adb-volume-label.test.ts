import { describe, expect, it, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { VolumeInfo } from '$lib/file-explorer/types'
import { deviceVolumeLabel } from './adb-volume-label'

// The suffixed label resolves through the i18n catalog (`tString`); pin the
// base locale so the asserted en copy is deterministic.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

function volume(overrides: Partial<VolumeInfo> & Pick<VolumeInfo, 'id' | 'name'>): VolumeInfo {
  return {
    path: `mtp://${overrides.id}`,
    category: 'mobile_device',
    isEjectable: true,
    mountIsReadOnly: false,
    ...overrides,
  }
}

const ADB_PIXEL = volume({ id: 'adb-R58M12345', name: 'Pixel 9', path: 'adb://R58M12345', fsType: 'adb' })
const MTP_PIXEL = volume({ id: 'mtp-dev:65537', name: 'Pixel 9' })

describe('deviceVolumeLabel', () => {
  it('suffixes the ADB entry when an MTP twin shares its name', () => {
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL, MTP_PIXEL])).toBe('Pixel 9 (ADB)')
  })

  it('leaves a phone reachable only over ADB with its plain name', () => {
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL])).toBe('Pixel 9')
  })

  it('never suffixes the MTP half, even when the twin is there', () => {
    expect(deviceVolumeLabel(MTP_PIXEL, [ADB_PIXEL, MTP_PIXEL])).toBe('Pixel 9')
  })

  it('does not pair devices with different names', () => {
    const otherPhone = volume({ id: 'mtp-dev:65538', name: 'Galaxy S24' })
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL, otherPhone])).toBe('Pixel 9')
  })

  it('does not treat a second ADB volume as an MTP twin', () => {
    const adbTwin = volume({ id: 'adb-OTHER', name: 'Pixel 9', path: 'adb://OTHER', fsType: 'adb' })
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL, adbTwin])).toBe('Pixel 9')
  })

  it('does not pair with a same-named volume that is not a device', () => {
    const disk = volume({ id: 'ext', name: 'Pixel 9', path: '/Volumes/Pixel 9', category: 'attached_volume' })
    expect(deviceVolumeLabel(ADB_PIXEL, [ADB_PIXEL, disk])).toBe('Pixel 9')
  })
})
