import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type {
  OperationSnapshot,
  WriteCancelledEvent,
  WriteCompleteEvent,
  WriteProgressEvent,
  WriteSettledEvent,
} from '$lib/ipc/bindings'

// The fan-out is the only thing in the window that subscribes to the write
// streams, so every one of them is mocked here and driven through `_testEmit`.
const unlisteners = {
  progress: vi.fn(),
  complete: vi.fn(),
  error: vi.fn(),
  cancelled: vi.fn(),
  settled: vi.fn(),
  conflict: vi.fn(),
  operations: vi.fn(),
}

vi.mock('$lib/tauri-commands', () => ({
  onWriteProgress: vi.fn(() => Promise.resolve(unlisteners.progress)),
  onWriteComplete: vi.fn(() => Promise.resolve(unlisteners.complete)),
  onWriteError: vi.fn(() => Promise.resolve(unlisteners.error)),
  onWriteCancelled: vi.fn(() => Promise.resolve(unlisteners.cancelled)),
  onWriteSettled: vi.fn(() => Promise.resolve(unlisteners.settled)),
  onWriteConflict: vi.fn(() => Promise.resolve(unlisteners.conflict)),
  onOperationsChanged: vi.fn(() => Promise.resolve(unlisteners.operations)),
}))

import {
  onWriteProgress,
  onWriteComplete,
  onWriteError,
  onWriteCancelled,
  onWriteSettled,
  onWriteConflict,
  onOperationsChanged,
} from '$lib/tauri-commands'
import { createOperationEventFanout, UNCLAIMED_BUFFER_TTL_MS, type OperationDelivery } from './operation-event-fanout'

function progress(id: string, over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
  return {
    operationId: id,
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'file',
    filesDone: 1,
    filesTotal: 2,
    bytesDone: 50,
    bytesTotal: 100,
    ...over,
  }
}

function complete(id: string, over: Partial<WriteCompleteEvent> = {}): WriteCompleteEvent {
  return { operationId: id, operationType: 'copy', filesProcessed: 2, filesSkipped: 0, bytesProcessed: 100, ...over }
}

function cancelled(id: string): WriteCancelledEvent {
  return { operationId: id, operationType: 'copy', filesProcessed: 1, rolledBack: false }
}

function settled(id: string): WriteSettledEvent {
  return { operationId: id, operationType: 'copy' }
}

function snapshot(id: string, status: OperationSnapshot['status'] = 'running'): OperationSnapshot {
  return {
    operationId: id,
    operationType: 'copy',
    status,
    source: '/src',
    destination: '/dst',
    supportsRollback: true,
    error: null,
  }
}

/** Records every delivery a session would see, in arrival order. */
function recorder(): { deliveries: OperationDelivery[]; sink: (delivery: OperationDelivery) => void } {
  const deliveries: OperationDelivery[] = []
  return { deliveries, sink: (delivery) => deliveries.push(delivery) }
}

beforeEach(() => {
  vi.clearAllMocks()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('routing', () => {
  it('delivers an event only to the session attached to its operation', () => {
    const fanout = createOperationEventFanout()
    const a = recorder()
    const b = recorder()
    fanout.attach('a', a.sink)
    fanout.attach('b', b.sink)

    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 70 }) })

    expect(a.deliveries).toHaveLength(1)
    expect(b.deliveries).toHaveLength(0)
    fanout.dispose()
  })

  it('stops delivering once a session detaches', () => {
    const fanout = createOperationEventFanout()
    const a = recorder()
    const attachment = fanout.attach('a', a.sink)
    attachment.detach()

    fanout._testEmit({ kind: 'progress', event: progress('a') })

    expect(a.deliveries).toHaveLength(0)
    fanout.dispose()
  })

  it('refuses a second session for the same operation, so one id can only have one truth', () => {
    const fanout = createOperationEventFanout()
    fanout.attach('a', recorder().sink)
    expect(() => fanout.attach('a', recorder().sink)).toThrow()
    fanout.dispose()
  })

  it('hands an attaching session the operation`s row from the latest registry snapshot', () => {
    const fanout = createOperationEventFanout()
    fanout._testEmit({ kind: 'snapshot', operations: [snapshot('a', 'paused'), snapshot('b')] })

    const a = recorder()
    fanout.attach('a', a.sink)

    expect(a.deliveries).toEqual([{ kind: 'snapshot', snapshot: snapshot('a', 'paused') }])
    fanout.dispose()
  })

  it('tells a session nothing when its operation is absent from a snapshot, because absence means "removed"', () => {
    const fanout = createOperationEventFanout()
    const a = recorder()
    fanout.attach('a', a.sink)

    fanout._testEmit({ kind: 'snapshot', operations: [snapshot('a')] })
    fanout._testEmit({ kind: 'snapshot', operations: [] })

    expect(a.deliveries).toEqual([{ kind: 'snapshot', snapshot: snapshot('a') }])
    fanout.dispose()
  })
})

