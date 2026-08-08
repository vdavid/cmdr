/**
 * The corner chip, driven through a real operations store.
 *
 * The store is the one the main window holds (`main-window-operations`), so the
 * seam is mocked to hand back a store this test feeds via `_testApplySnapshot` /
 * `_testApplyProgress` — the same reducers the live `operations-changed` and
 * `write-progress` streams drive.
 *
 * Every mount goes through `renderChip`, which advances past the chip's settle
 * delay: work that lasts less than a moment deliberately never reaches the
 * corner, and one test below covers exactly that.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'
import { CHIP_SETTLE_MS } from './operation-chip'

// The store subscribes to Tauri events; `init()` is never called here, but the
// module-level import still has to resolve.
vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
}))

const openQueueWindow = vi.fn(() => Promise.resolve())
vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: (): Promise<void> => openQueueWindow(),
}))

let store: ReturnType<typeof createOperationsStore> | null = null
vi.mock('$lib/file-operations/queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => store?.operations ?? [],
  getMainWindowOperations: () => store,
}))

let foregroundOperationId: string | null = null
vi.mock('$lib/file-operations/foreground-operation.svelte', () => ({
  getForegroundOperationId: () => foregroundOperationId,
}))

import { createOperationsStore } from '$lib/file-operations/queue/operations-store.svelte'
import OperationChip from './OperationChip.svelte'

function snapshot(over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'running',
    source: '/Users/me/Documents',
    destination: '/Volumes/Naspolya/Backup',
    supportsRollback: true,
    error: null,
    ...over,
  }
}

function progress(over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'report.pdf',
    filesDone: 60,
    filesTotal: 214,
    bytesDone: 420,
    bytesTotal: 1000,
    etaSeconds: 80,
    ...over,
  }
}

let target: HTMLElement

/** Mounts the chip and lets its settle delay elapse, so the assertions are
 *  about the steady state rather than the appearance delay. */
function renderChip(): HTMLElement {
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(OperationChip, { target })
  flushSync()
  vi.advanceTimersByTime(CHIP_SETTLE_MS)
  flushSync()
  return target
}

function chip(): HTMLButtonElement | null {
  return target.querySelector('.operation-chip')
}

beforeEach(() => {
  vi.useFakeTimers()
  document.body.innerHTML = ''
  openQueueWindow.mockClear()
  foregroundOperationId = null
  store = createOperationsStore()
})

afterEach(() => {
  store?.dispose()
  store = null
  vi.useRealTimers()
})

