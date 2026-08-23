/**
 * Tier 3 a11y tests for the indexing components that mock nothing: the toast, the
 * two tooltip rows, the collapsed drive summary, and the shared step checklist.
 *
 * One file per component would cost about five times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, fixtures, props,
 * and assertions; the fixtures stay inside their block because three of them define
 * a `scanActivity` with different defaults.
 *
 * `IndexingStatusIndicator` and `StaleDriveDialog` stay in `stateful.a11y.test.ts`:
 * they mock `./index-state.svelte` and `$lib/stores/volume-store.svelte`, and both
 * modules are read for real by `IndexingDriveSummary` here.
 */

import { describe, it, expect, afterEach } from 'vitest'
import { mount, tick, flushSync } from 'svelte'
import FirstConnectIndexToastContent from './FirstConnectIndexToastContent.svelte'
import IndexingDriveRow from './IndexingDriveRow.svelte'
import IndexingDriveSummary from './IndexingDriveSummary.svelte'
import IndexingEnrichRow from './IndexingEnrichRow.svelte'
import IndexingStatusBody from './IndexingStatusBody.svelte'
import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
import type { VolumeEnrichActivity } from './media-enrich-state.svelte'
import type { ActivityPhase } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

// These components share one jsdom document, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `FirstConnectIndexToastContent.svelte`: the first-connect
 * "index this drive?" toast (heading, body, and three action buttons) must have
 * no axe violations.
 */