describe('subscriptions', () => {
  it('subscribes to every write stream at init, before any session exists', async () => {
    const fanout = createOperationEventFanout()
    await fanout.init()

    for (const subscribe of [
      onWriteProgress,
      onWriteComplete,
      onWriteError,
      onWriteCancelled,
      onWriteSettled,
      onWriteConflict,
      onOperationsChanged,
    ]) {
      expect(subscribe).toHaveBeenCalledTimes(1)
    }
    fanout.dispose()
  })

  it('drops every listener on dispose', async () => {
    const fanout = createOperationEventFanout()
    await fanout.init()
    fanout.dispose()

    for (const unlisten of Object.values(unlisteners)) {
      expect(unlisten).toHaveBeenCalledTimes(1)
    }
  })

  it('unsubscribes whatever lands after a dispose that raced the init', async () => {
    const fanout = createOperationEventFanout()
    const pending = fanout.init()
    fanout.dispose()
    await pending

    for (const unlisten of Object.values(unlisteners)) {
      expect(unlisten).toHaveBeenCalledTimes(1)
    }
  })
})

describe('buffering for an unclaimed operation', () => {
  it('flushes what arrived before the session existed', () => {
    const fanout = createOperationEventFanout()
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 60 }) })
    fanout._testEmit({ kind: 'complete', event: complete('a') })

    const a = recorder()
    fanout.attach('a', a.sink)

    expect(a.deliveries.map((d) => d.kind)).toEqual(['progress', 'complete'])
    fanout.dispose()
  })

  it('keeps only the NEWEST write-progress per unclaimed id, so the buffer is bounded by ids, not by ticks', () => {
    const fanout = createOperationEventFanout()
    for (let i = 1; i <= 50; i++) {
      fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: i }) })
    }

    const a = recorder()
    fanout.attach('a', a.sink)

    expect(a.deliveries).toHaveLength(1)
    expect(a.deliveries[0]).toMatchObject({ kind: 'progress', event: { bytesDone: 50 } })
    fanout.dispose()
  })

  it('keeps at most one of each terminal event, so a late session resolves rather than replaying', () => {
    const fanout = createOperationEventFanout()
    fanout._testEmit({ kind: 'cancelled', event: cancelled('a') })
    fanout._testEmit({ kind: 'settled', event: settled('a') })
    fanout._testEmit({ kind: 'settled', event: settled('a') })

    const a = recorder()
    fanout.attach('a', a.sink)

    expect(a.deliveries.map((d) => d.kind)).toEqual(['cancelled', 'settled'])
    fanout.dispose()
  })

  it('flushes in arrival order, with each kind at the position of its newest event', () => {
    const fanout = createOperationEventFanout()
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 10 }) })
    fanout._testEmit({ kind: 'cancelled', event: cancelled('a') })
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 20 }) })

    const a = recorder()
    fanout.attach('a', a.sink)

    expect(a.deliveries.map((d) => d.kind)).toEqual(['cancelled', 'progress'])
    expect(a.deliveries[1]).toMatchObject({ event: { bytesDone: 20 } })
    fanout.dispose()
  })

  it('drops a claimed id`s buffer, so a later session never replays an ended operation twice', () => {
    const fanout = createOperationEventFanout()
    fanout._testEmit({ kind: 'complete', event: complete('a') })

    const first = recorder()
    fanout.attach('a', first.sink).detach()
    const second = recorder()
    fanout.attach('a', second.sink)

    expect(first.deliveries).toHaveLength(1)
    expect(second.deliveries).toHaveLength(0)
    fanout.dispose()
  })

  it('ages an unclaimed buffer out on the TTL, swept by the next event', () => {
    vi.useFakeTimers()
    const fanout = createOperationEventFanout()
    fanout._testEmit({ kind: 'progress', event: progress('stale') })

    vi.advanceTimersByTime(UNCLAIMED_BUFFER_TTL_MS + 1)
    // The sweep runs on the next insert, on the backend's own precedent.
    fanout._testEmit({ kind: 'progress', event: progress('fresh') })

    const stale = recorder()
    fanout.attach('stale', stale.sink)
    const fresh = recorder()
    fanout.attach('fresh', fresh.sink)

    expect(stale.deliveries).toHaveLength(0)
    expect(fresh.deliveries).toHaveLength(1)
    fanout.dispose()
  })

  it('flushes the whole buffer before any live event for that id reaches the session', () => {
    const fanout = createOperationEventFanout()
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 10 }) })

    // The interleaving is explicit: attach and the live emit are one synchronous
    // block, so a flush deferred to a microtask would land in the wrong order.
    const a = recorder()
    fanout.attach('a', a.sink)
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 20 }) })

    expect(a.deliveries.map((d) => (d.kind === 'progress' ? d.event.bytesDone : null))).toEqual([10, 20])
    fanout.dispose()
  })
})

