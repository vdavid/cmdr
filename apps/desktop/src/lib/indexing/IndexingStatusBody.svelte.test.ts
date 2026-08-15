/**
 * Mount tests for `IndexingStatusBody.svelte`, the shared presentational step
 * checklist rendered by both surfaces (the corner indicator's drive rows and the
 * breadcrumb badge's scanning tooltip). It's pure props-driven (activity +
 * aggregation + an injected `now` tick + the wrapper's `windowedEta` + `phase` +
 * `isNetwork`), so each scenario is a `mount` with the right fixture. `tString`
 * resolves the real `en` catalog. The contract under test: steps are composed
 * from the events that fired (find/save/compute/catch-up for local, fewer for
 * network, one for replay), each carries its state, and the active step shows the
 * live detail with the count-first policy (calibrated → bar, first scan →
 * count + elapsed, no bar).
 */
import { describe, it, expect } from 'vitest'
import { mount, flushSync } from 'svelte'
import IndexingStatusBody from './IndexingStatusBody.svelte'
import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
import type { ActivityPhase, CoveragePhaseLabel, ScanRunKind } from '$lib/ipc/bindings'

function scanActivity(overrides: Partial<VolumeIndexActivity> = {}): VolumeIndexActivity {
  return {
    volumeId: 'root',
    phase: 'scanning',
    entriesScanned: 42_000,
    dirsFound: 1_200,
    bytesScanned: 1_000_000,
    scanStartedAt: Date.now() - 4000,
    priorTotalEntries: null,
    priorScanDurationMs: 120_000,
    volumeUsedBytes: null,
    replayEventsProcessed: 0,
    replayEstimatedTotal: 0,
    replayStartedAt: 0,
    ...overrides,
  }
}

function replayActivity(overrides: Partial<VolumeIndexActivity> = {}): VolumeIndexActivity {
  return {
    ...scanActivity(),
    phase: 'replaying',
    replayEventsProcessed: 3_000,
    replayEstimatedTotal: 10_000,
    ...overrides,
  }
}

function render(props: {
  activity: VolumeIndexActivity
  aggregation?: AggregationActivity | undefined
  now?: number
  windowedEta?: string | null
  phase?: ActivityPhase | undefined
  isNetwork?: boolean
  coveredInPhases?: boolean
  coveragePhase?: CoveragePhaseLabel
  scanRunKind?: ScanRunKind
}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(IndexingStatusBody, {
    target,
    props: {
      aggregation: undefined,
      now: Date.now(),
      windowedEta: null,
      phase: undefined,
      isNetwork: false,
      coveredInPhases: false,
      ...props,
    },
  })
  flushSync()
  return target
}

/** The header above the checklist, or `null` when it renders none. */
function header(target: HTMLElement): string | null {
  return target.querySelector('.run-kind')?.textContent ?? null
}

/** The text of every rendered step label, in order. */
function stepLabels(target: HTMLElement): (string | null)[] {
  return [...target.querySelectorAll('.step-label')].map((el) => el.textContent)
}

/** The step kinds (by status class) keyed to their label, for state assertions. */
function stepStatus(target: HTMLElement, label: string): string | undefined {
  const li = [...target.querySelectorAll('.step')].find((el) => el.querySelector('.step-label')?.textContent === label)
  if (!li) return undefined
  if (li.classList.contains('step-active')) return 'active'
  if (li.classList.contains('step-done')) return 'done'
  if (li.classList.contains('step-pending')) return 'pending'
  return undefined
}

