/**
 * Tests for the pure queued-enrichment predicate behind the indicator's
 * "image indexing starts after the drive scan" line.
 */
import { describe, it, expect } from 'vitest'
import { isEnrichQueued } from './media-enrich-queued'

describe('isEnrichQueued', () => {
  it('is queued while an eligible volume drive-indexes with no enrich pass yet', () => {
    // The reported case: "Index image contents" flipped on mid-full-scan.
    expect(isEnrichQueued(true, ['root'], ['root'], [])).toBe(true)
  })

  it('is not queued when image indexing is off', () => {
    expect(isEnrichQueued(false, ['root'], ['root'], [])).toBe(false)
  })

  it('is not queued for an ineligible volume (a USB stick is never enriched)', () => {
    expect(isEnrichQueued(true, ['root'], ['usb-stick'], [])).toBe(false)
  })

  it('is not queued once the volume is already enriching', () => {
    expect(isEnrichQueued(true, ['root'], ['root'], ['root'])).toBe(false)
  })

  it('is not queued with no drive-indexing volumes at all', () => {
    expect(isEnrichQueued(true, ['root'], [], [])).toBe(false)
  })

  it('queues off an opted-in SMB volume scanning while root idles', () => {
    expect(isEnrichQueued(true, ['root', 'smb-nas'], ['smb-nas'], [])).toBe(true)
  })
})
