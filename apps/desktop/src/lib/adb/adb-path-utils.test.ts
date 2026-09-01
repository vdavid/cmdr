import { describe, it, expect } from 'vitest'
import {
  constructAdbPath,
  getAdbDisplayPath,
  getAdbParentPath,
  getDeviceDisplayPath,
  isAdbPath,
  isAdbVolumeId,
  isDeviceScheme,
  isDeviceVolumeId,
  joinAdbPath,
  parseAdbPath,
} from './adb-path-utils'

describe('parseAdbPath', () => {
  it('parses the device root', () => {
    expect(parseAdbPath('adb://R58M12345')).toEqual({ serial: 'R58M12345', path: '' })
    expect(parseAdbPath('adb://R58M12345/')).toEqual({ serial: 'R58M12345', path: '' })
  })

  it('parses a folder on the device', () => {
    expect(parseAdbPath('adb://R58M12345/sdcard/DCIM')).toEqual({ serial: 'R58M12345', path: 'sdcard/DCIM' })
  })

  it('keeps a TCP serial (host:port) intact', () => {
    expect(parseAdbPath('adb://192.168.1.5:5555/sdcard')).toEqual({ serial: '192.168.1.5:5555', path: 'sdcard' })
  })

  it('returns null for a non-ADB path or a missing serial', () => {
    expect(parseAdbPath('/Users/me')).toBeNull()
    expect(parseAdbPath('mtp://0-5/65537')).toBeNull()
    expect(parseAdbPath('adb://')).toBeNull()
  })
})

describe('constructAdbPath', () => {
  it('builds the root without a trailing slash', () => {
    expect(constructAdbPath('R58M12345')).toBe('adb://R58M12345')
    expect(constructAdbPath('R58M12345', '/')).toBe('adb://R58M12345')
  })

  it('accepts absolute and relative device paths alike (root-anchored is idempotent)', () => {
    expect(constructAdbPath('R58M12345', '/sdcard/DCIM')).toBe('adb://R58M12345/sdcard/DCIM')
    expect(constructAdbPath('R58M12345', 'sdcard/DCIM')).toBe('adb://R58M12345/sdcard/DCIM')
  })

  it('round-trips through parseAdbPath', () => {
    const parsed = parseAdbPath('adb://R58M12345/data/local/tmp')
    if (!parsed) throw new Error('expected an ADB path to parse')
    expect(constructAdbPath(parsed.serial, parsed.path)).toBe('adb://R58M12345/data/local/tmp')
  })
})

describe('getAdbParentPath', () => {
  it('walks up one level and stops at the device root', () => {
    expect(getAdbParentPath('adb://R58M12345/sdcard/DCIM')).toBe('adb://R58M12345/sdcard')
    expect(getAdbParentPath('adb://R58M12345/sdcard')).toBe('adb://R58M12345')
    expect(getAdbParentPath('adb://R58M12345')).toBeNull()
  })

  it('returns null for a non-ADB path', () => {
    expect(getAdbParentPath('/Users/me')).toBeNull()
  })
})

describe('joinAdbPath', () => {
  it('appends a child at the root and below it', () => {
    expect(joinAdbPath('adb://R58M12345', 'sdcard')).toBe('adb://R58M12345/sdcard')
    expect(joinAdbPath('adb://R58M12345/sdcard', 'DCIM')).toBe('adb://R58M12345/sdcard/DCIM')
  })

  it('leaves a non-ADB base untouched', () => {
    expect(joinAdbPath('/Users/me', 'x')).toBe('/Users/me')
  })
})

describe('getAdbDisplayPath', () => {
  it('shows the absolute device path, "/" at the root', () => {
    expect(getAdbDisplayPath('adb://R58M12345')).toBe('/')
    expect(getAdbDisplayPath('adb://R58M12345/sdcard/DCIM')).toBe('/sdcard/DCIM')
  })

  it('passes a non-ADB path through', () => {
    expect(getAdbDisplayPath('/Users/me')).toBe('/Users/me')
  })
})

describe('the device-family predicates', () => {
  it('isAdbVolumeId matches only the adb- prefix', () => {
    expect(isAdbVolumeId('adb-pixel-7-a1b2c3d')).toBe(true)
    expect(isAdbVolumeId('mtp-336592896:65537')).toBe(false)
    expect(isAdbVolumeId('root')).toBe(false)
  })

  it('isAdbPath matches only the adb:// scheme', () => {
    expect(isAdbPath('adb://R58M12345/sdcard')).toBe(true)
    expect(isAdbPath('mtp://0-5/65537')).toBe(false)
  })

  it('isDeviceVolumeId covers both MTP and ADB ids and nothing local', () => {
    expect(isDeviceVolumeId('adb-pixel-7-a1b2c3d')).toBe(true)
    expect(isDeviceVolumeId('mtp-336592896:65537')).toBe(true)
    expect(isDeviceVolumeId('mtp-336592896')).toBe(true)
    expect(isDeviceVolumeId('root')).toBe(false)
    expect(isDeviceVolumeId('smb-host-share')).toBe(false)
  })

  it('isDeviceScheme covers both schemes', () => {
    expect(isDeviceScheme('adb://R58M12345')).toBe(true)
    expect(isDeviceScheme('mtp://0-5/65537')).toBe(true)
    expect(isDeviceScheme('smb://host/share')).toBe(false)
    expect(isDeviceScheme('/Volumes/USB')).toBe(false)
  })

  it('getDeviceDisplayPath dispatches per scheme and passes the rest through', () => {
    expect(getDeviceDisplayPath('mtp://0-5/65537')).toBe('/')
    expect(getDeviceDisplayPath('mtp://0-5/65537/DCIM/Camera')).toBe('/DCIM/Camera')
    expect(getDeviceDisplayPath('adb://R58M12345/sdcard/DCIM')).toBe('/sdcard/DCIM')
    expect(getDeviceDisplayPath('/Users/me')).toBe('/Users/me')
  })
})
