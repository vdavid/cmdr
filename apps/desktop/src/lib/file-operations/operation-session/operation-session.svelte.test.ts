import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'

// Hoisted: `vi.mock`'s factory runs before the module body, so the mock it
// closes over has to exist by then.
const { listOperationsMock } = vi.hoisted(() => ({
  listOperationsMock: vi.fn<() => Promise<OperationSnapshot[]>>(() => Promise.resolve([])),
}))

vi.mock('$lib/tauri-commands', () => ({
  listOperations: listOperationsMock,
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
}))

import { createOperationEventFanout, type OperationEventFanout } from './operation-event-fanout'
import { createOperationSession, type OperationSession } from './operation-session.svelte'

function snapshot(
  id: string,
  status: OperationSnapshot['status'] = 'running',
  over: Partial<OperationSnapshot> = {},
): OperationSnapshot {
  return {
    operationId: id,
    operationType: 'copy',
    status,
    source: '/src',
    destination: '/dst',
    supportsRollback: true,
    error: null,
    ...over,
  }
}

function progress(id: string, over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
  return {
    operationId: id,
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'file',
    filesDone: 1,
    filesTotal: 10,
    bytesDone: 50,
    bytesTotal: 1000,
    ...over,
  }
}

/** A promise plus its resolver, so a test decides exactly when an async
 *  dependency settles (and can leave it pending indefinitely). */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

/** A fan-out plus one session on it, inside a reactive scope so the session's
 *  derived state has somewhere to live. `before` runs against the fan-out while
 *  the operation is still unclaimed. */
function harness(before?: (fanout: OperationEventFanout) => void) {
  const fanout = createOperationEventFanout()
  before?.(fanout)
  let session!: OperationSession
  const stopScope = $effect.root(() => {
    session = createOperationSession('a', fanout)
  })
  // An arrow property, not a method: tests destructure it off the harness.
  const dispose = (): void => {
    session.dispose()
    stopScope()
    fanout.dispose()
  }
  return { fanout, session, dispose }
}

/** Lets the seed's `list_operations()` promise and its continuation run. */
async function settleSeed(): Promise<void> {
  await listOperationsMock.mock.results[0]?.value
  await Promise.resolve()
}

beforeEach(() => {
  listOperationsMock.mockReset()
  listOperationsMock.mockResolvedValue([])
})

afterEach(() => {
  vi.useRealTimers()
})

describe('seeding', () => {
  it('recovers an operation the window has heard nothing about', async () => {
    listOperationsMock.mockResolvedValue([snapshot('a', 'paused')])
    const { session, dispose } = harness()

    await settleSeed()

    expect(session.status).toBe('paused')
    expect(session.settled).toBe(false)
    dispose()
  })

  it('resolves an operation the registry has never heard of as already over', async () => {
    listOperationsMock.mockResolvedValue([snapshot('someone-else')])
    const { session, dispose } = harness()

    await settleSeed()

    expect(session.settled).toBe(true)
    expect(session.outcome).toEqual({ kind: 'gone' })
    dispose()
  })

  it('lets a terminal event that lands mid-seed win over the seed', async () => {
    // The interleaving is explicit: the seed stays pending until this test says
    // otherwise, so it can't pass on incidental microtask ordering.
    const pendingSeed = deferred<OperationSnapshot[]>()
    listOperationsMock.mockReturnValue(pendingSeed.promise)
    const { fanout, session, dispose } = harness()

    fanout._testEmit({
      kind: 'complete',
      event: { operationId: 'a', operationType: 'copy', filesProcessed: 3, filesSkipped: 0, bytesProcessed: 30 },
    })
    // The registry still had the operation when the seed was taken.
    pendingSeed.resolve([snapshot('a', 'running')])
    await settleSeed()

    expect(session.outcome?.kind).toBe('complete')
    expect(session.status).toBeNull()
    dispose()
  })

  it('presents a seeded scanning operation as live and counting, not as stuck at zero', async () => {
    // What a reload lands on mid-scan: a `Running` record with no progress yet,
    // whose first tick counts with both totals still at 0.
    listOperationsMock.mockResolvedValue([snapshot('a', 'running')])
    const { fanout, session, dispose } = harness()
    await settleSeed()

    fanout._testEmit({
      kind: 'progress',
      event: progress('a', {
        phase: 'scanning',
        filesDone: 120,
        dirsDone: 7,
        bytesDone: 4096,
        filesTotal: 0,
        bytesTotal: 0,
        currentDir: '/src/deep',
      }),
    })

    expect(session.status).toBe('running')
    expect(session.phase).toBe('scanning')
    expect(session.settled).toBe(false)
    expect(session.scan.filesFound).toBe(120)
    expect(session.scan.dirsFound).toBe(7)
    expect(session.scan.bytesFound).toBe(4096)
    expect(session.scan.currentDir).toBe('/src/deep')
    // No ETA is invented while the totals are unknown.
    expect(session.etaSecondsDisplay).toBeNull()
    dispose()
  })

  it('reads what the fan-out buffered before it existed, and skips the seed for it', async () => {
    listOperationsMock.mockResolvedValue([snapshot('a', 'running')])
    const { session, dispose } = harness((fanout) => {
      fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 900 }) })
      fanout._testEmit({
        kind: 'cancelled',
        event: { operationId: 'a', operationType: 'copy', filesProcessed: 4, rolledBack: false },
      })
    })

    expect(session.outcome?.kind).toBe('cancelled')
    expect(session.readout?.bytesDone).toBe(900)
    // Not merely discarded: never asked for. Every view of every row would
    // otherwise cost an IPC round trip for an answer the attach already gave.
    expect(listOperationsMock).not.toHaveBeenCalled()

    await settleSeed()

    expect(session.status).toBeNull()
    dispose()
  })
})

