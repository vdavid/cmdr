/**
 * Unit tests for the pure volume-kind classifier used by the per-pane tint.
 */

import { describe, it, expect } from 'vitest'
import { volumeKindFor } from './volume-tint.svelte'

describe('volumeKindFor', () => {
  it('classifies the root local drive as local', () => {
    expect(volumeKindFor('root', 'apfs', 'main_volume')).toBe('local')
  })

  it('classifies an attached external as local', () => {
    expect(volumeKindFor('attached-1', 'exfat', 'attached_volume')).toBe('local')
  })

  it('classifies a cloud drive as local', () => {
    expect(volumeKindFor('icloud', 'apfs', 'cloud_drive')).toBe('local')
  })

  it('classifies a network-category volume as smb', () => {
    expect(volumeKindFor('volumesnaspi', 'smbfs', 'network')).toBe('smb')
  })

  it('classifies smbfs fsType without category as smb', () => {
    expect(volumeKindFor('some-id', 'smbfs', undefined)).toBe('smb')
  })

  it('classifies an MTP storage id as mtp', () => {
    expect(volumeKindFor('mtp-336592896:65537', undefined, 'mobile_device')).toBe('mtp')
  })

  it('classifies a device-only MTP id (with colon) as mtp', () => {
    expect(volumeKindFor('0-5:65537', undefined, undefined)).toBe('mtp')
  })

  it('classifies a "mtp-" prefixed id without colon as mtp', () => {
    expect(volumeKindFor('mtp-336592896', undefined, 'mobile_device')).toBe('mtp')
  })

  it('classifies favorites and synthetic browsers as other', () => {
    expect(volumeKindFor('network', undefined, 'favorite')).toBe('other')
  })

  it('classifies unknown ids with no metadata as other', () => {
    expect(volumeKindFor('mystery', undefined, undefined)).toBe('other')
  })
})
