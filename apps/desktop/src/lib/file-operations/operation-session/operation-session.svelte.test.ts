import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { OperationSnapshot, WriteConflictEvent, WriteProgressEvent } from '$lib/ipc/bindings'

// Hoisted: `vi.mock`'s factory runs before the module body, so the mock it
// closes over has to exist by then.
const { listOperationsMock, commandMocks } = vi.hoisted(() => ({
  listOperationsMock: vi.fn<() => Promise<OperationSnapshot[]>>(() => Promise.resolve([])),
  commandMocks: {
    pauseOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    resumeOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    cancelOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    cancelWriteOperation: vi.fn<(id: string, rollback: boolean) => Promise<void>>(() => Promise.resolve()),
    resolveWriteConflict: vi.fn<
      (id: string, conflictId: number, resolution: string, applyToAll: boolean) => Promise<string>
    >(() => Promise.resolve('resolved')),
  },
}))

vi.mock('$lib/tauri-commands', () => ({
  listOperations: listOperationsMock,
  ...commandMocks,
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
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

/** The clash a parked operation is waiting on. `conflictId` defaults to the
 *  operation's first one; a test raising a second clash overrides it, because
 *  that number is what an answer names. */
function conflict(id: string, over: Partial<WriteConflictEvent> = {}): WriteConflictEvent {
  return {
    operationId: id,
    conflictId: 1,
    sourcePath: '/src/f',
    destinationPath: '/dst/f',
    sourceSize: 1,
    destinationSize: 2,
    sourceModified: null,
    destinationModified: null,
    destinationIsNewer: false,
    sizeDifference: 1,
    ...over,
  }
}

beforeEach(() => {
  listOperationsMock.mockReset()
  listOperationsMock.mockResolvedValue([])
  for (const mock of Object.values(commandMocks)) mock.mockReset()
  commandMocks.pauseOperation.mockResolvedValue(undefined)
  commandMocks.resumeOperation.mockResolvedValue(undefined)
  commandMocks.cancelOperation.mockResolvedValue(undefined)
  commandMocks.cancelWriteOperation.mockResolvedValue(undefined)
  commandMocks.resolveWriteConflict.mockResolvedValue('resolved')
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

  it('drops the speed but keeps the countdown while the operation is paused', () => {
    const { fanout, session, dispose } = harness()

    fanout._testEmit({ kind: 'snapshot', operations: [snapshot('a', 'running')] })
    fanout._testEmit({
      kind: 'progress',
      event: progress('a', { bytesPerSecond: 4096, filesPerSecond: 1905, etaSeconds: 58 }),
    })
    expect(session.bytesPerSecondDisplay).toBe(4096)
    expect(session.filesPerSecondDisplay).toBe(1905)
    expect(session.etaSecondsDisplay).toBe(58)

    // A paused transfer emits no further ticks, so the last measured SPEED sits
    // there describing a copy that isn't moving. The time left is a different
    // claim: the backend leaves the paused seconds out of its rate window, so
    // "58s left" is still what remains once the user presses Resume — which is
    // the number they paused to think about.
    fanout._testEmit({ kind: 'snapshot', operations: [snapshot('a', 'paused')] })
    expect(session.bytesPerSecondDisplay).toBeNull()
    expect(session.filesPerSecondDisplay).toBeNull()
    expect(session.etaSecondsDisplay).toBe(58)

    // Resuming brings the speed back: the smoother kept its history, so the
    // display picks up where it left off rather than re-warming from scratch.
    fanout._testEmit({ kind: 'snapshot', operations: [snapshot('a', 'running')] })
    expect(session.bytesPerSecondDisplay).toBe(4096)
    expect(session.etaSecondsDisplay).toBe(58)
    dispose()
  })

  it('drops the speed but keeps the countdown while a clash waits for an answer', () => {
    const { fanout, session, dispose } = harness()

    // Not paused: an operation parked on a conflict prompt is still `running`.
    // The backend's own wait classification is what says a person is deciding.
    fanout._testEmit({ kind: 'snapshot', operations: [snapshot('a', 'running')] })
    fanout._testEmit({
      kind: 'progress',
      event: progress('a', {
        bytesPerSecond: 4096,
        filesPerSecond: 1905,
        etaSeconds: 58,
        activity: { inFlight: 1, stillForSeconds: 0, waitingOn: 'you' },
      }),
    })

    expect(session.bytesPerSecondDisplay).toBeNull()
    expect(session.filesPerSecondDisplay).toBeNull()
    expect(session.etaSecondsDisplay).toBe(58)
    dispose()
  })

  it('keeps showing the speed through a wait on a slow device, which IS the transfer', () => {
    const { fanout, session, dispose } = harness()

    fanout._testEmit({ kind: 'snapshot', operations: [snapshot('a', 'running')] })
    fanout._testEmit({
      kind: 'progress',
      event: progress('a', {
        bytesPerSecond: 4096,
        filesPerSecond: 1905,
        etaSeconds: 58,
        activity: { inFlight: 1, stillForSeconds: 12, waitingOn: 'destination' },
      }),
    })

    expect(session.bytesPerSecondDisplay).toBe(4096)
    expect(session.etaSecondsDisplay).toBe(58)
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

    fanout._testEmit({ kind: 'conflict', event: conflict('a') })

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

describe('commands', () => {
  it('toggles the way the registry snapshot points, not the way the progress event does', async () => {
    const { fanout, session, dispose } = harness()
    // The trap in one emit pair: a parked operation keeps answering
    // `is_running: true` and its last tick still says `copying`, while the
    // snapshot is the only thing that knows it stopped.
    fanout._testEmit({ kind: 'progress', event: progress('a', { phase: 'copying' }) })
    fanout._testEmit({ kind: 'snapshot', operations: [snapshot('a', 'paused')] })

    await session.togglePause()

    expect(commandMocks.resumeOperation).toHaveBeenCalledWith('a')
    expect(commandMocks.pauseOperation).not.toHaveBeenCalled()
    dispose()
  })

  it.each(['resolved', 'already_resolved', 'no_pending_conflict', 'unknown_operation'] as const)(
    'lets go of the clash on a %s verdict, because the question is over either way',
    async (outcome) => {
      commandMocks.resolveWriteConflict.mockResolvedValueOnce(outcome)
      const { fanout, session, dispose } = harness()
      fanout._testEmit({ kind: 'conflict', event: conflict('a') })

      expect(await session.resolveConflict(1, 'overwrite', false)).toBe(outcome)

      expect(session.conflict).toBeNull()
      dispose()
    },
  )

  it('lets go of the clash it answered, and keeps one that arrived while it was answering', async () => {
    // The reported wedge: the backend raises the next clash in the same breath
    // as it takes the answer for this one, so the fan-out delivers it while the
    // IPC promise is still in the air. Clearing whatever sits in the slot when
    // the answer returns throws that newer clash away, and the transfer parks
    // forever with no prompt on screen.
    const { fanout, session, dispose } = harness()
    fanout._testEmit({ kind: 'conflict', event: conflict('a') })
    commandMocks.resolveWriteConflict.mockImplementationOnce(() => {
      fanout._testEmit({ kind: 'conflict', event: conflict('a', { conflictId: 2, destinationPath: '/dst/next' }) })
      return Promise.resolve('resolved')
    })

    expect(await session.resolveConflict(1, 'skip', false)).toBe('resolved')

    expect(session.conflict?.conflictId).toBe(2)
    expect(session.conflict?.destinationPath).toBe('/dst/next')
    dispose()
  })

  it('lets go of a clash somebody else answered', () => {
    // Nothing was called here: an agent answered over MCP, or another window
    // won the race. The operation says the clash is over, and this view has to
    // stop showing it — a modal asking a question with no answer left to give
    // also blocks everything new behind it.
    const { fanout, session, dispose } = harness()
    fanout._testEmit({ kind: 'conflict', event: conflict('a') })

    fanout._testEmit({ kind: 'conflictResolved', event: { operationId: 'a', conflictId: 1 } })

    expect(session.conflict).toBeNull()
    expect(commandMocks.resolveWriteConflict).not.toHaveBeenCalled()
    dispose()
  })

  it('keeps a newer clash when the retraction names the older one', () => {
    // Same race as the answering path: the operation raises its next clash the
    // moment it takes an answer, so the retraction for the old one can land
    // with the new one already on screen.
    const { fanout, session, dispose } = harness()
    fanout._testEmit({ kind: 'conflict', event: conflict('a') })
    fanout._testEmit({ kind: 'conflict', event: conflict('a', { conflictId: 2, destinationPath: '/dst/next' }) })

    fanout._testEmit({ kind: 'conflictResolved', event: { operationId: 'a', conflictId: 1 } })

    expect(session.conflict?.conflictId).toBe(2)
    dispose()
  })

  it('keeps the clash when the answer never landed', async () => {
    commandMocks.resolveWriteConflict.mockRejectedValueOnce(new Error('ipc down'))
    const { fanout, session, dispose } = harness()
    fanout._testEmit({ kind: 'conflict', event: conflict('a') })

    expect(await session.resolveConflict(1, 'overwrite', false)).toBeNull()

    expect(session.conflict?.sourcePath).toBe('/src/f')
    dispose()
  })

  it('answers a clash it never saw, because the backend is the one that arbitrates', async () => {
    // A view can adopt an operation whose `write-conflict` went to a session
    // that has since been let go. Refusing here would leave the user clicking a
    // button that does nothing; the backend's verdict is the only authority.
    const { session, dispose } = harness()

    expect(await session.resolveConflict(7, 'skip', true)).toBe('resolved')

    expect(commandMocks.resolveWriteConflict).toHaveBeenCalledWith('a', 7, 'skip', true)
    dispose()
  })

  it('issues a cancel through the manager, so a queued operation is dropped too', async () => {
    const { session, dispose } = harness()

    expect(await session.cancel()).toBe(true)

    expect(commandMocks.cancelOperation).toHaveBeenCalledWith('a')
    expect(session.cancelling).toBe(true)
    dispose()
  })
})