describe('derived read state', () => {
  it('settles on a terminal event, never on leaving the snapshot', () => {
    const { fanout, session, dispose } = harness()

    fanout._testEmit({ kind: 'snapshot', operations: [snapshot('a')] })
    fanout._testEmit({ kind: 'snapshot', operations: [] })
    expect(session.settled).toBe(false)
    expect(session.status).toBe('running')

    fanout._testEmit({ kind: 'settled', event: { operationId: 'a', operationType: 'copy' } })
    // `write-settled` says the task tore down; it doesn't say how it ended.
    expect(session.settleEventReceived).toBe(true)
    expect(session.settled).toBe(false)

    fanout._testEmit({
      kind: 'error',
      event: { operationId: 'a', operationType: 'copy', error: { type: 'source_not_found', path: '/src' } },
    })
    expect(session.settled).toBe(true)
    expect(session.outcome?.kind).toBe('error')
    dispose()
  })

  it('keeps the first outcome when a second terminal event follows', () => {
    const { fanout, session, dispose } = harness()

    fanout._testEmit({
      kind: 'complete',
      event: { operationId: 'a', operationType: 'copy', filesProcessed: 2, filesSkipped: 0, bytesProcessed: 20 },
    })
    fanout._testEmit({
      kind: 'cancelled',
      event: { operationId: 'a', operationType: 'copy', filesProcessed: 2, rolledBack: false },
    })

    expect(session.outcome?.kind).toBe('complete')
    dispose()
  })

  it('smooths the ETA across ticks with one smoother', () => {
    const { fanout, session, dispose } = harness()

    fanout._testEmit({ kind: 'progress', event: progress('a', { etaSeconds: 100 }) })
    expect(session.etaSecondsDisplay).toBe(100)
    fanout._testEmit({ kind: 'progress', event: progress('a', { etaSeconds: 200 }) })
    // 100 + 0.25 * (200 - 100): one smoother carried across ticks, not a re-warm.
    expect(session.etaSecondsDisplay).toBe(125)
    dispose()
  })

  it('re-warms the ETA and drops the scan rates when the phase changes', () => {
    vi.useFakeTimers()
    const { fanout, session, dispose } = harness()

    fanout._testEmit({ kind: 'progress', event: progress('a', { phase: 'scanning', filesDone: 10, bytesDone: 100 }) })
    vi.advanceTimersByTime(1000)
    fanout._testEmit({ kind: 'progress', event: progress('a', { phase: 'scanning', filesDone: 30, bytesDone: 300 }) })
    expect(session.scan.filesPerSecond).toBe(20)

    fanout._testEmit({ kind: 'progress', event: progress('a', { phase: 'copying', etaSeconds: 60 }) })
    expect(session.scan.filesPerSecond).toBeNull()
    // The write phase's first ETA is adopted as-is, never blended with the scan's.
    expect(session.etaSecondsDisplay).toBe(60)
    dispose()
  })

  it('holds the conflict the operation is parked on', () => {
    const { fanout, session, dispose } = harness()

    fanout._testEmit({
      kind: 'conflict',
      event: {
        operationId: 'a',
        sourcePath: '/src/f',
        destinationPath: '/dst/f',
        sourceSize: 1,
        destinationSize: 2,
        sourceModified: null,
        destinationModified: null,
        destinationIsNewer: false,
        sizeDifference: 1,
      },
    })

    expect(session.conflict?.sourcePath).toBe('/src/f')
    dispose()
  })

  it('stops reading the stream once disposed', () => {
    const { fanout, session } = harness()
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 10 }) })
    session.dispose()

    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 999 }) })

    expect(session.readout?.bytesDone).toBe(10)
    fanout.dispose()
  })
})
