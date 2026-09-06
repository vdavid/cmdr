import { describe, expect, it } from 'vitest'
import { filesystemLabel } from './filesystem-label'
import type { VolumeInfo } from '../types'

function vol(overrides: Partial<VolumeInfo>): VolumeInfo {
  return {
    id: 'v',
    name: 'Vol',
    path: '/Volumes/Vol',
    category: 'attached_volume',
    isEjectable: true,
    ...overrides,
  }
}

describe('filesystemLabel', () => {
  it('maps real-volume filesystem types to display names', () => {
    expect(filesystemLabel(vol({ fsType: 'msdos' }))).toBe('FAT32')
    expect(filesystemLabel(vol({ fsType: 'vfat' }))).toBe('FAT32')
    expect(filesystemLabel(vol({ fsType: 'exfat' }))).toBe('exFAT')
    expect(filesystemLabel(vol({ fsType: 'ntfs' }))).toBe('NTFS')
    expect(filesystemLabel(vol({ category: 'main_volume', fsType: 'apfs' }))).toBe('APFS')
  })

  it('is case-insensitive on the raw type', () => {
    expect(filesystemLabel(vol({ fsType: 'MSDOS' }))).toBe('FAT32')
  })

  it('shows nothing for disk images, cloud drives, MTP, network, favorites', () => {
    expect(filesystemLabel(vol({ fsType: 'hfs', isDiskImage: true }))).toBeNull()
    expect(filesystemLabel(vol({ category: 'cloud_drive', fsType: 'apfs' }))).toBeNull()
    expect(filesystemLabel(vol({ category: 'mobile_device' }))).toBeNull()
    expect(filesystemLabel(vol({ category: 'network' }))).toBeNull()
    expect(filesystemLabel(vol({ category: 'favorite', fsType: 'apfs' }))).toBeNull()
  })

  it('shows nothing for SMB shares (backing filesystem unseen) or unknown/absent types', () => {
    expect(filesystemLabel(vol({ fsType: 'smbfs' }))).toBeNull()
    expect(filesystemLabel(vol({ fsType: 'cifs' }))).toBeNull()
    expect(filesystemLabel(vol({ fsType: 'zalgofs' }))).toBeNull()
    expect(filesystemLabel(vol({ fsType: undefined }))).toBeNull()
  })
})

describe('filesystemLabel: remote places', () => {
  it('names the protocol for a server row, so the slot says what the row speaks', () => {
    expect(filesystemLabel(vol({ category: 'network', fsType: 'sftp' }))).toBe('SFTP')
    expect(filesystemLabel(vol({ category: 'network', fsType: 'webdav' }))).toBe('WebDAV')
    expect(filesystemLabel(vol({ category: 'network', fsType: 'smbfs' }))).toBe('SMB')
    expect(filesystemLabel(vol({ category: 'network', fsType: 'cifs' }))).toBe('SMB')
  })

  it('is case-insensitive there too, and stays quiet on an unknown protocol', () => {
    expect(filesystemLabel(vol({ category: 'network', fsType: 'SFTP' }))).toBe('SFTP')
    expect(filesystemLabel(vol({ category: 'network', fsType: 'gopher' }))).toBeNull()
  })

  it('leaves a local disk alone', () => {
    // The protocol names are a NETWORK-row answer. A local disk that somehow
    // reports `smbfs` is the SMB-mount case the local table already refuses.
    expect(filesystemLabel(vol({ fsType: 'sftp' }))).toBeNull()
  })
})
