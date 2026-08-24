import { describe, expect, it } from 'vitest'

import { itemCountBucket } from './analytics'

describe('itemCountBucket', () => {
  it('matches the backend ladder exactly', () => {
    // Mirrors `item_count_buckets_map_to_coarse_ranges` in
    // `apps/desktop/src-tauri/src/analytics/mod.rs`. Every boundary is here on
    // purpose: one product with two ideas of what "a lot" means is worse than no
    // bucketing at all, and nothing else would catch a one-off drift.
    expect(itemCountBucket(0)).toBe('0')
    expect(itemCountBucket(1)).toBe('1')
    expect(itemCountBucket(2)).toBe('2-10')
    expect(itemCountBucket(10)).toBe('2-10')
    expect(itemCountBucket(11)).toBe('11-100')
    expect(itemCountBucket(100)).toBe('11-100')
    expect(itemCountBucket(101)).toBe('101-1000')
    expect(itemCountBucket(1000)).toBe('101-1000')
    expect(itemCountBucket(1001)).toBe('1000+')
    expect(itemCountBucket(50_000)).toBe('1000+')
  })

  it('treats a negative count as zero rather than inventing a bucket', () => {
    // The Rust twin takes a `usize`, so it can't be handed one; TypeScript can.
    expect(itemCountBucket(-1)).toBe('0')
  })
})
