/**
 * The status corner's structural contract: it is always mounted, it renders its
 * children BEFORE the hourglass (left of it, in a right-aligned row), and it —
 * not the hourglass — carries the corner placement.
 *
 * The indexing state and the volume store are stubbed the same way
 * `$lib/indexing/IndexingStatusIndicator.a11y.test.ts` stubs them, so the
 * hourglass can be driven between idle and scanning without a real indexer.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick, flushSync, createRawSnippet } from 'svelte'
import StatusCorner from './StatusCorner.svelte'
import type { VolumeIndexActivity } from '$lib/indexing/index-state.svelte'
import type { OperationRow } from '$lib/file-operations/queue/operations-store.svelte'
import { CHIP_SETTLE_MS } from './operation-chip'

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

// The corner mounts the operation chip itself, so the chip's inputs are stubbed
// here too: a plain row list the test sets, and no foreground modal.
let operationRows: OperationRow[] = []

vi.mock('$lib/file-operations/queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => operationRows,
  getMainWindowOperations: () => null,
}))

vi.mock('$lib/file-operations/foreground-operation.svelte', () => ({
  getForegroundOperationId: () => null,
}))

vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: () => Promise.resolve(),
}))

function runningCopy(): OperationRow {
  return {
    snapshot: {
      operationId: 'op-1',
      operationType: 'copy',
      status: 'running',
      source: '/Users/me/Documents',
      destination: '/Volumes/Naspolya/Backup',
      supportsRollback: true,
      error: null,
    },
    progress: null,
    etaSecondsDisplay: null,
  }
}

function mountCorner(children?: ReturnType<typeof createRawSnippet>): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(StatusCorner, { target, props: children ? { children } : {} })
  return target
}

/** A minimal child to occupy the chip's future slot. */
const chipSnippet = createRawSnippet(() => ({
  render: () => '<button class="fake-chip" type="button">chip</button>',
}))

describe('StatusCorner', () => {
  it('mounts the row even when nothing has status to report', async () => {
    activeVolumes = []
    const target = mountCorner()
    await tick()
    expect(target.querySelector('.status-corner')).not.toBeNull()
    expect(target.querySelector('.indexing-status')).toBeNull()
  })

  it('renders children before the hourglass, so the hourglass stays rightmost', async () => {
    activeVolumes = [scanActivity('root')]
    const target = mountCorner(chipSnippet)
    await tick()
    const corner = target.querySelector('.status-corner')
    if (!corner) throw new Error('the status corner never mounted')
    const chip = corner.querySelector('.fake-chip')
    const hourglass = corner.querySelector('.indexing-status')
    if (!chip || !hourglass) throw new Error('expected both the child and the hourglass in the corner')
    // `DOCUMENT_POSITION_FOLLOWING` = the hourglass comes after the chip.
    expect(chip.compareDocumentPosition(hourglass) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })

  it('puts the operation chip left of the hourglass', () => {
    vi.useFakeTimers()
    try {
      activeVolumes = [scanActivity('root')]
      operationRows = [runningCopy()]
      const target = mountCorner()
      flushSync()
      vi.advanceTimersByTime(CHIP_SETTLE_MS)
      flushSync()
      const chip = target.querySelector('.operation-chip')
      const hourglass = target.querySelector('.indexing-status')
      if (!chip || !hourglass) throw new Error('expected both the chip and the hourglass in the corner')
      expect(chip.compareDocumentPosition(hourglass) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    } finally {
      operationRows = []
      vi.useRealTimers()
    }
  })

  it('hosts the hourglass itself, so the main page mounts one corner and not two indicators', async () => {
    activeVolumes = [scanActivity('root')]
    const target = mountCorner()
    await tick()
    expect(target.querySelectorAll('.status-corner .indexing-status')).toHaveLength(1)
  })
})