describe('where an operation had got to, for a session that arrives later', () => {
  // A view can attach to an operation long after it started (the queue's Show
  // button). The buffer alone can't answer "where is it now": it's dropped on
  // the first claim, and a PAUSED operation emits nothing to refill it, so the
  // second session would sit at zero for as long as the pause lasts.

  it('hands a second session the last tick, even though the first session drained the buffer', () => {
    const fanout = createOperationEventFanout()
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 10 }) })

    const first = recorder()
    const held = fanout.attach('a', first.sink)
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 80 }) })
    held.detach()

    const second = recorder()
    fanout.attach('a', second.sink)

    expect(second.deliveries).toHaveLength(1)
    expect(second.deliveries[0]).toMatchObject({ kind: 'progress', event: { bytesDone: 80 } })
    fanout.dispose()
  })

  it('prefers the buffered tick, which is newer than the last delivered one', () => {
    const fanout = createOperationEventFanout()
    const first = recorder()
    fanout.attach('a', first.sink).detach()
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 30 }) })

    // Arrives while nobody holds the id, so it's buffered as well as retained.
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 90 }) })
    const second = recorder()
    fanout.attach('a', second.sink)

    expect(second.deliveries).toHaveLength(1)
    expect(second.deliveries[0]).toMatchObject({ kind: 'progress', event: { bytesDone: 90 } })
    fanout.dispose()
  })

  it('forgets it once the operation ends, so a later session never paints bars over an ending', () => {
    const fanout = createOperationEventFanout()
    const first = recorder()
    const held = fanout.attach('a', first.sink)
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 50 }) })
    fanout._testEmit({ kind: 'complete', event: complete('a') })
    held.detach()

    const second = recorder()
    fanout.attach('a', second.sink)

    expect(second.deliveries).toHaveLength(0)
    fanout.dispose()
  })

  it('ages out on the same TTL as the buffer', () => {
    vi.useFakeTimers()
    const fanout = createOperationEventFanout()
    const first = recorder()
    const held = fanout.attach('a', first.sink)
    fanout._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 50 }) })
    held.detach()

    vi.advanceTimersByTime(UNCLAIMED_BUFFER_TTL_MS + 1)
    fanout._testEmit({ kind: 'progress', event: progress('unrelated') })

    const second = recorder()
    fanout.attach('a', second.sink)

    expect(second.deliveries).toHaveLength(0)
    fanout.dispose()
    vi.useRealTimers()
  })
})
