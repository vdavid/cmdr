import { describe, expect, it } from 'vitest'
import { AGE_THRESHOLDS_MS, tierClassForAge } from './age-tier-utils'

const NOW_MS = Date.parse('2026-05-11T12:00:00Z')
const toSec = (ms: number) => ms / 1000

describe('tierClassForAge', () => {
  it('returns null for missing or non-finite timestamps', () => {
    expect(tierClassForAge(null, NOW_MS)).toBeNull()
    expect(tierClassForAge(undefined, NOW_MS)).toBeNull()
    expect(tierClassForAge(Number.NaN, NOW_MS)).toBeNull()
    expect(tierClassForAge(Number.POSITIVE_INFINITY, NOW_MS)).toBeNull()
  })

  it('clamps future-dated files to age-fresh', () => {
    const future = toSec(NOW_MS + 7 * 24 * 60 * 60 * 1000)
    expect(tierClassForAge(future, NOW_MS)).toBe('age-fresh')
  })

  it('returns age-fresh just under the 1-month boundary', () => {
    const justUnder = toSec(NOW_MS - AGE_THRESHOLDS_MS.fresh + 1000)
    expect(tierClassForAge(justUnder, NOW_MS)).toBe('age-fresh')
  })

  it('crosses to age-recent at the 1-month boundary', () => {
    const atBoundary = toSec(NOW_MS - AGE_THRESHOLDS_MS.fresh)
    expect(tierClassForAge(atBoundary, NOW_MS)).toBe('age-recent')
  })

  it('returns age-recent for files older than 1 month but under 1 year', () => {
    const sixMonths = toSec(NOW_MS - 180 * 24 * 60 * 60 * 1000)
    expect(tierClassForAge(sixMonths, NOW_MS)).toBe('age-recent')
  })

  it('crosses to age-aging at the 1-year boundary', () => {
    const atBoundary = toSec(NOW_MS - AGE_THRESHOLDS_MS.recent)
    expect(tierClassForAge(atBoundary, NOW_MS)).toBe('age-aging')
  })

  it('crosses to age-old at the 2-year boundary', () => {
    const atBoundary = toSec(NOW_MS - AGE_THRESHOLDS_MS.aging)
    expect(tierClassForAge(atBoundary, NOW_MS)).toBe('age-old')
  })

  it('crosses to age-ancient at the 3-year boundary', () => {
    const atBoundary = toSec(NOW_MS - AGE_THRESHOLDS_MS.old)
    expect(tierClassForAge(atBoundary, NOW_MS)).toBe('age-ancient')
  })

  it('returns age-ancient for very old files', () => {
    const tenYears = toSec(NOW_MS - 10 * 365 * 24 * 60 * 60 * 1000)
    expect(tierClassForAge(tenYears, NOW_MS)).toBe('age-ancient')
  })
})
