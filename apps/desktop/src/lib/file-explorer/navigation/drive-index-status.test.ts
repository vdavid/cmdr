import { describe, expect, it } from 'vitest'
import type { VolumeIndexStatus } from '$lib/ipc/bindings'
import {
  driveIndexState,
  driveIndexColorClass,
  driveIndexMenuActions,
  driveIndexMenuLabelKey,
  driveIndexDuration,
  driveIndexRefusalMessageKey,
  driveIndexCoalescedNote,
  hasLastScanFacts,
} from './drive-index-status'

function makeStatus(overrides: Partial<VolumeIndexStatus> = {}): VolumeIndexStatus {
  return {
    volumeId: 'root',
    enabled: true,
    freshness: 'fresh',
    failure: null,
    scanCompletedAt: 1_750_000_000,
    scanDurationMs: 134_000,
    coalescedSignalsSinceSweep: 0,
    nextSweepDueAt: null,
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
    expect(driveIndexState(makeStatus({ freshness: 'failed' }))).toBe('failed')
  })

  it('maps a failed index to red even though it reports not-enabled', () => {
    // A failed index is registered (so the badge is honest) but `enabled: false`
    // (its writer is torn down). It must render red, NOT fall through to gray.
    expect(
      driveIndexState(makeStatus({ enabled: false, freshness: 'failed', failure: { code: 10, extendedCode: 266 } })),
    ).toBe('failed')
  })
})

