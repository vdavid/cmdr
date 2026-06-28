/**
 * Tier 3 a11y tests for `IndexingDriveSummary.svelte`, the collapsed one-line
 * summary for a SECONDARY drive in the corner indicator when several drives index
 * at once (the primary expands to its full checklist). Pure props-driven (it reads
 * the real `index-state` for the volume's phase, which returns `undefined` here),
 * so each scenario is a `mount`. `tString` resolves the real `en` catalog.
 */
import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import IndexingDriveSummary from './IndexingDriveSummary.svelte'
import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

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

describe('IndexingDriveSummary a11y', () => {
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
