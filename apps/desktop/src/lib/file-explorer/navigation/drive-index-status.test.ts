import { describe, expect, it } from 'vitest'
import type { VolumeIndexStatus } from '$lib/ipc/bindings'
import {
  driveIndexState,
  driveIndexColorClass,
  driveIndexMenuActions,
  driveIndexMenuLabelKey,
  driveIndexDuration,
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

  it('offers stop + forget while scanning', () => {
    expect(driveIndexMenuActions('scanning')).toEqual(['stop', 'forget'])
  })

  it('offers rescan + disable + forget when fresh or stale', () => {
    expect(driveIndexMenuActions('fresh')).toEqual(['rescan', 'disable', 'forget'])
    expect(driveIndexMenuActions('stale')).toEqual(['rescan', 'disable', 'forget'])
  })

  it('does not offer forget when disabled (no index to delete)', () => {
    expect(driveIndexMenuActions('disabled')).not.toContain('forget')
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
