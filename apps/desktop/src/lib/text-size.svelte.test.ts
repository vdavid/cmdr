import { describe, it, expect } from 'vitest'
import { compoundScale } from '$lib/text-size.svelte'

describe('compoundScale', () => {
  it('returns the user fraction at system multiplier 1', () => {
    expect(compoundScale(1, 100)).toBe(1)
    expect(compoundScale(1, 50)).toBeCloseTo(0.5)
    expect(compoundScale(1, 200)).toBeCloseTo(2)
  })

  it('multiplies system and user', () => {
    expect(compoundScale(1.5, 100)).toBeCloseTo(1.5)
    expect(compoundScale(2, 50)).toBeCloseTo(1)
    expect(compoundScale(0.82, 150)).toBeCloseTo(1.23)
  })

  it('falls back to 1 for invalid system values', () => {
    expect(compoundScale(Number.NaN, 100)).toBe(1)
    expect(compoundScale(0, 100)).toBe(1)
    expect(compoundScale(-2, 100)).toBe(1)
  })

  it('falls back to 1 for invalid user values', () => {
    expect(compoundScale(1.5, Number.NaN)).toBe(1.5)
    expect(compoundScale(1.5, 0)).toBe(1.5)
    expect(compoundScale(1.5, -50)).toBe(1.5)
  })

  it('clamps to a minimum of 0.1 to avoid degenerate sizes', () => {
    // No realistic input produces this, but covers the guard.
    expect(compoundScale(0.01, 1)).toBeGreaterThanOrEqual(0.1)
  })
})
