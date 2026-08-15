/**
 * Tier 3 a11y tests for `StatusCorner.svelte`.
 *
 * The corner is a layout row, so what it has to prove is that hosting the
 * hourglass (and, later, the operation chip) inside it introduces no violation
 * of its own: no stray landmark, no unlabelled interactive element. The indexing
 * state and volume store are stubbed exactly as in
 * `$lib/indexing/IndexingStatusIndicator.a11y.test.ts`.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import StatusCorner from './StatusCorner.svelte'
import type { VolumeIndexActivity } from '$lib/indexing/index-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let activeVolumes: VolumeIndexActivity[] = []

function scanActivity(volumeId: string): VolumeIndexActivity {
  return {
    volumeId,
    phase: 'scanning',
    entriesScanned: 42000,
    dirsFound: 1200,
    bytesScanned: 1_000_000,
    scanStartedAt: Date.now() - 4000,
    priorTotalEntries: 100000,
    priorScanDurationMs: 120000,
    volumeUsedBytes: null,
    replayEventsProcessed: 0,
    replayEstimatedTotal: 0,
    replayStartedAt: 0,
  }
}

vi.mock('$lib/indexing/index-state.svelte', () => ({
  ROOT_VOLUME_ID: 'root',
  getActiveIndexVolumes: () => activeVolumes,
  isAnyVolumeIndexing: () => activeVolumes.length > 0,
  getVolumeAggregation: () => undefined,
  getAggregatingVolumeIds: () => [],
  getActivePhaseVolumeIds: () => [],
  getVolumePhase: () => undefined,
  getVolumeScanRunKind: () => undefined,
  isVolumeCoveredInPhases: () => false,
  placeholderActivity: (volumeId: string) => scanActivity(volumeId),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD' }],
}))

vi.mock('$lib/settings', () => ({
  getSetting: () => false,
  onSpecificSettingChange: () => () => {},
}))

vi.mock('$lib/media-index/enabled-volumes', () => ({
  getEnabledMediaIndexVolumeIds: () => [],
}))

describe('StatusCorner a11y', () => {
  it('idle (nothing to report) has no violations', async () => {
    activeVolumes = []
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(StatusCorner, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with the hourglass showing has no violations', async () => {
    activeVolumes = [scanActivity('root')]
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(StatusCorner, { target, props: {} })
    await tick()
    expect(target.querySelector('.indexing-status')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('with a child beside the hourglass has no violations', async () => {
    activeVolumes = [scanActivity('root')]
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(StatusCorner, {
      target,
      props: {
        children: createRawSnippet(() => ({
          render: () => '<button class="fake-chip" type="button">Copying</button>',
        })),
      },
    })
    await tick()
    expect(target.querySelector('.fake-chip')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
