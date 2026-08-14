/**
 * What a queue row reads from its operation's session, and what it asks of it.
 *
 * The command half lives here rather than in `QueueRow.svelte.test.ts` because a
 * row issues its Pause / Cancel / Rollback through the session, which only
 * exists once the window has a registry. That file keeps the affordance
 * questions (which control shows for which status); this one answers what a
 * click actually does.
 *
 * The read half: the estimates nobody else in the window is allowed to keep a
 * second copy of.
 *
 * Both are stateful, and that is the whole reason they live in one place. The
 * ETA smoother has a shipped precedent behind it (one operation once read
 * "8m 12s remaining" in one window and "5m 46s" in the other), and two smoothers
 * fed identical samples from identical starting points would agree, so the
 * hazard only bites when one of them starts later — which is exactly what a view
 * attaching to a transfer already in flight does. Counting constructions is how
 * that stays impossible rather than merely absent: nothing else here would
 * notice a second layer quietly reappearing in the store.
 *
 * The smoother count is scoped to the queue window on purpose; the main
 * window's half of the same guarantee lives beside the surface it protects, in
 * `../transfer/transfer-progress-state.svelte.test.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'

const { commandMocks } = vi.hoisted(() => ({
  commandMocks: {
    pauseOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    resumeOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    cancelOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    cancelWriteOperation: vi.fn<(id: string, rollback: boolean) => Promise<void>>(() => Promise.resolve()),
    resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  },
}))

vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve([])),
  ...commandMocks,
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
  // `ModalDialog` (the rollback question) registers itself with the backend's
  // soft-dialog tracker on mount and unmount.
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

// `<Size>` deep in the readout reads reactive settings; the real path needs the
// settings store.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'decimal',
}))

// The real smoother, watched. Both the session and (were it ever to grow one
// again) the store resolve this same module, so the count covers the window.
vi.mock('../progress-readout', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../progress-readout')>()
  return { ...actual, createEtaSmoother: vi.fn(actual.createEtaSmoother) }
})

import { createEtaSmoother } from '../progress-readout'
import {
  destroyOperationSessions,
  getOperationSessions,
  initOperationSessions,
} from '../operation-session/window-operation-sessions.svelte'
import { createOperationsStore, type OperationRow } from './operations-store.svelte'
import QueueRow from './QueueRow.svelte'

function snapshot(operationId: string, status: OperationSnapshot['status'] = 'running'): OperationSnapshot {
  return {
    operationId,
    operationType: 'copy',
    status,
    source: '/Users/me/Documents/report.pdf',
    destination: '/Volumes/Backup/report.pdf',
    supportsRollback: true,
    error: null,
  }
}

function progress(operationId: string, over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
  return {
    operationId,
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'report.pdf',
    filesDone: 2,
    filesTotal: 10,
    bytesDone: 420,
    bytesTotal: 1000,
    etaSeconds: 80,
    ...over,
  }
}

let store: ReturnType<typeof createOperationsStore>
/** The mounted rows, each with the props object it re-reads its row from. */
let views: { operationId: string; props: { row: OperationRow }; instance: ReturnType<typeof mount> }[] = []
let target: HTMLElement

/** Hands every mounted row its current row object, the way the page's `{#each}`
 *  does on any store change. */
function refreshRows(): void {
  for (const view of views) view.props.row = rowFor(view.operationId)
  flushSync()
}

/** One `operations-changed`, delivered the way the backend delivers it: to
 *  every listener in the window. The store reduces membership from it, and each
 *  session learns its own lifecycle status, which is what a Pause/Resume press
 *  steers by. */
function emitSnapshot(operations: OperationSnapshot[]): void {
  store._testApplySnapshot(operations)
  getOperationSessions()?._testEmit({ kind: 'snapshot', operations })
  flushSync()
  refreshRows()
}

/** One tick, delivered the way the backend delivers it: to every listener in
 *  the window. The store keeps the latest one, the session smooths it. */
function emitProgress(event: WriteProgressEvent): void {
  store._testApplyProgress(event)
  getOperationSessions()?._testEmit({ kind: 'progress', event })
  flushSync()
  refreshRows()
}

/** Mounts the queue window's row for an operation, which is what claims the
 *  session for it. */