describe('FirstConnectIndexToastContent a11y', () => {
  function mountToast() {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FirstConnectIndexToastContent, {
      target,
      props: {
        toastId: 'toast-1',
        volumeId: 'smb-backups',
        volumeName: 'Backups',
        onEnable: () => {},
        onSilenceDrive: () => {},
        onSilenceAll: () => {},
      },
    })
    flushSync()
    return target
  }

  it('the rendered toast has no violations', async () => {
    const target = mountToast()
    expect(target.querySelector('.first-connect-toast')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `IndexingDriveRow.svelte`.
 *
 * One row in the multi-drive indexing tooltip. It's a pure props-driven
 * presentational component (no store / Tauri deps), so each state is just a
 * `mount` with the right props: scanning (with and without a calibrated
 * progress bar), replaying, the aggregation phase folded into the row, and the
 * multi-drive heading. `tString` resolves the real `en` catalog.
 */
describe('IndexingDriveRow a11y', () => {
  function scanActivity(overrides: Partial<VolumeIndexActivity> = {}): VolumeIndexActivity {
    return {
      volumeId: 'root',
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

  function replayActivity(overrides: Partial<VolumeIndexActivity> = {}): VolumeIndexActivity {
    return {
      volumeId: 'root',
      phase: 'replaying',
      entriesScanned: 0,
      dirsFound: 0,
      bytesScanned: 0,
      scanStartedAt: 0,
      priorTotalEntries: null,
      priorScanDurationMs: null,
      volumeUsedBytes: null,
      replayEventsProcessed: 3000,
      replayEstimatedTotal: 10000,
      replayStartedAt: Date.now() - 4000,
      ...overrides,
    }
  }

  const baseProps = {
    // A scan activity by default; every test overrides `activity` explicitly. It's
    // here so the merged props type carries the required `activity` field.
    activity: scanActivity(),
    driveName: 'Macintosh HD',
    showHeading: false,
    aggregation: undefined,
  }

  async function mountRow(props: Record<string, unknown>): Promise<HTMLDivElement> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingDriveRow, { target, props: { ...baseProps, ...props } })
    await tick()
    return target
  }

  it('scanning, counter-only (no calibrated progress) has no a11y violations', async () => {
    const target = await mountRow({ activity: scanActivity({ priorTotalEntries: null, volumeUsedBytes: null }) })
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('scanning, first scan (tier 2) shows count + elapsed and no progress bar', async () => {
    // A byte denominator but no prior calibration → rough first scan: count +
    // elapsed clock, no bar and no progressbar role.
    const target = await mountRow({
      activity: scanActivity({ priorTotalEntries: null, volumeUsedBytes: 10_000_000 }),
    })
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    expect(target.querySelector('.tooltip-detail')?.textContent).toContain('42,000')
    expect(target.querySelector('.tooltip-detail')?.textContent).toMatch(/·\s*\d+:\d{2}/)
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('scanning with a calibrated progress bar has no a11y violations', async () => {
    const target = await mountRow({ activity: scanActivity({ priorTotalEntries: 100000 }) })
    expect(target.querySelector('.tooltip-progress')).not.toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('replaying has no a11y violations', async () => {
    const target = await mountRow({ activity: replayActivity() })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('with the drive-name heading shown has no a11y violations', async () => {
    const target = await mountRow({ activity: scanActivity({ priorTotalEntries: 100000 }), showHeading: true })
    expect(target.querySelector('.drive-heading')).not.toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('with an aggregation phase folded into the row has no a11y violations', async () => {
    const target = await mountRow({
      activity: scanActivity({ priorTotalEntries: 100000 }),
      aggregation: { phase: 'computing', current: 500, total: 1000, startedAt: Date.now() - 3000 },
    })
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier 3 a11y tests for `IndexingDriveSummary.svelte`, the collapsed one-line
 * summary for a SECONDARY drive in the corner indicator when several drives index
 * at once (the primary expands to its full checklist). Pure props-driven (it reads
 * the real `index-state` for the volume's phase, which returns `undefined` here),
 * so each scenario is a `mount`. `tString` resolves the real `en` catalog.
 */
describe('IndexingDriveSummary a11y', () => {
  function scanActivity(overrides: Partial<VolumeIndexActivity> = {}): VolumeIndexActivity {
    return {
      volumeId: 'smb-nas',
      phase: 'scanning',
      entriesScanned: 42000,
      dirsFound: 1200,
      bytesScanned: 1_000_000,
      scanStartedAt: Date.now() - 4000,
      priorTotalEntries: null,
      priorScanDurationMs: null,
      volumeUsedBytes: null,
      replayEventsProcessed: 0,
      replayEstimatedTotal: 0,
      replayStartedAt: 0,
      ...overrides,
    }
  }

  async function mountSummary(props: {
    activity: VolumeIndexActivity
    aggregation?: AggregationActivity | undefined
    driveName?: string
  }): Promise<HTMLDivElement> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingDriveSummary, {
      target,
      props: { aggregation: undefined, driveName: 'Backups', ...props },
    })
    await tick()
    return target
  }

  it('a first-scan summary (name + step + count) has no a11y violations', async () => {
    const target = await mountSummary({ activity: scanActivity({ volumeUsedBytes: 10_000_000 }) })
    expect(target.querySelector('.drive-heading')?.textContent).toBe('Backups')
    expect(target.querySelector('.summary-step')?.textContent).toBe('Find files')
    expect(target.querySelector('.summary-metric')?.textContent).toContain('42,000')
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('a calibrated summary (name + step + percent) has no a11y violations', async () => {
    const target = await mountSummary({ activity: scanActivity({ priorTotalEntries: 100_000 }) })
    expect(target.querySelector('.summary-metric')?.textContent).toContain('42%')
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('a compute-step summary has no a11y violations', async () => {
    const target = await mountSummary({
      activity: scanActivity(),
      aggregation: { phase: 'computing', current: 500, total: 1000, startedAt: Date.now() - 3000 },
    })
    expect(target.querySelector('.summary-step')?.textContent).toBe('Compute folder sizes')
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier 3 a11y tests for `IndexingEnrichRow.svelte`.
 *
 * The "Image indexing" block in the multi-drive indexing tooltip. A pure
 * props-driven presentational component (no store / Tauri deps), so each state is a
 * `mount` with the right props: actively enriching with the images + bytes double bar,
 * paused (both reasons), and with the drive heading. `tString` resolves the real `en`
 * catalog. Mirrors the `IndexingDriveRow` block above.
 */
describe('IndexingEnrichRow a11y', () => {
  function enrichActivity(overrides: Partial<VolumeEnrichActivity> = {}): VolumeEnrichActivity {
    return {
      volumeId: 'root',
      done: 1_200,
      total: 5_000,
      bytesDone: 2_000_000,
      bytesTotal: 9_000_000,
      paused: null,
      startedAt: Date.now() - 4000,
      ...overrides,
    }
  }

  const baseProps = {
    activity: enrichActivity(),
    driveName: 'Macintosh HD',
    showHeading: true,
  }

  async function mountRow(props: Record<string, unknown>): Promise<HTMLDivElement> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingEnrichRow, { target, props: { ...baseProps, ...props } })
    await tick()
    return target
  }

  it('actively enriching with the images + bytes double bar has no violations', async () => {
    const target = await mountRow({ activity: enrichActivity() })
    // Two labeled progress bars (images + bytes).
    expect(target.querySelectorAll('[role="progressbar"]').length).toBe(2)
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('paused waiting for idle has no violations (no bars, just the status)', async () => {
    const target = await mountRow({ activity: enrichActivity({ paused: 'waitingForIdle' }) })
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('paused disconnected has no violations', async () => {
    const target = await mountRow({ activity: enrichActivity({ paused: 'disconnected' }) })
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('without a total (indeterminate) renders no bar and has no violations', async () => {
    const target = await mountRow({ activity: enrichActivity({ total: 0, bytesTotal: 0 }) })
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('with the heading hidden has no violations', async () => {
    const target = await mountRow({ activity: enrichActivity(), showHeading: false })
    expect(target.querySelector('.enrich-heading')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier 3 a11y tests for `IndexingStatusBody.svelte`.
 *
 * The shared, presentational per-volume step checklist rendered by BOTH surfaces
 * (the corner indicator's drive rows and the breadcrumb badge's scanning
 * tooltip). It's pure props-driven (no store / Tauri deps), so each scenario is a
 * `mount` with the right props. The checklist is a `<ul>`/`<li>` list: each step
 * carries its label plus a visually-hidden status word ("Done" / "In progress" /
 * "Not started"), the marker icons/spinner are decorative (`aria-hidden`), and
 * the active step's progress bar carries the step label as its `aria-label`.
 * `tString` resolves the real `en` catalog.
 */
describe('IndexingStatusBody a11y', () => {
  const NOW = Date.now()

  function scanActivity(overrides: Partial<VolumeIndexActivity> = {}): VolumeIndexActivity {
    return {
      volumeId: 'root',
      phase: 'scanning',
      entriesScanned: 42000,
      dirsFound: 1200,
      bytesScanned: 1_000_000,
      scanStartedAt: NOW - 4000,
      priorTotalEntries: null,
      priorScanDurationMs: 120000,
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
      replayEventsProcessed: 3000,
      replayEstimatedTotal: 10000,
      ...overrides,
    }
  }

  const baseProps = {
    activity: scanActivity(),
    aggregation: undefined,
    now: NOW,
    windowedEta: null,
    phase: undefined as ActivityPhase | undefined,
    isNetwork: false,
    coveredInPhases: false,
  }

  async function mountBody(props: Record<string, unknown>): Promise<HTMLDivElement> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(IndexingStatusBody, { target, props: { ...baseProps, ...props } })
    await tick()
    return target
  }

  it('the checklist exposes each step as a list item with a status word', async () => {
    const target = await mountBody({ activity: scanActivity({ priorTotalEntries: 100000 }), windowedEta: '1m left' })
    expect(target.querySelectorAll('ul > li.step').length).toBe(4)
    // The visually-hidden status conveys waiting/in-progress/done to screen readers.
    const srStatuses = [...target.querySelectorAll('.step .sr-only')].map((el) => el.textContent)
    expect(srStatuses).toContain('In progress')
    expect(srStatuses).toContain('Not started')
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('scanning, counter-only (no calibrated progress) has no a11y violations', async () => {
    const target = await mountBody({ activity: scanActivity({ priorTotalEntries: null, volumeUsedBytes: null }) })
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('scanning, first scan (tier 2) shows count + elapsed and no progress bar', async () => {
    const target = await mountBody({
      activity: scanActivity({ priorTotalEntries: null, volumeUsedBytes: 10_000_000 }),
    })
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    expect(target.querySelector('.tooltip-detail')?.textContent).toContain('42,000')
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('scanning with a calibrated progress bar has no a11y violations', async () => {
    const target = await mountBody({ activity: scanActivity({ priorTotalEntries: 100000 }), windowedEta: '1m left' })
    expect(target.querySelector('.tooltip-progress')).not.toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('the compute step active (aggregation) has no a11y violations', async () => {
    const target = await mountBody({
      activity: scanActivity({ priorTotalEntries: 100000 }),
      aggregation: { phase: 'computing', current: 500, total: 1000, startedAt: NOW - 3000 },
    })
    expect(target.querySelector('.tooltip-progress')).not.toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('the catch-up (reconcile) step active has no a11y violations', async () => {
    const target = await mountBody({ activity: scanActivity(), phase: 'reconciling' })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('replaying has no a11y violations', async () => {
    const target = await mountBody({ activity: replayActivity(), windowedEta: '30s left' })
    await expectNoA11yViolations(target)
    target.remove()
  })
})
