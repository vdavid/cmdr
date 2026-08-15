/**
 * Unit tests for the per-volume aggregation attribution in `index-state.svelte`.
 *
 * The regression this pins: `index-aggregation-progress` (and `-complete`) carry
 * a `volumeId`, so two drives aggregating at once each get their own progress —
 * no more single global aggregation state mis-attributed to the last scan to
 * complete. We mock the Tauri event wrappers to capture the registered callbacks
 * and fire them directly, then read the per-volume aggregation getters.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { flushSync } from 'svelte'
import type {
  ActivityPhase,
  AggregationProgressEvent,
  IndexAggregationCompleteEvent,
  IndexCoverageBranchEndedEvent,
  IndexCoverageBranchStartedEvent,
  IndexCoveragePhaseStartedEvent,
  IndexPhaseChangedEvent,
  IndexReplayProgressEvent,
  IndexScanAbortedEvent,
  IndexScanProgressEvent,
  IndexScanStartedEvent,
  CoveragePhaseLabel,
  ScanRunKind,
} from '$lib/ipc/bindings'

// Captured callbacks the module registers via the wrappers below.
let aggProgressCb: ((p: AggregationProgressEvent) => void) | undefined
let aggCompleteCb: ((p: IndexAggregationCompleteEvent) => void) | undefined
let scanStartedCb: ((p: IndexScanStartedEvent) => void) | undefined
let scanProgressCb: ((p: IndexScanProgressEvent) => void) | undefined
let scanAbortedCb: ((p: IndexScanAbortedEvent) => void) | undefined
let phaseCb: ((p: IndexPhaseChangedEvent) => void) | undefined
let replayProgressCb: ((p: IndexReplayProgressEvent) => void) | undefined
let branchStartedCb: ((p: IndexCoverageBranchStartedEvent) => void) | undefined
let branchEndedCb: ((p: IndexCoverageBranchEndedEvent) => void) | undefined
let coveragePhaseCb: ((p: IndexCoveragePhaseStartedEvent) => void) | undefined

const noopUnlisten = () => {}

// Mock the typed event wrappers: capture the ones the tests drive, no-op the rest.
vi.mock('$lib/tauri-commands', () => ({
  onIndexScanStarted: (cb: (p: IndexScanStartedEvent) => void) => {
    scanStartedCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexScanProgress: (cb: (p: IndexScanProgressEvent) => void) => {
    scanProgressCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexScanComplete: () => Promise.resolve(noopUnlisten),
  onIndexScanAborted: (cb: (p: IndexScanAbortedEvent) => void) => {
    scanAbortedCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexCoverageBranchStarted: (cb: (p: IndexCoverageBranchStartedEvent) => void) => {
    branchStartedCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexCoverageBranchEnded: (cb: (p: IndexCoverageBranchEndedEvent) => void) => {
    branchEndedCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexCoveragePhaseStarted: (cb: (p: IndexCoveragePhaseStartedEvent) => void) => {
    coveragePhaseCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexPhaseChanged: (cb: (p: IndexPhaseChangedEvent) => void) => {
    phaseCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexAggregationProgress: (cb: (p: AggregationProgressEvent) => void) => {
    aggProgressCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexAggregationComplete: (cb: (p: IndexAggregationCompleteEvent) => void) => {
    aggCompleteCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexRescanNotification: () => Promise.resolve(noopUnlisten),
  onIndexReplayProgress: (cb: (p: IndexReplayProgressEvent) => void) => {
    replayProgressCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexReplayComplete: () => Promise.resolve(noopUnlisten),
}))

// `initIndexState` calls `commands.getIndexStatus()` for the root backfill.
// Return "not scanning" so the backfill is a no-op.
vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getIndexStatus: () => Promise.resolve({ status: 'ok', data: { scanning: false } }),
  },
}))

import {
  initIndexState,
  destroyIndexState,
  getVolumeAggregation,
  getVolumeActivity,
  getEntriesScanned,
  getVolumePhase,
  getVolumeScanRunKind,
  getActivePhaseVolumeIds,
  getAggregatingVolumeIds,
  isVolumeScanning,
  isVolumeAggregating,
  isAnyVolumeIndexing,
  getWalkedGround,
  getVolumeCoveragePhase,
  type AggregationActivity,
} from './index-state.svelte'
import { isPathAffectedByWalk } from './walked-ground'

// Fire an aggregation-progress event through the captured callback.
function emitProgress(volumeId: string, phase: string, current: number, total: number): void {
  if (!aggProgressCb) throw new Error('aggregation-progress callback not registered')
  aggProgressCb({ volumeId, phase, current, total })
}

// Fire an aggregation-complete event through the captured callback.
function emitComplete(volumeId: string): void {
  if (!aggCompleteCb) throw new Error('aggregation-complete callback not registered')
  aggCompleteCb({ volumeId })
}

// Read a volume's aggregation, asserting it's present.
function expectAggregation(volumeId: string): AggregationActivity {
  const agg = getVolumeAggregation(volumeId)
  expect(agg).toBeDefined()
  return agg as AggregationActivity
}

describe('index-state per-volume aggregation', () => {
  beforeEach(async () => {
    destroyIndexState()
    aggProgressCb = undefined
    aggCompleteCb = undefined
    scanProgressCb = undefined
    scanAbortedCb = undefined
    phaseCb = undefined
    branchStartedCb = undefined
    branchEndedCb = undefined
    await initIndexState()
  })

  it('tracks two volumes aggregating concurrently, each with its own progress', () => {
    emitProgress('root', 'computing', 250, 1000)
    emitProgress('smb-nas', 'writing', 40, 50)

    expect(getVolumeAggregation('root')).toMatchObject({ phase: 'computing', current: 250, total: 1000 })
    expect(getVolumeAggregation('smb-nas')).toMatchObject({ phase: 'writing', current: 40, total: 50 })
    // Each is attributed to its own volume — not conflated into one signal.
    expect(getAggregatingVolumeIds().sort()).toEqual(['root', 'smb-nas'])
    expect(isVolumeAggregating('root')).toBe(true)
    expect(isVolumeAggregating('smb-nas')).toBe(true)
    // A volume with no aggregation entry reads false (the per-volume scoping).
    expect(isVolumeAggregating('mtp-phone')).toBe(false)
    expect(isAnyVolumeIndexing()).toBe(true)
  })

  it('updating one volume leaves the other untouched', () => {
    emitProgress('root', 'computing', 100, 1000)
    emitProgress('smb-nas', 'writing', 10, 50)

    emitProgress('root', 'computing', 900, 1000)

    expect(getVolumeAggregation('root')).toMatchObject({ current: 900 })
    // The NAS row is unaffected by the root update.
    expect(getVolumeAggregation('smb-nas')).toMatchObject({ phase: 'writing', current: 10, total: 50 })
  })

  it("completion clears only the named volume's aggregation", () => {
    emitProgress('root', 'computing', 250, 1000)
    emitProgress('smb-nas', 'writing', 40, 50)

    emitComplete('root')

    expect(getVolumeAggregation('root')).toBeUndefined()
    expect(getVolumeAggregation('smb-nas')).toMatchObject({ phase: 'writing' })
    expect(getAggregatingVolumeIds()).toEqual(['smb-nas'])
    expect(isVolumeAggregating('root')).toBe(false)
    expect(isVolumeAggregating('smb-nas')).toBe(true)

    emitComplete('smb-nas')
    expect(isVolumeAggregating('smb-nas')).toBe(false)
    expect(getAggregatingVolumeIds()).toEqual([])
  })

  it("clears a volume's live activity on an abort (no completion event fires)", () => {
    if (!scanProgressCb) throw new Error('scan-progress callback not registered')
    if (!scanAbortedCb) throw new Error('scan-aborted callback not registered')

    // A network scan reports progress (seeds the activity entry), then aborts
    // (disconnect/cancel) — which fires NO scan-complete, only scan-aborted.
    scanProgressCb({ volumeId: 'smb-nas', entriesScanned: 1234, dirsFound: 56, bytesScanned: 7890 })
    emitProgress('smb-nas', 'computing', 10, 100) // a partial aggregation entry too
    expect(getVolumeActivity('smb-nas')).toBeDefined()
    expect(getVolumeAggregation('smb-nas')).toBeDefined()
    expect(isAnyVolumeIndexing()).toBe(true)

    scanAbortedCb({ volumeId: 'smb-nas' })

    // The stuck-row bug fix: the activity (and any partial aggregation) is gone,
    // so the corner indicator and badge tooltip don't keep a "scanning" row.
    expect(getVolumeActivity('smb-nas')).toBeUndefined()
    expect(getVolumeAggregation('smb-nas')).toBeUndefined()
    expect(isAnyVolumeIndexing()).toBe(false)
  })

  it('aborting one volume leaves another scanning volume untouched', () => {
    if (!scanProgressCb) throw new Error('scan-progress callback not registered')
    if (!scanAbortedCb) throw new Error('scan-aborted callback not registered')

    scanProgressCb({ volumeId: 'smb-a', entriesScanned: 100, dirsFound: 1, bytesScanned: 0 })
    scanProgressCb({ volumeId: 'mtp-phone', entriesScanned: 7, dirsFound: 1, bytesScanned: 0 })

    scanAbortedCb({ volumeId: 'smb-a' })

    expect(getVolumeActivity('smb-a')).toBeUndefined()
    expect(getVolumeActivity('mtp-phone')?.entriesScanned).toBe(7)
  })

  it('scopes isVolumeScanning to the scanning volume only', () => {
    if (!scanProgressCb) throw new Error('scan-progress callback not registered')

    // Only smb-b is scanning. A per-folder hourglass on volume A (root) must NOT
    // light up — the bug the global `isScanning()` caused.
    scanProgressCb({ volumeId: 'smb-b', entriesScanned: 100, dirsFound: 1, bytesScanned: 0 })
    expect(isVolumeScanning('smb-b')).toBe(true)
    expect(isVolumeScanning('root')).toBe(false)
    expect(isVolumeScanning('mtp-phone')).toBe(false)
  })

  it('reports isVolumeScanning false for a replaying (not scanning) volume', () => {
    if (!replayProgressCb) throw new Error('replay-progress callback not registered')

    // Replay is live activity but NOT a scan; the scan-only hourglass stays off.
    replayProgressCb({ volumeId: 'root', eventsProcessed: 10, estimatedTotal: 100 })
    expect(getVolumeActivity('root')?.phase).toBe('replaying')
    expect(isVolumeScanning('root')).toBe(false)
  })

  it('resets the phase start clock on a phase change but keeps it within a phase', () => {
    emitProgress('root', 'computing', 100, 1000)
    const firstStart = expectAggregation('root').startedAt

    // Same phase: startedAt is preserved so the ETA window keeps extrapolating.
    emitProgress('root', 'computing', 500, 1000)
    expect(expectAggregation('root').startedAt).toBe(firstStart)

    // New phase: startedAt resets so the next phase's ETA starts fresh.
    emitProgress('root', 'writing', 0, 1000)
    expect(expectAggregation('root').startedAt).toBeGreaterThanOrEqual(firstStart)
    expect(expectAggregation('root').phase).toBe('writing')
  })
})

describe('index-state per-volume pipeline phase', () => {
  beforeEach(async () => {
    destroyIndexState()
    phaseCb = undefined
    scanAbortedCb = undefined
    await initIndexState()
  })

  function emitPhase(volumeId: string, phase: ActivityPhase): void {
    if (!phaseCb) throw new Error('phase-changed callback not registered')
    phaseCb({ volumeId, phase })
  }

  it('records the latest mid-pipeline phase per volume', () => {
    emitPhase('root', 'scanning')
    expect(getVolumePhase('root')).toBe('scanning')

    // A later transition replaces it (the scan finished, aggregation began).
    emitPhase('root', 'aggregating')
    expect(getVolumePhase('root')).toBe('aggregating')

    emitPhase('root', 'reconciling')
    expect(getVolumePhase('root')).toBe('reconciling')
  })

  it("keeps each volume's phase independent", () => {
    emitPhase('root', 'reconciling')
    emitPhase('smb-nas', 'scanning')

    expect(getVolumePhase('root')).toBe('reconciling')
    expect(getVolumePhase('smb-nas')).toBe('scanning')
  })

  it('clears the phase on the terminal live transition', () => {
    emitPhase('root', 'reconciling')
    expect(getVolumePhase('root')).toBe('reconciling')

    // `live` means the pipeline finished: no active step, so the entry is dropped.
    emitPhase('root', 'live')
    expect(getVolumePhase('root')).toBeUndefined()
  })

  it('clears the phase on idle (stop / shutdown / disconnect)', () => {
    emitPhase('root', 'scanning')
    emitPhase('root', 'idle')
    expect(getVolumePhase('root')).toBeUndefined()
  })

  it('clears the phase when a network scan aborts (cancel/fail fires no phase event)', () => {
    if (!scanAbortedCb) throw new Error('scan-aborted callback not registered')
    emitPhase('smb-nas', 'scanning')
    expect(getVolumePhase('smb-nas')).toBe('scanning')

    scanAbortedCb({ volumeId: 'smb-nas' })
    expect(getVolumePhase('smb-nas')).toBeUndefined()
  })

  it('reports undefined for a volume that never started a pipeline', () => {
    expect(getVolumePhase('mtp-phone')).toBeUndefined()
  })

  it('keeps the surface visible through a phase-only step (reconcile, no live entry)', () => {
    // Scan + aggregation both finished (no live entry), only the phase event marks
    // the reconcile. The hourglass must stay up so the catch-up step is visible.
    emitPhase('root', 'reconciling')
    expect(isAnyVolumeIndexing()).toBe(true)
    expect(getActivePhaseVolumeIds()).toEqual(['root'])

    emitPhase('root', 'live')
    expect(isAnyVolumeIndexing()).toBe(false)
    expect(getActivePhaseVolumeIds()).toEqual([])
  })
})

describe('index-state per-volume scan run kind (the run-kind header fact)', () => {
  beforeEach(async () => {
    destroyIndexState()
    scanStartedCb = undefined
    scanProgressCb = undefined
    scanAbortedCb = undefined
    phaseCb = undefined
    branchStartedCb = undefined
    branchEndedCb = undefined
    await initIndexState()
  })

  function emitStarted(
    volumeId: string,
    scanRunKind: ScanRunKind,
    priorTotalEntries: number | null = null,
    coveredInPhases = false,
  ): void {
    if (!scanStartedCb) throw new Error('scan-started callback not registered')
    scanStartedCb({
      volumeId,
      scanRunKind,
      priorTotalEntries,
      priorScanDurationMs: null,
      volumeUsedBytes: null,
      coveredInPhases,
    })
  }

  it('takes the backend answer verbatim, including the case a prior-totals guess got wrong', () => {
    emitStarted('root', 'change_check', 405356)
    expect(getVolumeScanRunKind('root')).toBe('change_check')

    emitStarted('smb-nas', 'first_scan')
    expect(getVolumeScanRunKind('smb-nas')).toBe('first_scan')

    // A populated index whose last scan never completed: prior totals exist, yet
    // the walk truncates and rebuilds. Guessing from the totals called this a
    // change check; the backend says rebuild, and the backend wins.
    emitStarted('ext-usb', 'full_rebuild', 405356)
    expect(getVolumeScanRunKind('ext-usb')).toBe('full_rebuild')
  })

  it('survives past the live scan entry, so the header holds through aggregation', () => {
    if (!phaseCb) throw new Error('phase-changed callback not registered')
    emitStarted('root', 'change_check', 1000)
    // The scan finished (activity entry gone), aggregation is running: the
    // pipeline isn't terminal, so the kind must still be known.
    phaseCb({ volumeId: 'root', phase: 'aggregating' })
    expect(getVolumeScanRunKind('root')).toBe('change_check')
  })

  it('expires on the terminal live transition and on an abort', () => {
    if (!phaseCb || !scanAbortedCb) throw new Error('callbacks not registered')
    emitStarted('root', 'full_rebuild', 1000)
    phaseCb({ volumeId: 'root', phase: 'live' })
    expect(getVolumeScanRunKind('root')).toBeUndefined()

    emitStarted('smb-nas', 'first_scan')
    scanAbortedCb({ volumeId: 'smb-nas' })
    expect(getVolumeScanRunKind('smb-nas')).toBeUndefined()
  })

  it('stays unknown for a volume seeded by a progress tick alone (mid-scan reload)', () => {
    if (!scanProgressCb) throw new Error('scan-progress callback not registered')
    scanProgressCb({ volumeId: 'smb-nas', entriesScanned: 10, dirsFound: 2, bytesScanned: 100 })
    // No started event and no status backfill ⇒ no run-kind fact ⇒ the header is
    // omitted, not guessed.
    expect(getVolumeScanRunKind('smb-nas')).toBeUndefined()
  })
})

// These guard the live-counter REACTIVITY, not just the data. The getter tests
// above read the stored value directly, so they pass even when a `SvelteMap.set`
// re-sets the SAME mutated object reference (which `SvelteMap` treats as a no-op
// and never notifies). Here we run a real `$effect` over the getter and assert it
// re-fires on the SECOND progress event — the notification the frozen-counter bug
// dropped. See the fresh-object gotcha in DETAILS § "State model".
describe('index-state scan-progress reactivity', () => {
  beforeEach(async () => {
    destroyIndexState()
    scanProgressCb = undefined
    replayProgressCb = undefined
    await initIndexState()
  })

  it('re-fires reactive consumers on every scan-progress tick (root entriesScanned)', () => {
    if (!scanProgressCb) throw new Error('scan-progress callback not registered')

    const seen: number[] = []
    const cleanup = $effect.root(() => {
      $effect(() => {
        seen.push(getEntriesScanned())
      })
    })

    flushSync() // initial effect run: 0 (no root activity yet)
    scanProgressCb({ volumeId: 'root', entriesScanned: 40_000, dirsFound: 100, bytesScanned: 1_000 })
    flushSync()
    scanProgressCb({ volumeId: 'root', entriesScanned: 2_000_000, dirsFound: 5_000, bytesScanned: 99_999 })
    flushSync()

    cleanup()

    // Pre-fix this stops at [0, 40000]: the second set re-uses the same object
    // reference, so SvelteMap never notifies and the effect never sees 2M.
    expect(seen).toEqual([0, 40_000, 2_000_000])
  })

  it('re-fires reactive consumers on every replay-progress tick', () => {
    if (!replayProgressCb) throw new Error('replay-progress callback not registered')

    const seen: number[] = []
    const cleanup = $effect.root(() => {
      $effect(() => {
        seen.push(getVolumeActivity('root')?.replayEventsProcessed ?? -1)
      })
    })

    flushSync() // initial: -1 (no activity)
    replayProgressCb({ volumeId: 'root', eventsProcessed: 500, estimatedTotal: 10_000 })
    flushSync()
    replayProgressCb({ volumeId: 'root', eventsProcessed: 9_500, estimatedTotal: 10_000 })
    flushSync()

    cleanup()

    // Pre-fix this stops at [-1, 500]: the second set re-uses the same reference.
    expect(seen).toEqual([-1, 500, 9_500])
  })
})

describe('index-state walked ground', () => {
  beforeEach(async () => {
    destroyIndexState()
    scanStartedCb = undefined
    scanProgressCb = undefined
    scanAbortedCb = undefined
    phaseCb = undefined
    branchStartedCb = undefined
    branchEndedCb = undefined
    await initIndexState()
  })

  function emitStarted(volumeId: string, coveredInPhases: boolean): void {
    if (!scanStartedCb) throw new Error('scan-started callback not registered')
    scanStartedCb({
      volumeId,
      scanRunKind: 'first_scan',
      priorTotalEntries: null,
      priorScanDurationMs: null,
      volumeUsedBytes: null,
      coveredInPhases,
    })
  }

  function startWalking(volumeId: string, root: string): void {
    if (!branchStartedCb) throw new Error('coverage-branch-started callback not registered')
    branchStartedCb({ volumeId, roots: [root] })
  }

  function stopWalking(volumeId: string, root: string): void {
    if (!branchEndedCb) throw new Error('coverage-branch-ended callback not registered')
    branchEndedCb({ volumeId, roots: [root] })
  }

  it('starts a run with nothing in flux, whatever kind of run it is', () => {
    // ❌ Nothing is seeded from the scan-started event: the ground arrives on the
    // coverage-branch events, so there is no window where the frontend guesses
    // which kind of run this is and marks the wrong thing.
    emitStarted('root', true)
    expect(isPathAffectedByWalk(getWalkedGround('root'), '/Users/someone')).toBe(false)

    emitStarted('smb-nas', false)
    expect(isPathAffectedByWalk(getWalkedGround('smb-nas'), '/Volumes/nas/photos')).toBe(false)
  })

  it('puts the whole drive in flux when the run announces the volume root', () => {
    // What a whole-volume walk announces. One shape, one predicate: no sentinel
    // and no second kind of run for a row to branch on.
    emitStarted('root', false)
    startWalking('root', '/')

    expect(isPathAffectedByWalk(getWalkedGround('root'), '/anywhere')).toBe(true)
    expect(isPathAffectedByWalk(getWalkedGround('root'), '/')).toBe(true)
  })

  it('narrows to the branch under the walker, and lets go of it when it ends', () => {
    emitStarted('root', true)

    startWalking('root', '/Users/someone/Downloads')
    expect(isPathAffectedByWalk(getWalkedGround('root'), '/Users/someone/Downloads/big')).toBe(true)
    expect(isPathAffectedByWalk(getWalkedGround('root'), '/opt')).toBe(false)

    stopWalking('root', '/Users/someone/Downloads')
    expect(isPathAffectedByWalk(getWalkedGround('root'), '/Users/someone/Downloads/big')).toBe(false)
  })

  it('drops only the branch that ended', () => {
    emitStarted('root', true)
    if (!branchStartedCb || !branchEndedCb) throw new Error('coverage-branch callbacks not registered')

    branchStartedCb({ volumeId: 'root', roots: ['/opt/one', '/opt/two'] })
    branchEndedCb({ volumeId: 'root', roots: ['/opt/one'] })

    expect(getWalkedGround('root')).toEqual(['/opt/two'])
  })

  it('never lets a branch event on one drive move another drive', () => {
    emitStarted('root', true)
    emitStarted('smb-nas', true)

    startWalking('smb-nas', '/Users/someone/Downloads')

    expect(isPathAffectedByWalk(getWalkedGround('root'), '/Users/someone/Downloads')).toBe(false)
    expect(isPathAffectedByWalk(getWalkedGround('smb-nas'), '/Users/someone/Downloads')).toBe(true)
  })

  it("forgets a volume's ground when its run ends, on both terminal events", () => {
    if (!scanAbortedCb) throw new Error('scan-aborted callback not registered')
    emitStarted('root', true)
    startWalking('root', '/Users/someone/Downloads')

    scanAbortedCb({ volumeId: 'root' })

    expect(getWalkedGround('root')).toEqual([])
  })

  it('re-fires reactive consumers when the ground moves', () => {
    emitStarted('root', true)

    const seen: boolean[] = []
    const cleanup = $effect.root(() => {
      $effect(() => {
        seen.push(isPathAffectedByWalk(getWalkedGround('root'), '/Users/someone/Downloads/big'))
      })
    })

    flushSync()
    startWalking('root', '/Users/someone/Downloads')
    flushSync()
    stopWalking('root', '/Users/someone/Downloads')
    flushSync()

    cleanup()

    expect(seen).toEqual([false, true, false])
  })
})

describe('which phase of a first index is running', () => {
  beforeEach(async () => {
    destroyIndexState()
    await initIndexState()
  })

  function emitPhase(volumeId: string, label: CoveragePhaseLabel): void {
    if (!coveragePhaseCb) throw new Error('coverage-phase-started callback not registered')
    coveragePhaseCb({ volumeId, label })
  }

  it('has no answer until the backend gives one, so the header can fall back', () => {
    expect(getVolumeCoveragePhase('root')).toBeUndefined()
  })

  it('carries the backend label through, and moves on with the run', () => {
    emitPhase('root', 'priorityFolders')
    expect(getVolumeCoveragePhase('root')).toBe('priorityFolders')

    emitPhase('root', 'home')
    expect(getVolumeCoveragePhase('root')).toBe('home')

    emitPhase('root', 'wholeDrive')
    expect(getVolumeCoveragePhase('root')).toBe('wholeDrive')
  })

  it('is per volume, like every other index fact', () => {
    emitPhase('root', 'home')
    emitPhase('smb-nas', 'wholeDrive')

    expect(getVolumeCoveragePhase('root')).toBe('home')
    expect(getVolumeCoveragePhase('smb-nas')).toBe('wholeDrive')
  })

  it('expires with the run, so a finished drive keeps no stale header', () => {
    emitPhase('root', 'home')
    if (!phaseCb) throw new Error('phase-changed callback not registered')
    phaseCb({ volumeId: 'root', phase: 'live' })

    expect(getVolumeCoveragePhase('root')).toBeUndefined()
  })
})
