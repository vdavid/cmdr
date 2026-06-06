/**
 * Tier 3 a11y tests for `IndexingStatusIndicator.svelte`.
 *
 * The component reads module-level `$state` from `index-state.svelte`. That state is
 * stubbed here so we can render the indicator in idle, scanning, and aggregation modes
 * without touching the real indexer. The mock factory closes over module-scoped `let`
 * variables that each test reassigns BEFORE mounting (Vitest hoists `vi.mock`, so a
 * per-test factory wouldn't work).
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import IndexingStatusIndicator from './IndexingStatusIndicator.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let scanning = false
let aggregating = false
let aggPhase = 'computing'
let replaying = false
let priorTotalEntries: number | null = null
let volumeUsedBytes: number | null = null

vi.mock('./index-state.svelte', () => ({
  isScanning: () => scanning,
  getEntriesScanned: () => 42000,
  getDirsFound: () => 1200,
  getBytesScanned: () => 1_000_000,
  getScanStartedAt: () => Date.now() - 4000,
  getPriorTotalEntries: () => priorTotalEntries,
  getPriorScanDurationMs: () => 120000,
  getVolumeUsedBytes: () => volumeUsedBytes,
  isAggregating: () => aggregating,
  getAggregationPhase: () => aggPhase,
  getAggregationCurrent: () => 500,
  getAggregationTotal: () => 1000,
  getAggregationStartedAt: () => Date.now() - 3000,
  isReplaying: () => replaying,
  getReplayEventsProcessed: () => 5000,
  getReplayEstimatedTotal: () => 10000,
  getReplayStartedAt: () => Date.now() - 5000,
}))

describe('IndexingStatusIndicator a11y', () => {
  it('idle (no activity) renders nothing', async () => {
    scanning = false
    aggregating = false
    replaying = false
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('scanning (counter-only, no denominator) shows the icon with no a11y violations', async () => {
    scanning = true
    aggregating = false
    replaying = false
    priorTotalEntries = null
    volumeUsedBytes = null
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('scanning with calibrated progress shows the bar with no a11y violations', async () => {
    scanning = true
    aggregating = false
    replaying = false
    priorTotalEntries = 100000
    volumeUsedBytes = null
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    expect(target.querySelector('.tooltip-progress')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('aggregating with progress has no a11y violations', async () => {
    scanning = false
    aggregating = true
    aggPhase = 'computing'
    replaying = false
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
