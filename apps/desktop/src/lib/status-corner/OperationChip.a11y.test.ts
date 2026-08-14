/**
 * Tier 3 a11y tests for `OperationChip.svelte`.
 *
 * The chip is the corner's one interactive member, so what it has to prove is
 * that it's a properly named button in every state it can be in, and that the
 * bar inside it doesn't turn into a second, unnamed announcement.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { CHIP_SETTLE_MS } from './operation-chip'

vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: () => Promise.resolve(),
}))

let store: ReturnType<typeof createOperationsStore> | null = null
vi.mock('$lib/file-operations/queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => store?.operations ?? [],
  getMainWindowOperations: () => store,
}))

vi.mock('$lib/file-operations/foreground-operation.svelte', () => ({
  getForegroundOperationId: () => null,
  getForegroundFailureId: () => null,
}))

import { createOperationsStore } from '$lib/file-operations/queue/operations-store.svelte'
import OperationChip from './OperationChip.svelte'

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

function renderChip(status: OperationSnapshot['status'], progress: WriteProgressEvent = runningProgress): HTMLElement {
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

describe('OperationChip a11y', () => {
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