describe('OperationChip', () => {
  it('shows nothing while the queue is empty', () => {
    renderChip()
    expect(chip()).toBeNull()
  })

  it('names the running operation and carries its percentage in the label', () => {
    store?._testApplySnapshot([snapshot()])
    store?._testApplyProgress(progress())
    renderChip()
    expect(chip()?.querySelector('.chip-label')?.textContent).toBe('Copying')
    expect(chip()?.getAttribute('aria-label')).toBe('Copying, 42 percent. Open the operation queue.')
    expect(target.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('42')
  })

  it('counts files when the operation moves no bytes', () => {
    // A same-volume move renames server-side: a bytes bar would read 0% start
    // to finish.
    store?._testApplySnapshot([snapshot({ operationType: 'move' })])
    store?._testApplyProgress(progress({ bytesDone: 0, bytesTotal: 0, filesDone: 3, filesTotal: 10 }))
    renderChip()
    expect(target.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('30')
    expect(chip()?.getAttribute('aria-label')).toContain('30 percent')
  })

  it('reads 0 percent with nothing to count, and never NaN', () => {
    store?._testApplySnapshot([snapshot()])
    store?._testApplyProgress(progress({ bytesDone: 0, bytesTotal: 0, filesDone: 0, filesTotal: 0 }))
    renderChip()
    expect(chip()?.getAttribute('aria-label')).toBe('Copying, 0 percent. Open the operation queue.')
  })

  it('shows the first of several running operations, and nothing about the rest', () => {
    store?._testApplySnapshot([
      snapshot({ operationId: 'a', operationType: 'copy' }),
      snapshot({ operationId: 'b', operationType: 'trash' }),
    ])
    renderChip()
    expect(target.querySelectorAll('.operation-chip')).toHaveLength(1)
    expect(chip()?.textContent).toContain('Copying')
    expect(chip()?.textContent).not.toContain('trash')
    expect(target.textContent).not.toContain('+1')
  })

  it.each(['rename', 'create_folder', 'create_file'] as const)('stays out of the corner for a %s', (operationType) => {
    store?._testApplySnapshot([snapshot({ operationType })])
    renderChip()
    expect(chip()).toBeNull()
  })

  it('stays quiet while the foreground dialog owns the operation, and appears when it lets go', () => {
    foregroundOperationId = 'op-1'
    store?._testApplySnapshot([snapshot()])
    renderChip()
    expect(chip()).toBeNull()

    // The user presses Queue: the modal hands the operation over.
    foregroundOperationId = null
    store?._testApplySnapshot([snapshot()])
    flushSync()
    vi.advanceTimersByTime(CHIP_SETTLE_MS)
    flushSync()
    expect(chip()).not.toBeNull()
  })

  it('keeps a paused-only queue visible, with a still bar and the paused word', () => {
    store?._testApplySnapshot([snapshot({ status: 'paused' })])
    store?._testApplyProgress(progress())
    renderChip()
    expect(chip()?.querySelector('.chip-label')?.textContent).toBe('Paused')
    expect(target.querySelector('.fill')?.classList.contains('animated')).toBe(false)
    expect(target.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('42')
  })

  it('keeps the shimmer on a running bar', () => {
    store?._testApplySnapshot([snapshot()])
    store?._testApplyProgress(progress())
    renderChip()
    expect(target.querySelector('.fill')?.classList.contains('animated')).toBe(true)
  })

  it('waits out the settle delay, so a blink of work never flashes the corner', () => {
    store?._testApplySnapshot([snapshot()])
    target = document.createElement('div')
    document.body.appendChild(target)
    mount(OperationChip, { target })
    flushSync()
    expect(chip()).toBeNull()

    vi.advanceTimersByTime(CHIP_SETTLE_MS - 1)
    flushSync()
    expect(chip()).toBeNull()

    vi.advanceTimersByTime(1)
    flushSync()
    expect(chip()).not.toBeNull()
  })

  it('opens the operation queue on click', () => {
    store?._testApplySnapshot([snapshot()])
    renderChip()
    chip()?.click()
    flushSync()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
  })

  it('spells out the whole operation in its tooltip, ETA included', () => {
    store?._testApplySnapshot([snapshot()])
    store?._testApplyProgress(progress())
    renderChip()
    expect(target.querySelector('.tooltip-content')?.textContent).toBe(
      'Copying 214 items to Backup · 42% · 1m 20s left',
    )
  })

  it('drops the destination from the tooltip when nothing is being moved anywhere', () => {
    store?._testApplySnapshot([snapshot({ operationType: 'delete', destination: null })])
    store?._testApplyProgress(progress({ operationType: 'delete', etaSeconds: null }))
    renderChip()
    expect(target.querySelector('.tooltip-content')?.textContent).toBe('Deleting 214 items · 42%')
  })

  it('says paused in the tooltip instead of a countdown it no longer believes', () => {
    store?._testApplySnapshot([snapshot({ status: 'paused' })])
    store?._testApplyProgress(progress())
    renderChip()
    expect(target.querySelector('.tooltip-content')?.textContent).toBe('Copying 214 items to Backup · 42% · Paused')
  })

  it('leaves the count out of the tooltip before the first progress tick', () => {
    store?._testApplySnapshot([snapshot()])
    renderChip()
    expect(target.querySelector('.tooltip-content')?.textContent).toBe('Copying to Backup · 0%')
  })
})
