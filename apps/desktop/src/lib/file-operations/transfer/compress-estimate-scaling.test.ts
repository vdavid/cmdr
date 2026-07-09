import { describe, it, expect } from 'vitest'
import { scaleCompressedEstimate } from './compress-estimate-scaling'
import type { CompressedSizeEstimate } from '$lib/tauri-commands'

const est = (compressibleBytes: number, mediumBytes: number, incompressibleBytes: number): CompressedSizeEstimate => ({
  compressibleBytes,
  mediumBytes,
  incompressibleBytes,
})

describe('scaleCompressedEstimate', () => {
  it('at level 6 returns the plain sum of the per-class subtotals (reference level)', () => {
    expect(scaleCompressedEstimate(est(1000, 2000, 3000), 6)).toBe(6000)
  })

  it('inflates compressible bytes most at the "Faster" (level 1) end', () => {
    // Only the compressible bucket is populated, so the whole estimate scales by
    // its level-1 multiplier (1.448).
    expect(scaleCompressedEstimate(est(1000, 0, 0), 1)).toBeCloseTo(1448, 5)
  })

  it('barely moves already-compressed content across levels', () => {
    // Incompressible bucket only: level 1 is within ~0.2% of level 6.
    expect(scaleCompressedEstimate(est(0, 0, 1000), 1)).toBeCloseTo(1002, 5)
    expect(scaleCompressedEstimate(est(0, 0, 1000), 9)).toBeCloseTo(1000, 5)
  })

  it('scales each class by its own curve for a mixed estimate at level 1', () => {
    // 1000*1.448 + 1000*1.104 + 1000*1.002 = 3554.
    expect(scaleCompressedEstimate(est(1000, 1000, 1000), 1)).toBeCloseTo(3554, 5)
  })

  it('is monotonic-ish: level 1 >= level 6 for compressible content', () => {
    const e = est(5000, 3000, 2000)
    expect(scaleCompressedEstimate(e, 1)).toBeGreaterThan(scaleCompressedEstimate(e, 6))
  })

  it('clamps out-of-range levels to 1..9', () => {
    const e = est(1000, 0, 0)
    expect(scaleCompressedEstimate(e, 0)).toBe(scaleCompressedEstimate(e, 1))
    expect(scaleCompressedEstimate(e, 42)).toBe(scaleCompressedEstimate(e, 9))
    // A fractional level rounds to the nearest stop.
    expect(scaleCompressedEstimate(e, 5.9)).toBe(scaleCompressedEstimate(e, 6))
  })

  it('returns zero for an all-zero estimate', () => {
    expect(scaleCompressedEstimate(est(0, 0, 0), 3)).toBe(0)
  })
})
