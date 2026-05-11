import { describe, it, expect } from 'vitest'
import { ScanThroughput } from './scan-throughput'

describe('ScanThroughput', () => {
  it('returns null rates until two samples arrive', () => {
    const t = new ScanThroughput()
    expect(t.push({ timestampMs: 0, files: 0, bytes: 0 })).toEqual({
      filesPerSecond: null,
      bytesPerSecond: null,
    })
  })

  it('computes rate from two samples', () => {
    const t = new ScanThroughput()
    t.push({ timestampMs: 0, files: 0, bytes: 0 })
    const r = t.push({ timestampMs: 1000, files: 500, bytes: 1_000_000 })
    expect(r.filesPerSecond).toBe(500)
    expect(r.bytesPerSecond).toBe(1_000_000)
  })

  it('returns null rate when both samples share the same timestamp', () => {
    const t = new ScanThroughput()
    t.push({ timestampMs: 100, files: 0, bytes: 0 })
    const r = t.push({ timestampMs: 100, files: 5, bytes: 50 })
    expect(r.filesPerSecond).toBeNull()
    expect(r.bytesPerSecond).toBeNull()
  })

  it('discards samples outside the rolling window', () => {
    const t = new ScanThroughput(1000)
    // Stale baseline 5 s ago
    t.push({ timestampMs: 0, files: 0, bytes: 0 })
    // Two recent samples 500 ms apart
    t.push({ timestampMs: 5000, files: 1000, bytes: 1_000_000 })
    const r = t.push({ timestampMs: 5500, files: 1100, bytes: 1_100_000 })
    // Rate computed from the two recent samples only.
    expect(r.filesPerSecond).toBe(200) // (1100-1000)/0.5s
    expect(r.bytesPerSecond).toBe(200_000)
  })

  it('clamps negative deltas to zero (defensive against backend resets)', () => {
    const t = new ScanThroughput()
    t.push({ timestampMs: 0, files: 100, bytes: 1000 })
    // Scan restarts and reports lower numbers — should produce 0, not negative.
    const r = t.push({ timestampMs: 1000, files: 50, bytes: 500 })
    expect(r.filesPerSecond).toBe(0)
    expect(r.bytesPerSecond).toBe(0)
  })

  it('reset clears history', () => {
    const t = new ScanThroughput()
    t.push({ timestampMs: 0, files: 0, bytes: 0 })
    t.push({ timestampMs: 1000, files: 100, bytes: 1000 })
    t.reset()
    expect(t.readout()).toEqual({ filesPerSecond: null, bytesPerSecond: null })
    // After reset, need two new samples again.
    t.push({ timestampMs: 2000, files: 200, bytes: 2000 })
    expect(t.readout()).toEqual({ filesPerSecond: null, bytesPerSecond: null })
  })
})
