/**
 * Tests for the pure ETA helpers driving the drive-indexing status tooltip.
 */
import { describe, it, expect } from 'vitest'
import {
  formatEta,
  computeElapsedEta,
  computeWindowEta,
  blendEtas,
  pruneSnapshots,
  computeScanProgress,
  type EtaSnapshot,
} from './eta'

describe('formatEta', () => {
  it('reads "Almost done" under two seconds', () => {
    expect(formatEta(0)).toBe('Almost done')
    expect(formatEta(1.9)).toBe('Almost done')
  })

  it('counts down in whole seconds under a minute', () => {
    expect(formatEta(2)).toBe('2s left')
    expect(formatEta(12.4)).toBe('12s left')
    expect(formatEta(59)).toBe('59s left')
  })

  it('rounds to whole minutes between a minute and an hour', () => {
    expect(formatEta(60)).toBe('1m left')
    expect(formatEta(125)).toBe('2m left')
    expect(formatEta(600)).toBe('10m left')
  })

  it('spells out hours and minutes from an hour up', () => {
    expect(formatEta(3600)).toBe('1 hour left')
    expect(formatEta(3660)).toBe('1 hour 1 minute left')
    // The reported NAS case: "84m left" is hard to read as an hour-scale wait.
    expect(formatEta(84 * 60)).toBe('1 hour 24 minutes left')
    expect(formatEta(2 * 3600)).toBe('2 hours left')
    expect(formatEta(9 * 3600 + 59 * 60)).toBe('9 hours 59 minutes left')
  })

  it('drops the minutes from ten hours up', () => {
    expect(formatEta(10 * 3600)).toBe('10 hours left')
    // The reported NAS case: "1200m left" forced the reader to do the division.
    expect(formatEta(1200 * 60)).toBe('20 hours left')
    expect(formatEta(10 * 3600 + 29 * 60)).toBe('10 hours left')
    expect(formatEta(10 * 3600 + 31 * 60)).toBe('11 hours left')
  })

  it('reads "Almost done" for non-finite input', () => {
    // The scan branch is a new caller; a dropped null gate upstream would otherwise
    // surface "Infinitym left". One line of insurance against that failure mode.
    expect(formatEta(Number.POSITIVE_INFINITY)).toBe('Almost done')
    expect(formatEta(Number.NaN)).toBe('Almost done')
  })
})

describe('computeElapsedEta', () => {
  it('extrapolates remaining time from elapsed and progress', () => {
    // 5s elapsed for 100 done, 400 remaining → 5 * (400 / 100) = 20s
    expect(computeElapsedEta(5, 100, 400)).toBe(20)
  })

  it('returns null without elapsed time or progress', () => {
    expect(computeElapsedEta(0, 100, 400)).toBeNull()
    expect(computeElapsedEta(5, 0, 400)).toBeNull()
    expect(computeElapsedEta(-1, 100, 400)).toBeNull()
  })
})

describe('computeWindowEta', () => {
  it('returns null with fewer than two snapshots', () => {
    expect(computeWindowEta([], 100)).toBeNull()
    expect(computeWindowEta([{ timestamp: 0, eventsProcessed: 0 }], 100)).toBeNull()
  })

  it('derives a rate from the window and projects the remaining time', () => {
    const snapshots: EtaSnapshot[] = [
      { timestamp: 1000, eventsProcessed: 100 },
      { timestamp: 3000, eventsProcessed: 300 },
    ]
    // 200 events over 2s → 100/s; 500 remaining → 5s
    expect(computeWindowEta(snapshots, 500)).toBe(5)
  })

  it('returns null on a zero-width window or non-positive rate', () => {
    const sameTime: EtaSnapshot[] = [
      { timestamp: 1000, eventsProcessed: 100 },
      { timestamp: 1000, eventsProcessed: 200 },
    ]
    expect(computeWindowEta(sameTime, 500)).toBeNull()

    const noProgress: EtaSnapshot[] = [
      { timestamp: 1000, eventsProcessed: 200 },
      { timestamp: 3000, eventsProcessed: 200 },
    ]
    expect(computeWindowEta(noProgress, 500)).toBeNull()
  })
})

describe('blendEtas', () => {
  it('averages two estimates 50-50', () => {
    expect(blendEtas(10, 20)).toBe(15)
  })

  it('falls back to whichever one is available', () => {
    expect(blendEtas(10, null)).toBe(10)
    expect(blendEtas(null, 20)).toBe(20)
    expect(blendEtas(null, null)).toBeNull()
  })
})

describe('pruneSnapshots', () => {
  it('drops snapshots older than the window before the newest', () => {
    const snapshots: EtaSnapshot[] = [
      { timestamp: 1000, eventsProcessed: 10 },
      { timestamp: 4000, eventsProcessed: 40 },
      { timestamp: 7000, eventsProcessed: 70 },
    ]
    // Window 5000ms, newest at 7000 → cutoff 2000, drops the 1000 sample.
    expect(pruneSnapshots(snapshots, 5000)).toEqual([
      { timestamp: 4000, eventsProcessed: 40 },
      { timestamp: 7000, eventsProcessed: 70 },
    ])
  })

  it('returns the same array when nothing needs pruning', () => {
    const snapshots: EtaSnapshot[] = [
      { timestamp: 6000, eventsProcessed: 60 },
      { timestamp: 7000, eventsProcessed: 70 },
    ]
    expect(pruneSnapshots(snapshots, 5000)).toBe(snapshots)
  })

  it('returns the same array when empty', () => {
    const empty: EtaSnapshot[] = []
    expect(pruneSnapshots(empty, 5000)).toBe(empty)
  })
})

describe('computeScanProgress', () => {
  it('uses the entry calibration (tier 1) when a prior total is present', () => {
    // 5,000 of a prior 10,000 entries → 0.5, calibrated (not rough).
    expect(computeScanProgress(5000, 1000, 10000, 4_000_000)).toEqual({ fraction: 0.5, rough: false })
  })

  it('clamps the calibrated fraction at 0.99 when this scan outgrows the prior total', () => {
    // Disk grew since last scan: 12,000 vs a prior 10,000 → clamp to 0.99, never 100% mid-scan.
    expect(computeScanProgress(12000, 1000, 10000, 4_000_000)).toEqual({ fraction: 0.99, rough: false })
  })

  it('falls back to bytes (tier 2, rough) when there is no prior total but used bytes are known', () => {
    // 1 MB of a 4 MB volume → 0.25, rough.
    expect(computeScanProgress(5000, 1_000_000, null, 4_000_000)).toEqual({ fraction: 0.25, rough: true })
  })

  it('clamps the rough fraction at 0.95 (wider error band: clones overshoot)', () => {
    // Clone-heavy disk overshoots the statfs denominator → clamp lower than tier 1.
    expect(computeScanProgress(5000, 5_000_000, null, 4_000_000)).toEqual({ fraction: 0.95, rough: true })
  })

  it('prefers the entry calibration over bytes when both denominators are present', () => {
    expect(computeScanProgress(5000, 9_999_999, 10000, 4_000_000)).toEqual({ fraction: 0.5, rough: false })
  })

  it('returns null when neither denominator is available', () => {
    expect(computeScanProgress(5000, 1_000_000, null, null)).toBeNull()
  })

  it('returns null on zero denominators (no division by zero, no fake 100%)', () => {
    expect(computeScanProgress(5000, 1_000_000, 0, 4_000_000)).toEqual({ fraction: 0.25, rough: true })
    expect(computeScanProgress(5000, 1_000_000, 0, 0)).toBeNull()
    expect(computeScanProgress(0, 0, 0, 0)).toBeNull()
  })
})
