import { describe, it, expect } from 'vitest'
import { sampleFolderNames } from './folder-sampler'

function names(prefix: string, count: number): string[] {
  return Array.from({ length: count }, (_, i) => `${prefix}-${String(i)}.txt`)
}

describe('sampleFolderNames', () => {
  it('returns [] on an empty folder', () => {
    expect(sampleFolderNames([], 0)).toEqual([])
  })

  it('returns all entries when below the small-folder threshold (200)', () => {
    const list = names('f', 200)
    const sample = sampleFolderNames(list, 50)
    expect(sample).toEqual(list)
  })

  it('returns all entries when exactly at the threshold', () => {
    const list = names('f', 100)
    const sample = sampleFolderNames(list, 50)
    expect(sample.length).toBe(100)
    expect(new Set(sample)).toEqual(new Set(list))
  })

  it('caps a large folder at the default 240 entries', () => {
    const list = names('f', 5000)
    const sample = sampleFolderNames(list, 2500)
    expect(sample.length).toBeLessThanOrEqual(240)
  })

  it('includes the first 200, a band around the cursor, and the tail', () => {
    const list = names('f', 5000)
    const sample = sampleFolderNames(list, 2500)
    // First slot is present.
    expect(sample).toContain('f-0.txt')
    // 199 is the last of the first bucket.
    expect(sample).toContain('f-199.txt')
    // Cursor band (cursor=2500, 20 entries centered on cursor: 2490..2509).
    expect(sample).toContain('f-2500.txt')
    expect(sample).toContain('f-2490.txt')
    expect(sample).toContain('f-2509.txt')
    // Tail (last 20: 4980..4999).
    expect(sample).toContain('f-4999.txt')
    expect(sample).toContain('f-4980.txt')
  })

  it('deduplicates overlapping buckets', () => {
    // Cursor in the first bucket: band overlaps with first-200; tail of a small-ish
    // 250-entry list overlaps too. Output must have no duplicates.
    const list = names('f', 250)
    const sample = sampleFolderNames(list, 50)
    const unique = new Set(sample)
    expect(sample.length).toBe(unique.size)
  })

  it('clamps the cursor band at the start without going negative', () => {
    const list = names('f', 5000)
    const sample = sampleFolderNames(list, 0)
    // Band would be [-10, 10); should clamp to [0, 10).
    expect(sample).toContain('f-0.txt')
    expect(sample).toContain('f-9.txt')
    expect(sample.length).toBeLessThanOrEqual(240)
  })

  it('clamps the cursor band at the end without overflowing', () => {
    const list = names('f', 5000)
    const sample = sampleFolderNames(list, 4999)
    expect(sample).toContain('f-4999.txt')
    expect(sample).toContain('f-4989.txt')
    expect(sample.length).toBeLessThanOrEqual(240)
  })

  it('handles negative or out-of-range cursorIndex without crashing', () => {
    const list = names('f', 5000)
    const sample = sampleFolderNames(list, -1)
    // Cursor band is skipped; first 200 + tail (with no overlap) → 220.
    expect(sample.length).toBeLessThanOrEqual(240)
    expect(sample).toContain('f-0.txt')
    expect(sample).toContain('f-4999.txt')
  })

  it('respects a custom max cap', () => {
    const list = names('f', 5000)
    const sample = sampleFolderNames(list, 2500, 50)
    expect(sample.length).toBe(50)
  })
})
