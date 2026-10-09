import { describe, expect, it } from 'vitest'
import { volumeScheme } from './volume-scheme'

describe('volumeScheme', () => {
  it('reads every minted scheme off its prefix', () => {
    expect(volumeScheme('vol-macintosh-hd-0123456789abcdef')).toBe('local')
    expect(volumeScheme('path-volumes-usb-0123456789abcdef')).toBe('path')
    expect(volumeScheme('smb-naspi-media-0123456789abcdef')).toBe('smb')
    expect(volumeScheme('sftp-box-david-0123456789abcdef')).toBe('sftp')
    expect(volumeScheme('webdav-dav-host-0123456789abcdef')).toBe('webdav')
    expect(volumeScheme('s3-bucket-0123456789abcdef')).toBe('s3')
    expect(volumeScheme('adb-pixel-7-a1b2c3d')).toBe('adb')
    expect(volumeScheme('cloud-dropbox')).toBe('cloud')
    expect(volumeScheme('fav-42')).toBe('favorite')
  })

  it('matches the boot volume only as the exact `root` literal', () => {
    expect(volumeScheme('root')).toBe('root')
    expect(volumeScheme('root-ish')).toBe('unknown')
  })

  it('classifies both the MTP device id and its storage volume id as mtp', () => {
    expect(volumeScheme('mtp-336592896')).toBe('mtp')
    expect(volumeScheme('mtp-336592896:65537')).toBe('mtp')
  })

  it('treats virtual, legacy, and empty ids as unknown', () => {
    expect(volumeScheme('network')).toBe('unknown')
    expect(volumeScheme('search-results')).toBe('unknown')
    // The pre-`mtp-` colon shape: a colon alone no longer says MTP.
    expect(volumeScheme('0-5:65537')).toBe('unknown')
    expect(volumeScheme('Macintosh HD')).toBe('unknown')
    expect(volumeScheme('')).toBe('unknown')
  })

  it('needs the dash: a bare tag is not a scheme', () => {
    expect(volumeScheme('smb')).toBe('unknown')
    expect(volumeScheme('mtp')).toBe('unknown')
    expect(volumeScheme('adb-')).toBe('unknown')
  })
})