function mountRow(operationId: string): void {
  const props = $state({
    row: rowFor(operationId),
    selected: false,
    onToggleSelect: () => {},
    onDismiss: () => {},
  })
  views.push({ operationId, props, instance: mount(QueueRow, { target, props }) })
  flushSync()
}

/** A control on the row, by its accessible name (Rollback carries none, so it
 *  goes by label text). */
function button(name: string): HTMLButtonElement {
  const found =
    target.querySelector<HTMLButtonElement>(`[aria-label="${name}"]`) ??
    [...target.querySelectorAll('button')].find((b) => b.textContent.includes(name))
  if (!found) throw new Error(`No "${name}" control on the row`)
  return found
}

function rowFor(operationId: string): OperationRow {
  const row = store.operations.find((r) => r.snapshot.operationId === operationId)
  if (!row) throw new Error(`No row for ${operationId}`)
  return row
}

beforeEach(async () => {
  document.body.innerHTML = ''
  target = document.createElement('ul')
  document.body.appendChild(target)
  views = []
  store = createOperationsStore()
  await initOperationSessions()
  vi.mocked(createEtaSmoother).mockClear()
  for (const mock of Object.values(commandMocks)) mock.mockClear()
})

afterEach(() => {
  for (const view of views) void unmount(view.instance)
  views = []
  store.dispose()
  destroyOperationSessions()
})

describe('the queue window smooths an ETA exactly once per operation', () => {
  it('builds one smoother however many ticks arrive', () => {
    store._testApplySnapshot([snapshot('op-a')])
    flushSync()
    mountRow('op-a')

    emitProgress(progress('op-a'))
    emitProgress(progress('op-a', { bytesDone: 500 }))
    emitProgress(progress('op-a', { bytesDone: 600 }))

    expect(vi.mocked(createEtaSmoother)).toHaveBeenCalledTimes(1)
    // And the smoothed number is what reaches the row: proof the one smoother
    // is the one the user reads.
    expect(target.textContent).toContain('1m 20s')
  })

  it('keeps it across the snapshot ticks that rebuild the row', () => {
    // `operations-changed` replaces every row object, and it fires whenever
    // anything in the registry moves — another operation starting, one
    // finishing. A row that re-binds on each of those would drop its smoother
    // and start over mid-transfer, which is the divergence in slow motion.
    store._testApplySnapshot([snapshot('op-a')])
    flushSync()
    mountRow('op-a')
    emitProgress(progress('op-a'))

    store._testApplySnapshot([snapshot('op-a'), snapshot('op-b')])
    flushSync()
    refreshRows()
    emitProgress(progress('op-a', { bytesDone: 500 }))

    expect(vi.mocked(createEtaSmoother)).toHaveBeenCalledTimes(1)
  })

  it('gives a second operation its own, and stops there', () => {
    store._testApplySnapshot([snapshot('op-a'), snapshot('op-b')])
    flushSync()
    mountRow('op-a')
    mountRow('op-b')

    emitProgress(progress('op-a'))
    emitProgress(progress('op-b'))

    expect(vi.mocked(createEtaSmoother)).toHaveBeenCalledTimes(2)
  })

  it('leaves the store with none of its own', () => {
    // The store reduces membership and the latest raw tick, both stateless. A
    // smoother here would be the second layer, and it would only diverge for
    // the late-attaching view this design exists to serve.
    store._testApplySnapshot([snapshot('op-a')])
    store._testApplyProgress(progress('op-a'))
    store._testApplyProgress(progress('op-a', { bytesDone: 500 }))
    flushSync()

    expect(vi.mocked(createEtaSmoother)).not.toHaveBeenCalled()
    expect(rowFor('op-a').progress?.bytesDone).toBe(500)
  })
})

