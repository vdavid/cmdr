/**
 * What a queue row reads from its operation's session: the estimates nobody else
 * in the window is allowed to keep a second copy of.
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
 * The smoother count is scoped to the queue window on purpose. The progress
 * dialog still builds its own until it becomes a view of a session, so the same
 * assertion for the main window would fail for a reason this file has nothing to
 * say about.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'

vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
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

function snapshot(operationId: string): OperationSnapshot {
  return {
    operationId,
    operationType: 'copy',
    status: 'running',
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
    onPauseResume: () => {},
    onCancel: () => {},
    onRollback: () => {},
    onDismiss: () => {},
  })
  views.push({ operationId, props, instance: mount(QueueRow, { target, props }) })
  flushSync()
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

describe('a scanning row', () => {
  // The backend emits no rate while it counts, so this number exists only
  // because the session measures the walk from the ticks it is already
  // receiving. The row showed a bare tally before it had a session to ask.
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