describe('IndexingStatusBody checklist', () => {
  it('local scan lists all four steps with find files active', () => {
    const target = render({ activity: scanActivity({ priorTotalEntries: 100_000 }), windowedEta: '1m left' })
    expect(stepLabels(target)).toEqual([
      'Find files',
      'Save the file list',
      'Compute folder sizes',
      'Catch up on recent changes',
    ])
    expect(stepStatus(target, 'Find files')).toBe('active')
    expect(stepStatus(target, 'Save the file list')).toBe('pending')
  })

  it('scan tier-1 (calibrated): the active find-files step shows the bar + percent', () => {
    const target = render({ activity: scanActivity({ priorTotalEntries: 100_000 }), windowedEta: '1m left' })
    expect(target.querySelector('[role="progressbar"]')).not.toBeNull()
    // 42,000 / 100,000 = 42%, joined with the injected ETA.
    expect(target.querySelector('.tooltip-percent')?.textContent).toContain('42%')
    expect(target.querySelector('.tooltip-percent')?.textContent).toContain('1m left')
  })

  it('scan tier-2 (first scan): count + elapsed, the first-scan hint, and NO bar', () => {
    const target = render({ activity: scanActivity({ priorTotalEntries: null, volumeUsedBytes: 10_000_000 }) })
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    expect(target.querySelector('.step-hint')?.textContent).toContain('First scan')
    const detail = target.querySelector('.tooltip-detail')?.textContent ?? ''
    expect(detail).toContain('42,000')
    expect(detail).toMatch(/·\s*\d+:\d{2}/) // elapsed clock
  })

  it('counter-only scan (no denominator) shows no bar', () => {
    const target = render({ activity: scanActivity({ priorTotalEntries: null, volumeUsedBytes: null }) })
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    expect(target.querySelector('.tooltip-detail')?.textContent).toContain('42,000')
  })

  it('aggregation (computing): compute step active with its sub-phase line + bar, earlier steps done', () => {
    const target = render({
      activity: scanActivity(),
      aggregation: { phase: 'computing', current: 500, total: 1_000, startedAt: Date.now() - 3000 },
    })
    expect(stepStatus(target, 'Find files')).toBe('done')
    expect(stepStatus(target, 'Save the file list')).toBe('done')
    expect(stepStatus(target, 'Compute folder sizes')).toBe('active')
    expect(target.textContent).toContain('Computing folder sizes')
    expect(target.querySelector('[role="progressbar"]')).not.toBeNull()
  })

  it('reconcile phase: catch up active, everything before it done, no detail', () => {
    const target = render({ activity: scanActivity(), phase: 'reconciling' })
    expect(stepStatus(target, 'Compute folder sizes')).toBe('done')
    expect(stepStatus(target, 'Catch up on recent changes')).toBe('active')
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
  })

  it('a change check words its second step as an update and reassures about folder sizes', () => {
    const target = render({
      activity: scanActivity({ priorTotalEntries: 100_000 }),
      scanRunKind: 'change_check',
      windowedEta: '18m left',
    })
    expect(stepLabels(target)).toEqual([
      'Find files',
      'Update the file list',
      'Compute folder sizes',
      'Catch up on recent changes',
    ])
    // The whole point of the hint: a 20-minute bar is fine to ignore because the
    // sizes on screen stay put.
    expect(target.querySelector('.step-hint')?.textContent).toContain('folder sizes stay visible')
  })

  it('a full rebuild keeps the save wording (its list IS rewritten from scratch)', () => {
    const target = render({ activity: scanActivity({ priorTotalEntries: 100_000 }), scanRunKind: 'full_rebuild' })
    expect(stepLabels(target)).toContain('Save the file list')
    expect(target.querySelector('.step-hint')).toBeNull()
  })

  it('network scan omits the save and catch-up steps', () => {
    const target = render({ activity: scanActivity({ volumeId: 'smb-nas' }), isNetwork: true })
    expect(stepLabels(target)).toEqual(['Find files', 'Compute folder sizes'])
  })

  it('replay collapses to a single update-index step with the event count + bar', () => {
    const target = render({ activity: replayActivity(), windowedEta: '5s left' })
    expect(stepLabels(target)).toEqual(['Update index'])
    expect(target.querySelector('.tooltip-detail')?.textContent).toContain('3,000')
    expect(target.querySelector('[role="progressbar"]')).not.toBeNull()
  })
})

describe('IndexingStatusBody header during a first index covered in phases', () => {
  /** A phased run, with the backend's own `first_scan` kind on it. */
  function phased(coveragePhase?: CoveragePhaseLabel) {
    return render({
      activity: scanActivity({ priorTotalEntries: null }),
      phase: 'scanning',
      coveredInPhases: true,
      coveragePhase,
      scanRunKind: 'first_scan',
    })
  }

  it('says which phase is running, so the ORDER is what the user reads', () => {
    expect(header(phased('priorityFolders'))).toBe('Indexing the folders you use most')
    expect(header(phased('home'))).toBe('Indexing the rest of your home folder')
    expect(header(phased('wholeDrive'))).toBe('Indexing the rest of the drive')
  })

  it('falls back to the run-kind header before any phase has been announced', () => {
    // The window between the run starting and its first phase event, and the one
    // a mid-run reload lands in. ❌ Never blank, and ❌ never "First full scan".
    expect(header(phased(undefined))).toBe('First index')
  })

  it('leaves a whole-volume scan on its own header', () => {
    const target = render({
      activity: scanActivity({ priorTotalEntries: 100_000 }),
      phase: 'scanning',
      scanRunKind: 'first_scan',
    })
    expect(header(target)).toBe('First full scan')
  })
})
