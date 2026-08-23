/**
 * Tier 3 a11y tests for the two indexing components that need module stubs: the
 * corner indicator and the one-time stale-drive dialog.
 *
 * `svelte-tests` charges per test FILE, not per test (`docs/testing.md` § "What a
 * test actually costs"), so these two share a file. They disagree on
 * `$lib/settings` (`getSetting` false vs true) and on `$lib/stores/volume-store`
 * (two volumes vs one), so both are mutable stubs each block installs in its own
 * `beforeEach`.
 *
 * The mock-free indexing components live in `presentational.a11y.test.ts`: the
 * `./index-state.svelte` and volume-store stubs here would change what
 * `IndexingDriveSummary` renders, which reads both for real.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, flushSync } from 'svelte'
import IndexingStatusIndicator from './IndexingStatusIndicator.svelte'
import StaleDriveDialog from './StaleDriveDialog.svelte'
import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
import type { ActivityPhase, IndexFreshnessChangedEvent } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

let activeVolumes: VolumeIndexActivity[] = []
// Per-volume aggregation, keyed by volumeId (mirrors the real `aggregation` map).
let aggregationByVolume: Record<string, AggregationActivity> = {}
// Per-volume top-level phase, keyed by volumeId (mirrors the real `phase` map).
let phaseByVolume: Record<string, ActivityPhase> = {}

// The volume list each block wants, and the `getSetting` answer each block wants.
// Both blocks install their own in `beforeEach`, so neither inherits the other's.
let volumes: unknown[] = []
let settingValue: unknown = undefined

// Capture the freshness-event callback the dialog registers so the test can fire it.
let freshnessCb: ((p: IndexFreshnessChangedEvent) => void) | undefined

vi.mock('./index-state.svelte', () => ({
  ROOT_VOLUME_ID: 'root',
  getActiveIndexVolumes: () => activeVolumes,
  isAnyVolumeIndexing: () =>
    activeVolumes.length > 0 || Object.keys(aggregationByVolume).length > 0 || Object.keys(phaseByVolume).length > 0,
  getVolumeAggregation: (volumeId: string) => aggregationByVolume[volumeId],
  getAggregatingVolumeIds: () => Object.keys(aggregationByVolume),
  getActivePhaseVolumeIds: () => Object.keys(phaseByVolume),
  getVolumePhase: (volumeId: string) => phaseByVolume[volumeId],
  getVolumeScanRunKind: () => undefined,
  isVolumeCoveredInPhases: () => false,
  getVolumeCoveragePhase: () => undefined,
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
  getVolumes: () => volumes,
}))

// The union of the settings surface both components reach for. The real module is
// spread first so anything outside the union behaves as it does un-merged.
vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: () => settingValue,
  setSetting: () => {},
  onSpecificSettingChange: () => () => {},
}))

vi.mock('$lib/media-index/enabled-volumes', () => ({
  getEnabledMediaIndexVolumeIds: () => [],
}))

vi.mock('$lib/tauri-commands/indexing', () => ({
  onIndexFreshnessChanged: (cb: (p: IndexFreshnessChangedEvent) => void) => {
    freshnessCb = cb
    return Promise.resolve(() => {})
  },
}))

vi.mock('./drive-index-prefs', () => ({
  hasShownFirstStaleDialog: () => false,
  markFirstStaleDialogShown: () => {},
}))

// ModalDialog notifies the backend on open/close; stub those IPC calls.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

// Both components share one jsdom document, and the dialog portals into
// `document.body`, where axe resolves ARIA id references document-wide. Clearing
// between tests keeps each audit looking at its own container only.
afterEach(() => {
  document.body.innerHTML = ''
})

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
describe('IndexingStatusIndicator a11y', () => {
  beforeEach(() => {
    volumes = [
      { id: 'root', name: 'Macintosh HD' },
      { id: 'smb-nas', name: 'Backups' },
    ]
    // The queued-enrichment line's inputs: master toggle off and no eligible volumes,
    // so the line stays out of these scenarios (it has its own pure-predicate tests).
    settingValue = false
  })

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

/**
 * Tier 3 a11y tests for `StaleDriveDialog.svelte`: the one-time "your drive went
 * stale" dialog must have no axe violations once open. The dialog renders nothing
 * until a first external Fresh→Stale event arrives, so the mocks above let the
 * test fire that event (mirroring `StaleDriveDialog.test.ts`).
 */
describe('StaleDriveDialog a11y', () => {
  beforeEach(() => {
    volumes = [{ id: 'smb-backups', name: 'Backups', path: 'smb://x', category: 'network', isEjectable: false }]
    settingValue = true
  })

  it('the open dialog has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(StaleDriveDialog, { target })
    flushSync()

    freshnessCb?.({ volumeId: 'smb-backups', freshness: 'stale' })
    await tick()
    flushSync()

    await expectNoA11yViolations(target)
  })
})
