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

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import IndexingStatusBody from './IndexingStatusBody.svelte'
import type { VolumeIndexActivity } from './index-state.svelte'
import type { ActivityPhase } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

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
}

async function mountBody(props: Record<string, unknown>): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(IndexingStatusBody, { target, props: { ...baseProps, ...props } })
  await tick()
  return target
}

describe('IndexingStatusBody a11y', () => {
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
