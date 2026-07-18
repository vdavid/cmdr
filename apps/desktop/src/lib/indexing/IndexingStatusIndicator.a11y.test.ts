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
import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
import type { ActivityPhase } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

let activeVolumes: VolumeIndexActivity[] = []
// Per-volume aggregation, keyed by volumeId (mirrors the real `aggregation` map).
let aggregationByVolume: Record<string, AggregationActivity> = {}
// Per-volume top-level phase, keyed by volumeId (mirrors the real `phase` map).
let phaseByVolume: Record<string, ActivityPhase> = {}

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
  ROOT_VOLUME_ID: 'root',
  getActiveIndexVolumes: () => activeVolumes,
  isAnyVolumeIndexing: () =>
    activeVolumes.length > 0 || Object.keys(aggregationByVolume).length > 0 || Object.keys(phaseByVolume).length > 0,
  getVolumeAggregation: (volumeId: string) => aggregationByVolume[volumeId],
  getAggregatingVolumeIds: () => Object.keys(aggregationByVolume),
  getActivePhaseVolumeIds: () => Object.keys(phaseByVolume),
  getVolumePhase: (volumeId: string) => phaseByVolume[volumeId],
  getVolumeScanKind: () => undefined,
  placeholderActivity: (volumeId: string): VolumeIndexActivity => ({
    volumeId,
    phase: 'scanning',
    entriesScanned: 0,
    dirsFound: 0,
    bytesScanned: 0,
    scanStartedAt: 0,
    priorTotalEntries: null,
    priorScanDurationMs: null,
    volumeUsedBytes: null,
    replayEventsProcessed: 0,
    replayEstimatedTotal: 0,
    replayStartedAt: 0,
  }),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD' },
    { id: 'smb-nas', name: 'Backups' },
  ],
}))

// The queued-enrichment line's inputs: master toggle off and no eligible volumes,
// so the line stays out of these scenarios (it has its own pure-predicate tests).
vi.mock('$lib/settings', () => ({
  getSetting: () => false,
  onSpecificSettingChange: () => () => {},
}))

vi.mock('$lib/media-index/enabled-volumes', () => ({
  getEnabledMediaIndexVolumeIds: () => [],
}))

describe('IndexingStatusIndicator a11y', () => {
  it('idle (no activity) renders nothing', async () => {
    activeVolumes = []
    aggregationByVolume = {}
    phaseByVolume = {}
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('single-drive scanning (counter-only, no denominator) names the drive and shows no bar', async () => {
    activeVolumes = [scanActivity('root', { priorTotalEntries: null, volumeUsedBytes: null })]
    aggregationByVolume = {}
    phaseByVolume = {}
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    // The drive-name heading now shows even for a single drive.
    expect(target.querySelector('.drive-heading')?.textContent).toBe('Macintosh HD')
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('single-drive first scan (tier 2) shows count + elapsed and NO progress bar', async () => {
    // A rough first scan: a byte denominator but no prior-scan calibration, so
    // there's no trustworthy percent — count + elapsed only, no bar.
    activeVolumes = [scanActivity('root', { priorTotalEntries: null, volumeUsedBytes: 10_000_000 })]
    aggregationByVolume = {}
    phaseByVolume = {}
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    expect(target.querySelector('.drive-heading')?.textContent).toBe('Macintosh HD')
    // No bar and no progressbar role for the rough first scan.
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    // The count is still present (the live label screen readers announce).
    expect(target.querySelector('.tooltip-detail')?.textContent).toContain('42,000')
    // Elapsed clock present (scanStartedAt is 4 s ago).
    expect(target.querySelector('.tooltip-detail')?.textContent).toMatch(/·\s*\d+:\d{2}/)
    await expectNoA11yViolations(target)
  })

  it('single-drive scanning with calibrated progress names the drive and shows the bar', async () => {
    activeVolumes = [scanActivity('root', { priorTotalEntries: 100000 })]
    aggregationByVolume = {}
    phaseByVolume = {}
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    expect(target.querySelector('.drive-heading')?.textContent).toBe('Macintosh HD')
    expect(target.querySelector('.tooltip-progress')).not.toBeNull()
    expect(target.querySelector('[role="progressbar"]')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('aggregating with progress has no a11y violations', async () => {
    activeVolumes = []
    aggregationByVolume = {
      root: { phase: 'computing', current: 500, total: 1000, startedAt: Date.now() - 3000 },
    }
    phaseByVolume = {}
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('multiple drives: the primary expands to a full checklist, the rest collapse to one line', async () => {
    activeVolumes = [
      scanActivity('root', { priorTotalEntries: 100000 }),
      scanActivity('smb-nas', { priorTotalEntries: 50000 }),
    ]
    aggregationByVolume = {}
    phaseByVolume = {}
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    // A heading per drive (the expanded row + the collapsed summary both name theirs).
    expect(target.querySelectorAll('.drive-heading').length).toBe(2)
    // Only the primary drive expands to the step checklist; the secondary collapses.
    expect(target.querySelectorAll('.step-list').length).toBe(1)
    expect(target.querySelectorAll('.drive-summary').length).toBe(1)
    await expectNoA11yViolations(target)
  })

  it('a volume mid-reconcile (phase only, no live entry) stays visible with catch up active', async () => {
    // Scan + aggregation both finished; only the phase event marks the reconcile.
    activeVolumes = []
    aggregationByVolume = {}
    phaseByVolume = { root: 'reconciling' }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusIndicator, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    expect(target.querySelector('.step-list')).not.toBeNull()
    // The catch-up step is the active one (its label carries the active class).
    const active = target.querySelector('.step-active .step-label')?.textContent
    expect(active).toBe('Catch up on recent changes')
    await expectNoA11yViolations(target)
  })
})
