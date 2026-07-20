/**
 * Tier 3 a11y tests for `DriveIndexBadge.svelte`: the focusable, labeled status
 * dot and its open menu must have no axe violations, in each freshness state.
 * Mirrors `IndexingStatusIndicator.a11y.test.ts`.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { Freshness, VolumeIndexStatus } from '$lib/ipc/bindings'
import type { VolumeIndexActivity } from '$lib/indexing'

// The badge reads its own volume's live activity + phase from `index-state`; mock
// it so we can exercise the scanning tooltip's rich DOM checklist body.
let badgeActivity: VolumeIndexActivity | undefined
vi.mock('$lib/indexing', () => ({
  getVolumeActivity: () => badgeActivity,
  getVolumeAggregation: () => undefined,
  getVolumePhase: () => undefined,
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

import DriveIndexBadge from './DriveIndexBadge.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function makeStatus(freshness: Freshness | null, enabled = freshness != null): VolumeIndexStatus {
  return {
    volumeId: 'smb-test',
    enabled,
    freshness,
    failure: null,
    scanCompletedAt: freshness === 'fresh' ? 1_750_000_000 : null,
    scanDurationMs: freshness === 'fresh' ? 134_000 : null,
    coalescedSignalsSinceSweep: 0,
    nextSweepDueAt: null,
  }
}

async function mountBadge(status: VolumeIndexStatus) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DriveIndexBadge, {
    target,
    props: { volumeId: status.volumeId, status, driveName: 'Backups', onAction: () => {} },
  })
  await tick()
  return target
}

beforeEach(() => {
  badgeActivity = undefined
})

describe('DriveIndexBadge a11y', () => {
  it('the gray (disabled) dot has no violations', async () => {
    const target = await mountBadge(makeStatus(null, false))
    expect(target.querySelector('.drive-index-badge')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the blue (scanning) dot has no violations', async () => {
    const target = await mountBadge(makeStatus('scanning'))
    await expectNoA11yViolations(target)
  })

  it('the scanning dot with the rich DOM status body has no violations', async () => {
    badgeActivity = {
      volumeId: 'smb-test',
      phase: 'scanning',
      entriesScanned: 42_000,
      dirsFound: 1_200,
      bytesScanned: 1_000_000,
      scanStartedAt: Date.now() - 4000,
      priorTotalEntries: 100_000, // calibrated → renders the progress bar too
      priorScanDurationMs: 120_000,
      volumeUsedBytes: null,
      replayEventsProcessed: 0,
      replayEstimatedTotal: 0,
      replayStartedAt: 0,
    }
    const target = await mountBadge(makeStatus('scanning'))
    expect(target.querySelector('.scan-tooltip-body')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the green (fresh) dot has no violations', async () => {
    const target = await mountBadge(makeStatus('fresh'))
    await expectNoA11yViolations(target)
  })

  it('the yellow (stale) dot has no violations', async () => {
    const target = await mountBadge(makeStatus('stale'))
    await expectNoA11yViolations(target)
  })

  it('the open menu has no violations', async () => {
    const target = await mountBadge(makeStatus('stale'))
    const badge = target.querySelector<HTMLButtonElement>('.drive-index-badge')
    expect(badge).not.toBeNull()
    badge?.click()
    flushSync()
    expect(target.querySelector('.drive-index-menu')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
