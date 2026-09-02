/**
 * Tier 3 a11y tests for the status corner: the layout row itself, the operation
 * chip, and the two failure toasts.
 *
 * One file per component would cost about four times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * The chip's fake timers stay INSIDE its own block: file-wide they'd stall the
 * other blocks' async renders. `getMainWindowOperationRows` reads the same
 * module-level `store` the chip's block creates and disposes, so it answers `[]`
 * for the failures-toast block exactly as that block's own stub did.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, flushSync, createRawSnippet } from 'svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'
import type { VolumeIndexActivity } from '$lib/indexing/index-state.svelte'
import { createOperationsStore } from '$lib/file-operations/queue/operations-store.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { CHIP_SETTLE_MS } from './operation-chip'

let store: ReturnType<typeof createOperationsStore> | null = null
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

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: () => Promise.resolve(),
}))

vi.mock('$lib/file-operations/queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => store?.operations ?? [],
  getMainWindowOperations: () => store,
}))

vi.mock('$lib/file-operations/foreground-operation.svelte', () => ({
  getForegroundOperationId: () => null,
  getForegroundFailureId: () => null,
}))

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
  getVolumeCoveragePhase: () => undefined,
  placeholderActivity: (volumeId: string) => scanActivity(volumeId),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD' }],
}))

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: () => false,
  onSpecificSettingChange: () => () => {},
}))

vi.mock('$lib/media-index/enabled-volumes', () => ({
  getEnabledMediaIndexVolumeIds: () => [],
}))

import OperationChip from './OperationChip.svelte'
import OperationFailedToastContent from './OperationFailedToastContent.svelte'
import OperationFailureToastBody from './OperationFailureToastBody.svelte'
import OperationFailuresToastContent from './OperationFailuresToastContent.svelte'
import StatusCorner from './StatusCorner.svelte'

// These components share one jsdom document, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `OperationChip.svelte`.
 *
 * The chip is the corner's one interactive member, so what it has to prove is
 * that it's a properly named button in every state it can be in, and that the
 * bar inside it doesn't turn into a second, unnamed announcement.
 */
describe('OperationChip a11y', () => {
  const runningProgress: WriteProgressEvent = {
    operationId: 'op-1',
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'report.pdf',
    filesDone: 60,
    filesTotal: 214,
    bytesDone: 420,
    bytesTotal: 1000,
    etaSeconds: 80,
  }

  function snapshot(status: OperationSnapshot['status']): OperationSnapshot {
    return {
      operationId: 'op-1',
      operationType: 'copy',
      status,
      source: '/Users/me/Documents',
      destination: '/Volumes/Naspolya/Backup',
      supportsRollback: true,
      reverses: null,
      error: null,
    }
  }

  /** Still counting: both totals are 0 for the whole walk, so there's no bar and
   *  no honest percentage, and the chip names itself a different way. */
  const scanningProgress: WriteProgressEvent = {
    ...runningProgress,
    phase: 'scanning',
    filesDone: 900,
    filesTotal: 0,
    bytesDone: 0,
    bytesTotal: 0,
    etaSeconds: null,
  }

  function renderChip(
    status: OperationSnapshot['status'],
    progress: WriteProgressEvent = runningProgress,
  ): HTMLElement {
    store?._testApplySnapshot([snapshot(status)])
    store?._testApplyProgress(progress)
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(OperationChip, { target })
    flushSync()
    vi.advanceTimersByTime(CHIP_SETTLE_MS)
    flushSync()
    // Axe drives its own timers internally, so hand them back before it runs.
    vi.useRealTimers()
    return target
  }

  beforeEach(() => {
    vi.useFakeTimers()
    document.body.innerHTML = ''
    store = createOperationsStore()
  })

  afterEach(() => {
    store?.dispose()
    store = null
    vi.useRealTimers()
  })

  it('running has no violations', async () => {
    const target = renderChip('running')
    expect(target.querySelector('.operation-chip')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('paused has no violations', async () => {
    const target = renderChip('paused')
    expect(target.querySelector('.chip-label')?.textContent).toBe('Paused')
    await expectNoA11yViolations(target)
  })

  it('scanning is named with the verb it shows and the queue it opens', async () => {
    const target = renderChip('running', scanningProgress)
    const chip = target.querySelector('.operation-chip')
    // The two things the sighted chip offers and a percentage-free label could
    // silently drop: the visible word voice control presses it by (WCAG 2.5.3),
    // and the affordance that says what pressing it does.
    expect(chip?.getAttribute('aria-label')).toContain(target.querySelector('.chip-label')?.textContent ?? '')
    expect(chip?.getAttribute('aria-label')).toContain('Open the operation queue')
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `OperationFailedToastContent.svelte`. Its explanation is
 * markup injected by the error pipeline, so it's worth an axe pass of its own.
 */
describe('OperationFailedToastContent a11y', () => {
  const snapshot: OperationSnapshot = {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'failed',
    source: '/Users/me/Documents/report.pdf',
    destination: '/Volumes/Backup',
    supportsRollback: false,
    reverses: null,
    error: { type: 'insufficient_space', required: 1073741824, available: 1024, volumeName: 'Backup' },
  }

  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(OperationFailedToastContent, { target, props: { toastId: 'toast-1', snapshot } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `OperationFailureToastBody.svelte`, the body both failure
 * notices render. Two states: the title alone (what the summary passes) and the
 * title with a reason under it (what one failure passes). The glyph is decorative,
 * so the title's words have to carry the severity on their own.
 */
describe('OperationFailureToastBody a11y', () => {
  function mountBody(children?: ReturnType<typeof createRawSnippet>): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(OperationFailureToastBody, {
      target,
      props: { toastId: 'toast-1', title: 'Copy did not finish', children },
    })
    return target
  }

  it('title only has no a11y violations', async () => {
    const target = mountBody()
    await tick()
    expect(target.querySelector('.glyph')?.getAttribute('aria-hidden')).toBe('true')
    await expectNoA11yViolations(target)
  })

  it('with a reason under the title has no a11y violations', async () => {
    const target = mountBody(createRawSnippet(() => ({ render: () => '<p>There is not enough room on Backup.</p>' })))
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `OperationFailuresToastContent.svelte`, the summary a
 * burst of failures collapses into.
 */
describe('OperationFailuresToastContent a11y', () => {
  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(OperationFailuresToastContent, { target, props: { toastId: 'toast-1' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `StatusCorner.svelte`.
 *
 * The corner is a layout row, so what it has to prove is that hosting the
 * hourglass (and, later, the operation chip) inside it introduces no violation
 * of its own: no stray landmark, no unlabelled interactive element. The indexing
 * state and volume store are stubbed exactly as in
 * `$lib/indexing/stateful.a11y.test.ts`.
 */
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
