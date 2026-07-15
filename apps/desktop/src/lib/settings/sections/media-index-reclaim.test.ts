import { describe, expect, it } from 'vitest'

import { RECLAIM_MIN_EXCESS, RECLAIM_MIN_FRACTION, shouldOfferReclaim } from './media-index-reclaim'

describe('shouldOfferReclaim', () => {
  it('offers when the doomed set clears both the absolute and the relative floor', () => {
    // 199,850 doomed of 200,000 stored: way past 100 rows and past 5%.
    expect(shouldOfferReclaim(200_000, 199_850)).toBe(true)
  })

  it('stays hidden for a tiny leftover on a huge index', () => {
    // 50 doomed rows: below the absolute floor even though the index is large.
    expect(shouldOfferReclaim(1_000_000, 50)).toBe(false)
  })

  it('stays hidden when the doomed fraction is under 5%, even past the row floor', () => {
    // 150 doomed (> 100) but only 1.5% of 10,000 stored.
    expect(shouldOfferReclaim(10_000, 150)).toBe(false)
  })

  it('requires strictly MORE than both floors', () => {
    // Exactly at the absolute floor doesn't offer.
    expect(shouldOfferReclaim(10_000, RECLAIM_MIN_EXCESS)).toBe(false)
    // Exactly at the relative floor doesn't offer (5% of 4,000 = 200, doomed 200).
    expect(shouldOfferReclaim(4_000, RECLAIM_MIN_FRACTION * 4_000)).toBe(false)
  })

  it('offers when doomed is a large fraction of a modest index', () => {
    // 300 doomed of 1,000: past 100 rows and 30% > 5%.
    expect(shouldOfferReclaim(1_000, 300)).toBe(true)
  })

  it('never offers when nothing is doomed', () => {
    expect(shouldOfferReclaim(200_000, 0)).toBe(false)
  })
})
