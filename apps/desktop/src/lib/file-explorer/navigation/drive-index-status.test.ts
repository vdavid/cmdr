import { describe, expect, it } from 'vitest'
import type { VolumeIndexStatus } from '$lib/ipc/bindings'
import {
  driveIndexState,
  driveIndexColorClass,
  driveIndexMenuActions,
  driveIndexMenuLabelKey,
  driveIndexDuration,
  driveIndexScanProgress,
  hasLastScanFacts,
} from './drive-index-status'

function makeStatus(overrides: Partial<VolumeIndexStatus> = {}): VolumeIndexStatus {
  return {
    volumeId: 'root',
    enabled: true,
    freshness: 'fresh',
    scanCompletedAt: 1_750_000_000,
    scanDurationMs: 134_000,
    ...overrides,
  }
}

describe('driveIndexState', () => {
  it('maps a not-enabled volume to gray (disabled)', () => {
    expect(driveIndexState(makeStatus({ enabled: false, freshness: null }))).toBe('disabled')
  })

  it('maps enabled-but-no-freshness to gray (disabled)', () => {
    // Defensive: a registered index should always carry freshness, but a null
    // there must still render gray, never crash.
    expect(driveIndexState(makeStatus({ enabled: true, freshness: null }))).toBe('disabled')
  })

  it('maps each freshness 1:1 to its state', () => {
    expect(driveIndexState(makeStatus({ freshness: 'scanning' }))).toBe('scanning')
    expect(driveIndexState(makeStatus({ freshness: 'fresh' }))).toBe('fresh')
    expect(driveIndexState(makeStatus({ freshness: 'stale' }))).toBe('stale')
  })
})

describe('driveIndexColorClass', () => {
  it('returns the four color suffixes', () => {
    expect(driveIndexColorClass('disabled')).toBe('disabled')
    expect(driveIndexColorClass('scanning')).toBe('scanning')
    expect(driveIndexColorClass('fresh')).toBe('fresh')
    expect(driveIndexColorClass('stale')).toBe('stale')
  })
})

describe('driveIndexMenuActions', () => {
  it('offers only enable when disabled', () => {
    expect(driveIndexMenuActions('disabled')).toEqual(['enable'])
  })

  it('offers only stop while scanning', () => {
    expect(driveIndexMenuActions('scanning')).toEqual(['stop'])
  })

  it('offers rescan + disable when fresh or stale', () => {
    expect(driveIndexMenuActions('fresh')).toEqual(['rescan', 'disable'])
    expect(driveIndexMenuActions('stale')).toEqual(['rescan', 'disable'])
  })
})

describe('driveIndexMenuLabelKey', () => {
  it('maps each action to a distinct catalog key', () => {
    const keys = (['enable', 'rescan', 'disable', 'stop'] as const).map(driveIndexMenuLabelKey)
    expect(new Set(keys).size).toBe(4)
    expect(driveIndexMenuLabelKey('enable')).toBe('fileExplorer.navigation.driveIndex.menuEnable')
    expect(driveIndexMenuLabelKey('rescan')).toBe('fileExplorer.navigation.driveIndex.menuRescan')
    expect(driveIndexMenuLabelKey('disable')).toBe('fileExplorer.navigation.driveIndex.menuDisable')
    expect(driveIndexMenuLabelKey('stop')).toBe('fileExplorer.navigation.driveIndex.menuStop')
  })
})

describe('driveIndexDuration', () => {
  it('returns null for absent or negative durations', () => {
    expect(driveIndexDuration(null)).toBeNull()
    expect(driveIndexDuration(-1)).toBeNull()
  })

  it('formats sub-minute durations as seconds only', () => {
    expect(driveIndexDuration(14_000)).toEqual({
      key: 'fileExplorer.navigation.driveIndex.durationSec',
      params: { seconds: '14' },
    })
  })

  it('formats minute-plus durations as min + sec', () => {
    // 2 min 14 s = 134_000 ms
    expect(driveIndexDuration(134_000)).toEqual({
      key: 'fileExplorer.navigation.driveIndex.durationMinSec',
      params: { minutes: '2', seconds: '14' },
    })
  })

  it('rounds milliseconds to the nearest second', () => {
    expect(driveIndexDuration(13_600)).toEqual({
      key: 'fileExplorer.navigation.driveIndex.durationSec',
      params: { seconds: '14' },
    })
  })

  it('handles an exact minute (zero trailing seconds)', () => {
    expect(driveIndexDuration(60_000)).toEqual({
      key: 'fileExplorer.navigation.driveIndex.durationMinSec',
      params: { minutes: '1', seconds: '0' },
    })
  })
})

describe('hasLastScanFacts', () => {
  it('is true only when both date and duration are present', () => {
    expect(hasLastScanFacts(makeStatus())).toBe(true)
    expect(hasLastScanFacts(makeStatus({ scanCompletedAt: null }))).toBe(false)
    expect(hasLastScanFacts(makeStatus({ scanDurationMs: null }))).toBe(false)
  })
})

describe('driveIndexScanProgress', () => {
  const started = 1_000_000

  it('uses the count-only key before a full second has elapsed', () => {
    const r = driveIndexScanProgress(42, started, started + 500)
    expect(r.key).toBe('fileExplorer.navigation.driveIndex.tooltipScanningCount')
    expect(r.params.count).toBe(42)
    // countText is locale-formatted; just confirm it's a non-empty string.
    expect(typeof r.params.countText).toBe('string')
    expect(r.params.elapsed).toBeUndefined()
  })

  it('adds the elapsed clock once at least a second has elapsed', () => {
    const r = driveIndexScanProgress(12_345, started, started + 42_000)
    expect(r.key).toBe('fileExplorer.navigation.driveIndex.tooltipScanningCountElapsed')
    expect(r.params.count).toBe(12_345)
    expect(r.params.elapsed).toBe('0:42')
  })

  it('formats minutes with zero-padded seconds', () => {
    const r = driveIndexScanProgress(1, started, started + (12 * 60 + 5) * 1000)
    expect(r.params.elapsed).toBe('12:05')
  })

  it('passes the raw count through for plural selection', () => {
    expect(driveIndexScanProgress(1, started, started).params.count).toBe(1)
    expect(driveIndexScanProgress(2, started, started).params.count).toBe(2)
  })

  it('falls back to count-only when the clock is non-finite or behind the start', () => {
    expect(driveIndexScanProgress(5, started, started - 1000).key).toBe(
      'fileExplorer.navigation.driveIndex.tooltipScanningCount',
    )
    expect(driveIndexScanProgress(5, started, Number.NaN).key).toBe(
      'fileExplorer.navigation.driveIndex.tooltipScanningCount',
    )
  })
})