describe('driveIndexColorClass', () => {
  it('returns the four color suffixes', () => {
    expect(driveIndexColorClass('disabled')).toBe('disabled')
    expect(driveIndexColorClass('scanning')).toBe('scanning')
    expect(driveIndexColorClass('fresh')).toBe('fresh')
    expect(driveIndexColorClass('stale')).toBe('stale')
    expect(driveIndexColorClass('failed')).toBe('failed')
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

  it('offers rescan (rebuild) + forget when failed, but no disable', () => {
    // A failed index can only be rebuilt (rescan) or deleted (forget); there is
    // nothing running to disable.
    expect(driveIndexMenuActions('failed')).toEqual(['rescan', 'forget'])
    expect(driveIndexMenuActions('failed')).not.toContain('disable')
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

describe('driveIndexRefusalMessageKey', () => {
  it('maps an internal-error refusal (not an SMB volume) to the internal-error copy, not reconnect advice', () => {
    expect(driveIndexRefusalMessageKey('not_an_smb_volume')).toBe('fileExplorer.navigation.driveIndex.refusedInternal')
    expect(driveIndexRefusalMessageKey('not_registered')).toBe('fileExplorer.navigation.driveIndex.refusedInternal')
  })

  it('keeps the SMB-specific reasons on their share-oriented copy', () => {
    expect(driveIndexRefusalMessageKey('upgrade_failed')).toBe(
      'fileExplorer.navigation.driveIndex.refusedUpgradeFailed',
    )
    expect(driveIndexRefusalMessageKey('disconnected')).toBe('fileExplorer.navigation.driveIndex.refusedDisconnected')
  })

  it('returns null for credentials_needed (routes to the reconnect flow, no toast)', () => {
    expect(driveIndexRefusalMessageKey('credentials_needed')).toBeNull()
  })
})

describe('driveIndexCoalescedNote', () => {
  // `makeStatus`'s last scan is 1_750_000_000; NOW is exactly 24 hours later, and
  // the default next sweep is another 6 hours out.
  const NOW = 1_750_086_400
  const IN_SIX_HOURS = NOW + 6 * 3600

  it('renders nothing when macOS never lost track since the last full check', () => {
    expect(driveIndexCoalescedNote(makeStatus({ coalescedSignalsSinceSweep: 0 }), NOW)).toBeNull()
  })

  it('reports a single skipped signal, with the next check ahead', () => {
    expect(
      driveIndexCoalescedNote(makeStatus({ coalescedSignalsSinceSweep: 1, nextSweepDueAt: IN_SIX_HOURS }), NOW),
    ).toEqual({
      key: 'fileExplorer.navigation.driveIndex.tooltipCoalesced',
      count: 1,
      hours: 24,
      remaining: 6,
    })
  })

  it('reports several skipped signals', () => {
    expect(
      driveIndexCoalescedNote(makeStatus({ coalescedSignalsSinceSweep: 11, nextSweepDueAt: IN_SIX_HOURS }), NOW),
    ).toEqual({
      key: 'fileExplorer.navigation.driveIndex.tooltipCoalesced',
      count: 11,
      hours: 24,
      remaining: 6,
    })
  })

  it('drops the next-check promise for a drive with no scheduled sweep', () => {
    // `nextSweepDueAt` is null for every volume without a daily sweep (an external
    // drive runs a 45-second debounce, which promises nothing). Saying "in 0 hours"
    // there would be a lie, so the clause goes away entirely.
    expect(driveIndexCoalescedNote(makeStatus({ coalescedSignalsSinceSweep: 3, nextSweepDueAt: null }), NOW)).toEqual({
      key: 'fileExplorer.navigation.driveIndex.tooltipCoalescedNoNextCheck',
      count: 3,
      hours: 24,
      remaining: null,
    })
  })

  it('drops the next-check promise once the sweep is already due', () => {
    expect(
      driveIndexCoalescedNote(makeStatus({ coalescedSignalsSinceSweep: 3, nextSweepDueAt: NOW - 60 }), NOW),
    ).toEqual({
      key: 'fileExplorer.navigation.driveIndex.tooltipCoalescedNoNextCheck',
      count: 3,
      hours: 24,
      remaining: null,
    })
  })

  it('never says "in the last 0 hours" or "in 0 hours" under the hour', () => {
    const note = driveIndexCoalescedNote(
      makeStatus({
        coalescedSignalsSinceSweep: 2,
        scanCompletedAt: NOW - 90,
        nextSweepDueAt: NOW + 90,
      }),
      NOW,
    )
    expect(note).toEqual({
      key: 'fileExplorer.navigation.driveIndex.tooltipCoalesced',
      count: 2,
      hours: 1,
      remaining: 1,
    })
  })

  it('rounds partial hours up, so the window it names always covers what happened', () => {
    const note = driveIndexCoalescedNote(
      makeStatus({
        coalescedSignalsSinceSweep: 2,
        scanCompletedAt: NOW - (3 * 3600 + 60),
        nextSweepDueAt: NOW + (4 * 3600 + 60),
      }),
      NOW,
    )
    expect(note?.hours).toBe(4)
    expect(note?.remaining).toBe(5)
  })

  it('stays quiet on states where the note would confuse', () => {
    // Scanning: the sweep may be the very scan in flight. Disabled/failed: there's
    // no live index the note could describe.
    for (const status of [
      makeStatus({ coalescedSignalsSinceSweep: 4, freshness: 'scanning' }),
      makeStatus({ coalescedSignalsSinceSweep: 4, enabled: false, freshness: null }),
      makeStatus({ coalescedSignalsSinceSweep: 4, freshness: 'failed' }),
    ]) {
      expect(driveIndexCoalescedNote(status, NOW)).toBeNull()
    }
  })

  it('renders on a stale drive too, not only a fresh one', () => {
    expect(driveIndexCoalescedNote(makeStatus({ coalescedSignalsSinceSweep: 2, freshness: 'stale' }), NOW)?.key).toBe(
      'fileExplorer.navigation.driveIndex.tooltipCoalescedNoNextCheck',
    )
  })

  it('stays quiet when no completed scan anchors the time window', () => {
    expect(
      driveIndexCoalescedNote(makeStatus({ coalescedSignalsSinceSweep: 4, scanCompletedAt: null }), NOW),
    ).toBeNull()
  })
})
