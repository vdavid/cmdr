/**
 * Tests for the pure ETA helpers driving the drive-indexing status tooltip.
 */
import { describe, it, expect } from 'vitest'
import { formatEta, computeElapsedEta, computeWindowEta, blendEtas, pruneSnapshots, type EtaSnapshot } from './eta'

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

  it('rounds to whole minutes at a minute and above', () => {
    expect(formatEta(60)).toBe('1m left')
    expect(formatEta(125)).toBe('2m left')
    expect(formatEta(600)).toBe('10m left')
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