describe('a row commands its operation through the session', () => {
  it('pauses a running operation and resumes a paused one', async () => {
    emitSnapshot([snapshot('op-a')])
    mountRow('op-a')

    button('Pause this operation').click()
    expect(commandMocks.pauseOperation).toHaveBeenCalledWith('op-a')
    // Let the pause round-trip finish; until it does, the session refuses the
    // next press on the same button.
    await Promise.resolve()

    // Which way the one button goes is decided from the lifecycle status, and
    // the session learns it from the same snapshot the row renders.
    emitSnapshot([snapshot('op-a', 'paused')])
    button('Resume this operation').click()

    expect(commandMocks.resumeOperation).toHaveBeenCalledWith('op-a')
  })

  it('cancels through the manager, so a row still waiting on a lane is dropped too', () => {
    emitSnapshot([snapshot('op-a', 'queued')])
    mountRow('op-a')

    button('Cancel this operation').click()

    expect(commandMocks.cancelOperation).toHaveBeenCalledWith('op-a')
  })

  it('rolls back by asking the write operation to undo what it wrote, once the user says so', () => {
    emitSnapshot([snapshot('op-a')])
    mountRow('op-a')

    button('Rollback').click()
    flushSync()
    // The click asks; it doesn't delete. Rollback removes every file the
    // operation has written, and one it overwrote has no backup.
    expect(commandMocks.cancelWriteOperation).not.toHaveBeenCalled()

    button('Roll back').click()

    expect(commandMocks.cancelWriteOperation).toHaveBeenCalledWith('op-a', true)
  })

  it('keeps the files when the rollback question is declined', () => {
    emitSnapshot([snapshot('op-a')])
    mountRow('op-a')

    button('Rollback').click()
    flushSync()
    button('Keep them').click()
    flushSync()

    expect(commandMocks.cancelWriteOperation).not.toHaveBeenCalled()
    // The row is unchanged, so the operation can still be rolled back later.
    expect(button('Rollback')).toBeDefined()
  })

  it('sends one cancel however many times the button is pressed', async () => {
    emitSnapshot([snapshot('op-a')])
    mountRow('op-a')

    button('Cancel this operation').click()
    await Promise.resolve()
    button('Cancel this operation').click()

    expect(commandMocks.cancelOperation).toHaveBeenCalledTimes(1)
  })
})

describe('a row parked on a clash', () => {
  // The row is the surface somebody goes looking at when nothing is happening,
  // and "Running" over a frozen bar is the answer that sends them hunting. The
  // lifecycle status is still `running` here (a clash doesn't pause anything),
  // so the word has to come from what the backend says it's waiting on.
  it('says it needs an answer instead of reading as plain Running', () => {
    emitSnapshot([snapshot('op-a')])
    mountRow('op-a')
    emitProgress(progress('op-a', { bytesPerSecond: 4096, filesPerSecond: 8 }))
    expect(target.querySelector('.status-text')?.textContent).toBe('Running')

    emitProgress(
      progress('op-a', {
        bytesPerSecond: 4096,
        filesPerSecond: 8,
        activity: { inFlight: 0, stillForSeconds: 0, waitingOn: 'you' },
      }),
    )

    expect(target.querySelector('.status-text')?.textContent).toBe('Needs your answer')
    // And the speed goes with it: nothing is moving, so there's no honest one.
    expect(target.textContent).not.toContain('/s')
  })

  it('goes back to Running once the answer is in', () => {
    emitSnapshot([snapshot('op-a')])
    mountRow('op-a')
    emitProgress(progress('op-a', { activity: { inFlight: 0, stillForSeconds: 0, waitingOn: 'you' } }))

    emitProgress(progress('op-a', { bytesDone: 600 }))

    expect(target.querySelector('.status-text')?.textContent).toBe('Running')
  })
})

describe('a scanning row', () => {
  // The backend emits no rate while it counts, so this number exists only
  // because the session measures the walk from the ticks the row is already
  // rendering.
  it('shows how fast the walk is going', () => {
    vi.useFakeTimers()
    try {
      store._testApplySnapshot([snapshot('op-a')])
      flushSync()
      mountRow('op-a')

      const scanning = { phase: 'scanning' as const, filesTotal: 0, bytesTotal: 0 }
      emitProgress(progress('op-a', { ...scanning, filesDone: 400, bytesDone: 1000 }))
      // One rate needs two samples with time between them, which is also why a
      // scan opens without one.
      expect(target.querySelector('.scan-throughput')).toBeNull()

      vi.advanceTimersByTime(1000)
      emitProgress(progress('op-a', { ...scanning, filesDone: 900, bytesDone: 3000 }))

      expect(target.querySelector('.scan-throughput')?.textContent).toContain('500 files/s')
    } finally {
      vi.useRealTimers()
    }
  })
})
