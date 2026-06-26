/**
 * Tier 3 a11y tests for `IndexingStatusIndicator.svelte`.
 *
 * The component reads the multi-drive index state from `index-state.svelte` and
 * resolves drive names from the volume store. Both are stubbed here so we can
 * render the indicator in idle, single-drive, and multi-drive modes without
 * touching the real indexer. The mock factories close over module-scoped `let`
 * variables that each test reassigns BEFORE mounting (Vitest hoists `vi.mock`,
 * so a per-test factory wouldn't work).
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import IndexingStatusIndicator from './IndexingStatusIndicator.svelte'
import type { VolumeIndexActivity } from './index-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let activeVolumes: VolumeIndexActivity[] = []
let aggregating = false
let aggPhase = 'computing'
let aggVolumeId = 'root'

function scanActivity(volumeId: string, overrides: Partial<VolumeIndexActivity> = {}): VolumeIndexActivity {
  return {
    volumeId,
    phase: 'scanning',
    entriesScanned: 42000,
    dirsFound: 1200,
    bytesScanned: 1_000_000,
    scanStartedAt: Date.now() - 4000,
    priorTotalEntries: null,
    priorScanDurationMs: 120000,
    volumeUsedBytes: null,
    replayEventsProcessed: 0,
    replayEstimatedTotal: 0,
    replayStartedAt: 0,
    ...overrides,
  }
}

vi.mock('./index-state.svelte', () => ({
  getActiveIndexVolumes: () => activeVolumes,
  isAnyVolumeIndexing: () => activeVolumes.length > 0 || aggregating,
  isAggregating: () => aggregating,
  getAggregationPhase: () => aggPhase,
  getAggregationCurrent: () => 500,
  getAggregationTotal: () => 1000,
  getAggregationStartedAt: () => Date.now() - 3000,
  getAggregatingVolumeId: () => aggVolumeId,
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD' },
    { id: 'smb-nas', name: 'Backups' },
  ],
}))

describe('IndexingStatusIndicator a11y', () => {
  it('idle (no activity) renders nothing', async () => {
    activeVolumes = []
    aggregating = false
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('scanning (counter-only, no denominator) shows the icon with no a11y violations', async () => {
    activeVolumes = [scanActivity('root', { priorTotalEntries: null, volumeUsedBytes: null })]
    aggregating = false
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('scanning with calibrated progress shows the bar with no a11y violations', async () => {
    activeVolumes = [scanActivity('root', { priorTotalEntries: 100000 })]
    aggregating = false
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    expect(target.querySelector('.tooltip-progress')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('aggregating with progress has no a11y violations', async () => {
    activeVolumes = []
    aggregating = true
    aggPhase = 'computing'
    aggVolumeId = 'root'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('multiple drives scanning shows a heading per drive with no a11y violations', async () => {
    activeVolumes = [
      scanActivity('root', { priorTotalEntries: 100000 }),
      scanActivity('smb-nas', { priorTotalEntries: 50000 }),
    ]
    aggregating = false
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    // One heading per drive when more than one is active.
    expect(target.querySelectorAll('.drive-heading').length).toBe(2)
    await expectNoA11yViolations(target)
  })
})
