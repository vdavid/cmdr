/**
 * Mount tests for `IndexingStatusBody.svelte`, the shared presentational status
 * body rendered by both surfaces (the corner indicator's drive rows and the
 * breadcrumb badge's scanning tooltip). It's pure props-driven (activity +
 * aggregation + an injected `now` tick + the wrapper's `windowedEta`), so each
 * mode is just a `mount` with the right fixture. `tString` resolves the real `en`
 * catalog. The M2 count-first policy is the contract under test: tier-1 shows the
 * bar, tier-2 (first scan) shows count + elapsed and NO bar.
 */
import { describe, it, expect } from 'vitest'
import { mount, flushSync } from 'svelte'
import IndexingStatusBody from './IndexingStatusBody.svelte'
import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'

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
  return { ...scanActivity(), phase: 'replaying', replayEventsProcessed: 3_000, replayEstimatedTotal: 10_000, ...overrides }
}

function render(props: {
  activity: VolumeIndexActivity
  aggregation?: AggregationActivity | undefined
  now?: number
  windowedEta?: string | null
}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(IndexingStatusBody, {
    target,
    props: { aggregation: undefined, now: Date.now(), windowedEta: null, ...props },
  })
  flushSync()
  return target
}

describe('IndexingStatusBody', () => {
  it('scan tier-1 (calibrated) shows the bar + percent', () => {
    const target = render({ activity: scanActivity({ priorTotalEntries: 100_000 }), windowedEta: '1m left' })
    expect(target.textContent).toContain('Scanning your drive')
    expect(target.querySelector('[role="progressbar"]')).not.toBeNull()
    // 42,000 / 100,000 = 42%, joined with the injected ETA.
    expect(target.querySelector('.tooltip-percent')?.textContent).toContain('42%')
    expect(target.querySelector('.tooltip-percent')?.textContent).toContain('1m left')
  })

  it('scan tier-2 (first scan) shows count + elapsed and NO progress bar', () => {
    const target = render({ activity: scanActivity({ priorTotalEntries: null, volumeUsedBytes: 10_000_000 }) })
    expect(target.textContent).toContain('Scanning your drive (first scan)')
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    const detail = target.querySelector('.tooltip-detail')?.textContent ?? ''
    expect(detail).toContain('42,000')
    expect(detail).toMatch(/·\s*\d+:\d{2}/) // elapsed clock
  })

  it('counter-only scan (no denominator) shows no bar', () => {
    const target = render({ activity: scanActivity({ priorTotalEntries: null, volumeUsedBytes: null }) })
    expect(target.querySelector('.tooltip-progress')).toBeNull()
    expect(target.querySelector('.tooltip-detail')?.textContent).toContain('42,000')
  })

  it('aggregation renders the phase label + bar', () => {
    const target = render({
      activity: scanActivity(),
      aggregation: { phase: 'computing', current: 500, total: 1_000, startedAt: Date.now() - 3000 },
    })
    expect(target.textContent).toContain('Computing directory sizes')
    expect(target.querySelector('[role="progressbar"]')).not.toBeNull()
  })

  it('replay renders the update label, event count, and bar', () => {
    const target = render({ activity: replayActivity(), windowedEta: '5s left' })
    expect(target.textContent).toContain('Updating index')
    expect(target.querySelector('.tooltip-detail')?.textContent).toContain('3,000')
    expect(target.querySelector('[role="progressbar"]')).not.toBeNull()
  })
})
