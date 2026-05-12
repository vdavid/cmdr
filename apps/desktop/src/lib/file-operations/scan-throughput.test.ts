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

  // Targets stryker survivors around `dropStale`'s cutoff arithmetic and window
  // guards (lines 67–72). Pushes a 4-sample sequence with a non-linear progression
  // so that dropping the wrong number of stale samples yields a visibly different
  // rate. With `windowMs=1000`, the original drops only the very first sample;
  // mutants like `cutoff = nowMs + windowMs` or `<= cutoff` drop additional
  // samples and report a much lower rate.
  it('keeps the right window of samples when many arrive over a long span', () => {
    const t = new ScanThroughput(1000)
    t.push({ timestampMs: 0, files: 0, bytes: 0 })
    t.push({ timestampMs: 10_000, files: 100, bytes: 100 })
    t.push({ timestampMs: 10_500, files: 500, bytes: 500 })
    const r = t.push({ timestampMs: 11_000, files: 600, bytes: 600 })
    // Original drops only ts=0; rate = (600-100)/1.0s = 500.
    expect(r.filesPerSecond).toBe(500)
    expect(r.bytesPerSecond).toBe(500)
  })

  // Targets the `samples.length > 2` guard on line 70. The original keeps at
  // least two samples even after a long pause, so the readout still returns
  // a number (not null). Mutants that drop below two samples
  // (e.g. `true && ts < cutoff` or `length <= 2 && ts < cutoff`) would empty
  // the buffer and return null here.
  it('always keeps at least two samples even after a long pause', () => {
    const t = new ScanThroughput(1000)
    t.push({ timestampMs: 0, files: 0, bytes: 0 })
    t.push({ timestampMs: 100, files: 10, bytes: 100 })
    // Pause far longer than the window before pushing again.
    const r = t.push({ timestampMs: 60_000, files: 20, bytes: 200 })
    // Don't lock in the exact rate (depends on which two of the three samples
    // remain); the point is that the readout is still a finite number.
    expect(r.filesPerSecond).not.toBeNull()
    expect(r.bytesPerSecond).not.toBeNull()
    expect(r.filesPerSecond).toBeGreaterThan(0)
    expect(r.bytesPerSecond).toBeGreaterThan(0)
  })

  // Targets the `< cutoff` boundary on line 70. A sample whose timestamp is
  // exactly at the cutoff must NOT be dropped (strict <). Mutants that turn
  // this into `<=` or `>=` would shift it out and change the rate.
  it('treats the cutoff timestamp as inclusive (strict less-than)', () => {
    const t = new ScanThroughput(1000)
    // After the last push at ts=2000, cutoff = 2000 - 1000 = 1000.
    // The sample at ts=1000 sits exactly on the boundary: keep it.
    t.push({ timestampMs: 0, files: 0, bytes: 0 })
    t.push({ timestampMs: 1000, files: 100, bytes: 1000 })
    const r = t.push({ timestampMs: 2000, files: 300, bytes: 3000 })
    // Original keeps [1000@100, 2000@300]; rate = (300-100)/1.0 = 200.
    expect(r.filesPerSecond).toBe(200)
    expect(r.bytesPerSecond).toBe(2000)
  })
})
