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
import type { AggregationProgressEvent, IndexAggregationCompleteEvent } from '$lib/ipc/bindings'

// Captured callbacks the module registers via the wrappers below.
let aggProgressCb: ((p: AggregationProgressEvent) => void) | undefined
let aggCompleteCb: ((p: IndexAggregationCompleteEvent) => void) | undefined

const noopUnlisten = () => {}

// Mock the typed event wrappers: capture the aggregation ones, no-op the rest.
vi.mock('$lib/tauri-commands', () => ({
  onIndexScanStarted: () => Promise.resolve(noopUnlisten),
  onIndexScanProgress: () => Promise.resolve(noopUnlisten),
  onIndexScanComplete: () => Promise.resolve(noopUnlisten),
  onIndexAggregationProgress: (cb: (p: AggregationProgressEvent) => void) => {
    aggProgressCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexAggregationComplete: (cb: (p: IndexAggregationCompleteEvent) => void) => {
    aggCompleteCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexRescanNotification: () => Promise.resolve(noopUnlisten),
  onIndexReplayProgress: () => Promise.resolve(noopUnlisten),
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
  getAggregatingVolumeIds,
  isAggregating,
  isAnyVolumeIndexing,
  type AggregationActivity,
} from './index-state.svelte'

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
    await initIndexState()
  })

  it('tracks two volumes aggregating concurrently, each with its own progress', () => {
    emitProgress('root', 'computing', 250, 1000)
    emitProgress('smb-nas', 'writing', 40, 50)

    expect(getVolumeAggregation('root')).toMatchObject({ phase: 'computing', current: 250, total: 1000 })
    expect(getVolumeAggregation('smb-nas')).toMatchObject({ phase: 'writing', current: 40, total: 50 })
    // Each is attributed to its own volume — not conflated into one signal.
    expect(getAggregatingVolumeIds().sort()).toEqual(['root', 'smb-nas'])
    expect(isAggregating()).toBe(true)
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
    expect(isAggregating()).toBe(true)

    emitComplete('smb-nas')
    expect(isAggregating()).toBe(false)
    expect(getAggregatingVolumeIds()).toEqual([])
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
