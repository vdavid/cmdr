/**
 * Tier 3 a11y tests for `IndexingDriveRow.svelte`.
 *
 * One row in the multi-drive indexing tooltip. It's a pure props-driven
 * presentational component (no store / Tauri deps), so each state is just a
 * `mount` with the right props: scanning (with and without a calibrated
 * progress bar), replaying, the aggregation phase folded into the row, and the
 * multi-drive heading. `tString` resolves the real `en` catalog.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import IndexingDriveRow from './IndexingDriveRow.svelte'
import type { VolumeIndexActivity } from './index-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

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
  driveName: 'Macintosh HD',
  showHeading: false,
  aggregating: false,
  aggPhase: '',
  aggCurrent: 0,
  aggTotal: 0,
  aggStartedAt: 0,
}

async function mountRow(props: Record<string, unknown>): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(IndexingDriveRow, { target, props: { ...baseProps, ...props } })
  await tick()
  return target
}

describe('IndexingDriveRow a11y', () => {
  it('scanning, counter-only (no calibrated progress) has no a11y violations', async () => {
    const target = await mountRow({ activity: scanActivity({ priorTotalEntries: null, volumeUsedBytes: null }) })
    expect(target.querySelector('.tooltip-progress')).toBeNull()
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
      aggregating: true,
      aggPhase: 'computing',
      aggCurrent: 500,
      aggTotal: 1000,
      aggStartedAt: Date.now() - 3000,
    })
    await expectNoA11yViolations(target)
    target.remove()
  })
})
