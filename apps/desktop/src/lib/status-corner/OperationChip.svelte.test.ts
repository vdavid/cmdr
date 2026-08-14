/**
 * The corner chip, driven through a real operations store and a real session.
 *
 * The store is the one the main window holds (`main-window-operations`), so the
 * seam is mocked to hand back a store this test feeds via `_testApplySnapshot` /
 * `_testApplyProgress` — the same reducers the live `operations-changed` and
 * `write-progress` streams drive.
 *
 * One broadcast `write-progress` reaches two places in a real window: the store,
 * which holds the latest tick, and the operation's session, which holds the
 * smoothed ETA the chip renders. `emitProgress` feeds both, which is why a
 * tooltip assertion about the countdown works at all.
 *
 * Every mount goes through `renderChip`, which advances past the chip's settle
 * delay: work that lasts less than a moment deliberately never reaches the
 * corner, and one test below covers exactly that.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'
import { CHIP_SETTLE_MS } from './operation-chip'

// The store and the session fan-out both subscribe to Tauri events. The store's
// `init()` is never called here; the fan-out's is, so every stream it listens to
// has to be mockable.
vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
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
let foregroundFailureId: string | null = null
vi.mock('$lib/file-operations/foreground-operation.svelte', () => ({
  getForegroundOperationId: () => foregroundOperationId,
  getForegroundFailureId: () => foregroundFailureId,
}))

import { createOperationsStore } from '$lib/file-operations/queue/operations-store.svelte'
import {
  destroyOperationSessions,
  getOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
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

/** A retained failure: settled, no progress, carrying its typed reason. */
function failedSnapshot(operationId = 'op-1'): OperationSnapshot {
  return snapshot({
    operationId,
    status: 'failed',
    supportsRollback: false,
    error: { type: 'source_not_found', path: '/Users/me/Documents/report.pdf' },
  })
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

/** One `write-progress` tick, delivered the way the backend delivers it: to
 *  every listener in the window at once. The store keeps the latest tick (the
 *  bar and the count), the session turns it into the smoothed ETA. */
function emitProgress(event: WriteProgressEvent): void {
  store?._testApplyProgress(event)
  getOperationSessions()?._testEmit({ kind: 'progress', event })
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

beforeEach(async () => {
  vi.useFakeTimers()
  document.body.innerHTML = ''
  openQueueWindow.mockClear()
  foregroundOperationId = null
  foregroundFailureId = null
  store = createOperationsStore()
  await initOperationSessions()
})

afterEach(() => {
  store?.dispose()
  store = null
  // The registry is a window singleton, so a session left holding `op-1` would
  // hand the next test a warmed-up smoother.
  destroyOperationSessions()
  vi.useRealTimers()
})

describe('OperationChip', () => {
  it('shows nothing while the queue is empty', () => {
    renderChip()
    expect(chip()).toBeNull()
  })

  it('names the running operation and carries its percentage in the label', () => {
    store?._testApplySnapshot([snapshot()])
    emitProgress(progress())
    renderChip()
    expect(chip()?.querySelector('.chip-label')?.textContent).toBe('Copying')
    expect(chip()?.getAttribute('aria-label')).toBe('Copying, 42 percent. Open the operation queue.')
    expect(target.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('42')
  })

  it('counts files when the operation moves no bytes', () => {
    // A same-volume move renames server-side: a bytes bar would read 0% start
    // to finish.
    store?._testApplySnapshot([snapshot({ operationType: 'move' })])
    emitProgress(progress({ bytesDone: 0, bytesTotal: 0, filesDone: 3, filesTotal: 10 }))
    renderChip()
    expect(target.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('30')
    expect(chip()?.getAttribute('aria-label')).toContain('30 percent')
  })

  it('reads 0 percent with nothing to count, and never NaN', () => {
    store?._testApplySnapshot([snapshot()])
    emitProgress(progress({ bytesDone: 0, bytesTotal: 0, filesDone: 0, filesTotal: 0 }))
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
    emitProgress(progress())
    renderChip()
    expect(chip()?.querySelector('.chip-label')?.textContent).toBe('Paused')
    expect(target.querySelector('.fill')?.classList.contains('animated')).toBe(false)
    expect(target.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('42')
  })

  it('keeps the shimmer on a running bar', () => {
    store?._testApplySnapshot([snapshot()])
    emitProgress(progress())
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
    emitProgress(progress())
    renderChip()
    expect(target.querySelector('.tooltip-content')?.textContent).toBe(
      'Copying · 214 items · to Backup · 42% · 1m 20s left',
    )
  })

  it('drops the destination from the tooltip when nothing is being moved anywhere', () => {
    store?._testApplySnapshot([snapshot({ operationType: 'delete', destination: null })])
    emitProgress(progress({ operationType: 'delete', etaSeconds: null }))
    renderChip()
    expect(target.querySelector('.tooltip-content')?.textContent).toBe('Deleting · 214 items · 42%')
  })

  // The tooltip leads with what the chip itself says, so hovering "Paused" can't
  // open a line claiming the copy is running right now. Pre-fix it read
  // "Copying … · Paused", which English's aspect-free gerund hid and zh's 正在拷贝
  // stated outright next to 已暂停. The countdown stays on the end of it: the
  // work left doesn't stop being true because somebody paused to think, and
  // this chip has to agree with the queue row and the dialog about it.
  it('leads with the paused state, and still says how much is left', () => {
    store?._testApplySnapshot([snapshot({ status: 'paused' })])
    emitProgress(progress())
    renderChip()
    expect(target.querySelector('.tooltip-content')?.textContent).toBe(
      'Paused · 214 items · to Backup · 42% · 1m 20s left',
    )
  })

  it('leaves the count out of the tooltip before the first progress tick', () => {
    store?._testApplySnapshot([snapshot()])
    renderChip()
    expect(target.querySelector('.tooltip-content')?.textContent).toBe('Copying · to Backup · 0%')
  })

  it('keeps an ambient trace of a failure once the toast is gone', () => {
    // Without this, dismissing the toast with the queue window closed would
    // leave zero sign in the main window that anything went wrong.
    store?._testApplySnapshot([failedSnapshot()])
    renderChip()
    expect(chip()?.querySelector('.chip-label')?.textContent).toBe("Couldn't finish")
    // No bar: there's no progress left to describe.
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    expect(chip()?.getAttribute('aria-label')).toBe("1 operation couldn't finish. Open the operation queue to see why.")
  })

  it('counts several failures in the corner', () => {
    store?._testApplySnapshot([failedSnapshot('a'), failedSnapshot('b')])
    renderChip()
    expect(chip()?.getAttribute('aria-label')).toBe(
      "2 operations couldn't finish. Open the operation queue to see why.",
    )
  })

  it('lets a running operation win the corner over a retained failure', () => {
    store?._testApplySnapshot([failedSnapshot('a'), snapshot({ operationId: 'b' })])
    emitProgress(progress({ operationId: 'b' }))
    renderChip()
    expect(chip()?.querySelector('.chip-label')?.textContent).toBe('Copying')
    expect(target.querySelector('[role="progressbar"]')).not.toBeNull()
  })

  it('opens the operation queue from the failure state too', () => {
    store?._testApplySnapshot([failedSnapshot()])
    renderChip()
    chip()?.click()
    flushSync()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
  })

  it('shows an indeterminate scan state rather than a percentage that cannot move', () => {
    // The bridge gives a scanning operation a chip at all, so without this the
    // corner would read "Copying · 0%" for as long as the walk takes. Both
    // totals stay 0 through a scan by design, so the bar has nothing to draw.
    store?._testApplySnapshot([snapshot()])
    emitProgress(progress({ phase: 'scanning', filesDone: 900, filesTotal: 0, bytesTotal: 0 }))
    renderChip()

    expect(chip()?.querySelector('.chip-label')?.textContent).toBe('Copying')
    expect(target.querySelector('[role="progressbar"]'), 'no bar while the totals are unknown').toBeNull()
    expect(chip()?.getAttribute('aria-label')).toBe('Scanning…')
  })

  it('goes back to a real bar once the operation starts writing', () => {
    store?._testApplySnapshot([snapshot()])
    emitProgress(progress())
    renderChip()

    expect(target.querySelector('[role="progressbar"]')).not.toBeNull()
  })
})
