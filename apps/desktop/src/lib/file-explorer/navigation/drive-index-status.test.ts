import { describe, expect, it } from 'vitest'
import type { VolumeIndexStatus } from '$lib/ipc/bindings'
import {
  driveIndexState,
  driveIndexColorClass,
  driveIndexMenuActions,
  driveIndexMenuLabelKey,
  driveIndexDuration,
  driveIndexRefusalMessageKey,
  driveIndexActionFeedback,
  driveIndexCoalescedNote,
  driveIndexUnreadableNote,
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
    unreadableLocations: 0,
    unreadableRetried: false,
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

  it('offers nothing at all while the master drive-indexing switch is off', () => {
    // The master switch outranks every per-drive choice: the backend refuses every
    // start while it's off, so offering per-drive actions would promise work that
    // can't happen. The menu shows the explanatory note instead.
    for (const state of ['disabled', 'scanning', 'fresh', 'stale', 'failed'] as const) {
      expect(driveIndexMenuActions(state, false)).toEqual([])
    }
  })

  it('restores the per-state actions when the master switch is back on', () => {
    // Turning the master switch back on must return each drive to the actions its
    // OWN state allows, not a blanket set.
    expect(driveIndexMenuActions('fresh', true)).toEqual(['rescan', 'disable', 'forget'])
    expect(driveIndexMenuActions('disabled', true)).toEqual(['enable'])
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

  it('sends the master-switch refusal to the settings-oriented copy, not reconnect advice', () => {
    // Nothing is wrong with the share; drive indexing is off in Settings.
    expect(driveIndexRefusalMessageKey('indexing_disabled')).toBe(
      'fileExplorer.navigation.driveIndex.refusedIndexingOff',
    )
  })
})

describe('driveIndexActionFeedback', () => {
  it('says nothing when the scan started: the badge already shows it', () => {
    expect(driveIndexActionFeedback('rescan', { status: 'ok', data: { status: 'started' } })).toEqual({
      kind: 'silent',
    })
  })

  it('promises the scan when a live search is what stands in its way, in the words of the button pressed', () => {
    expect(
      driveIndexActionFeedback('rescan', { status: 'ok', data: { status: 'deferred_until_search_ends' } }),
    ).toEqual({
      kind: 'toast',
      key: 'fileExplorer.navigation.driveIndex.deferredRescan',
      level: 'info',
    })
    expect(
      driveIndexActionFeedback('enable', { status: 'ok', data: { status: 'deferred_until_search_ends' } }),
    ).toEqual({
      kind: 'toast',
      key: 'fileExplorer.navigation.driveIndex.deferredEnable',
      level: 'info',
    })
  })

  it('promises the scan when the drive is already being indexed, whichever button was pressed', () => {
    for (const action of ['rescan', 'enable'] as const) {
      expect(driveIndexActionFeedback(action, { status: 'ok', data: { status: 'deferred_until_scan_ends' } })).toEqual({
        kind: 'toast',
        key: 'fileExplorer.navigation.driveIndex.queuedBehindScan',
        level: 'info',
      })
    }
  })

  it('points at the master switch when it is what refused', () => {
    expect(driveIndexActionFeedback('enable', { status: 'ok', data: { status: 'indexing_disabled' } })).toEqual({
      kind: 'toast',
      key: 'fileExplorer.navigation.driveIndex.refusedIndexingOff',
      level: 'info',
    })
  })

  it('hands a typed per-drive refusal back for the caller to route', () => {
    expect(
      driveIndexActionFeedback('enable', {
        status: 'ok',
        data: { status: 'refused', reason: 'credentials_needed' },
      }),
    ).toEqual({ kind: 'refusal', reason: 'credentials_needed' })
  })

  it('speaks up when the command comes back with an error, which reaches the caller as a value, not a throw', () => {
    // `typedError` rethrows only real `Error` instances, so a Rust `Err(String)`
    // lands here as `{ status: 'error' }`. Left unhandled it was a click that did
    // nothing and said nothing.
    expect(driveIndexActionFeedback('rescan', { status: 'error', error: 'boom' })).toEqual({
      kind: 'toast',
      key: 'fileExplorer.navigation.driveIndex.refusedGeneric',
      level: 'error',
    })
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

  it('points at the check in flight instead of promising a later one', () => {
    // The other variants say "the next full check will fix it". While the check is
    // actually running, that clause is stale: say it's happening now. The running
    // scan cleared the completed-at marker, so there's no window left to name and
    // the running variant asks for none.
    expect(
      driveIndexCoalescedNote(
        makeStatus({ coalescedSignalsSinceSweep: 4, freshness: 'scanning', scanCompletedAt: null }),
        NOW,
      ),
    ).toEqual({
      key: 'fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning',
      count: 4,
      hours: null,
      remaining: null,
    })
  })

  it('stays quiet on states where the note would confuse', () => {
    // Disabled/failed: there's no live index the note could describe.
    for (const status of [
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

  it('stays quiet when a settled drive has no completed scan to anchor the time window', () => {
    expect(
      driveIndexCoalescedNote(makeStatus({ coalescedSignalsSinceSweep: 4, scanCompletedAt: null }), NOW),
    ).toBeNull()
  })
})

describe('driveIndexUnreadableNote — "done, with holes"', () => {
  it('says nothing when a finished index read everything', () => {
    expect(driveIndexUnreadableNote(makeStatus({ unreadableLocations: 0 }))).toBeNull()
  })

  it('counts PLACES and says Cmdr comes back when the ground is the retryable kind', () => {
    // The distinction that keeps this a footnote rather than a fault: a folder
    // that stopped answering is Cmdr's to retry, and saying so is the reason not
    // to offer the reader an action.
    expect(driveIndexUnreadableNote(makeStatus({ unreadableLocations: 3, unreadableRetried: true }))).toEqual({
      key: 'fileExplorer.navigation.driveIndex.tooltipUnreadableRetried',
      count: 3,
    })
  })

  it('drops the retry sentence for ground nothing will come back to', () => {
    // Refused, or skipped on purpose. Promising a retry there would be a promise
    // Cmdr never keeps.
    expect(driveIndexUnreadableNote(makeStatus({ unreadableLocations: 1, unreadableRetried: false }))).toEqual({
      key: 'fileExplorer.navigation.driveIndex.tooltipUnreadable',
      count: 1,
    })
  })

  it('speaks for a stale index too, which is a finished one going out of date', () => {
    expect(driveIndexUnreadableNote(makeStatus({ freshness: 'stale', unreadableLocations: 2 }))).not.toBeNull()
  })

  it('stays quiet while a drive is still being indexed', () => {
    // ❌ Ground the walker hasn't REACHED yet is not ground it couldn't read, and
    // the checklist is already saying what's happening.
    expect(driveIndexUnreadableNote(makeStatus({ freshness: 'scanning', unreadableLocations: 5 }))).toBeNull()
    expect(driveIndexUnreadableNote(makeStatus({ enabled: false, freshness: null, unreadableLocations: 5 }))).toBeNull()
  })
})
