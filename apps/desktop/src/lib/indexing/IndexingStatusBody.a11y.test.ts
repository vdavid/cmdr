/**
 * Tier 3 a11y tests for `IndexingStatusBody.svelte`.
 *
 * The shared, presentational per-volume status body rendered by BOTH surfaces
 * (the corner indicator's drive rows and the breadcrumb badge's scanning
 * tooltip). It's pure props-driven (no store / Tauri deps), so each mode is a
 * `mount` with the right props: scan (counter-only, rough first scan, and
 * calibrated-with-bar), replay, and an aggregation phase. `tString` resolves the
 * real `en` catalog. The wrapper normally injects `now` + `windowedEta`; here we
 * pass them directly.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import IndexingStatusBody from './IndexingStatusBody.svelte'
import type { VolumeIndexActivity } from './index-state.svelte'
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
    replayStartedAt: NOW - 4000,
    ...overrides,
  }
}

const baseProps = {
  // A scan activity by default; every test overrides `activity` explicitly. It's
  // here so the merged props type carries the required fields.
  activity: scanActivity(),
  aggregation: undefined,
  now: NOW,
  windowedEta: null,
}

async function mountBody(props: Record<string, unknown>): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(IndexingStatusBody, { target, props: { ...baseProps, ...props } })
  await tick()
  return target
}

describe('IndexingStatusBody a11y', () => {
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
    expect(target.querySelector('.tooltip-detail')?.textContent).toMatch(/·\s*\d+:\d{2}/)
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('scanning with a calibrated progress bar has no a11y violations', async () => {
    const target = await mountBody({ activity: scanActivity({ priorTotalEntries: 100000 }), windowedEta: '1m left' })
    expect(target.querySelector('.tooltip-progress')).not.toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('replaying has no a11y violations', async () => {
    const target = await mountBody({ activity: replayActivity(), windowedEta: '30s left' })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('with an aggregation phase folded in has no a11y violations', async () => {
    const target = await mountBody({
      activity: scanActivity({ priorTotalEntries: 100000 }),
      aggregation: { phase: 'computing', current: 500, total: 1000, startedAt: NOW - 3000 },
    })
    expect(target.querySelector('.tooltip-progress')).not.toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
