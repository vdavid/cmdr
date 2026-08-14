/**
 * The window's event demultiplexer: seven app-wide streams in, one per-operation
 * delivery out.
 *
 * Everything the backend emits about write operations is broadcast to every
 * webview with no addressee, so each interested party would otherwise subscribe
 * to all of them and filter by `operationId`. Ten sessions would mean seventy
 * subscriptions, but listener count is the least of it: the fan-out is a
 * correctness boundary. It is the one place that holds events arriving for an
 * operation no session has claimed yet, the one place that defines arrival
 * order, and the one place that asks the backend where every operation stands
 * when the window opens.
 *
 * It is a router with a holding area, NOT a gate. `createOperationsStore()` is a
 * reducer over ALL operations and keeps receiving everything unbuffered, so the
 * same `write-progress` event has two fates in one window: the store DROPS it
 * when it has no snapshot for that id (`queue/operations-store.svelte.ts`),
 * while the fan-out BUFFERS it. Both are correct, because the store's authority
 * is snapshot membership and the fan-out's job is to hold what a session has not
 * claimed yet.
 *
 * Buffer policy, and why it is what it is: `DETAILS.md` § "The buffer's bound".
 */

import {
  listOperations,
  onWriteProgress,
  onWriteComplete,
  onWriteError,
  onWriteCancelled,
  onWriteSettled,
  onWriteConflict,
  onWriteConflictResolved,
  onOperationsChanged,
  type OperationSnapshot,
  type UnlistenFn,
  type WriteCancelledEvent,
  type WriteCompleteEvent,
  type WriteConflictEvent,
  type WriteConflictResolvedEvent,
  type WriteErrorEvent,
  type WriteProgressEvent,
  type WriteSettledEvent,
} from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('operationSession')

/** One write-stream event, tagged so a session can switch on it. */
export type OperationEventDelivery =
  | { kind: 'progress'; event: WriteProgressEvent }
  | { kind: 'complete'; event: WriteCompleteEvent }
  | { kind: 'error'; event: WriteErrorEvent }
  | { kind: 'cancelled'; event: WriteCancelledEvent }
  | { kind: 'settled'; event: WriteSettledEvent }
  | { kind: 'conflict'; event: WriteConflictEvent }
  | { kind: 'conflictResolved'; event: WriteConflictResolvedEvent }

/** What a session receives: a write-stream event, or its own row out of the
 *  registry snapshot. A row is only ever delivered when the operation is IN the
 *  snapshot; absence is never delivered, because "removed" is what a completed,
 *  a cancelled, and a never-existed operation all look like. */
export type OperationDelivery = OperationEventDelivery | { kind: 'snapshot'; snapshot: OperationSnapshot }

/** What the backend emits: the write streams as they arrive, plus the whole
 *  registry snapshot. The fan-out demultiplexes; `_testEmit` takes this shape so
 *  a test drives the same path a live event does. */
export type OperationStreamEvent = OperationEventDelivery | { kind: 'snapshot'; operations: OperationSnapshot[] }

export type OperationEventSink = (delivery: OperationDelivery) => void

export interface FanoutAttachment {
  detach: () => void
}

export interface OperationEventFanout {
  /** Subscribe to every stream and seed the registry snapshot. Call once at
   *  window init, before any session exists: `listen()` is async, so
   *  subscribing lazily on the first session would miss everything that arrives
   *  while the promise is in flight. Awaiting it is what makes a cold window
   *  cost one `list_operations()` rather than one per row. */
  init: () => Promise<void>
  /** Claim an operation's events. Claim, flush, and go live are ONE synchronous
   *  block: no `await` may be introduced between them. Throws if the operation
   *  is already claimed (one session per operation per window). */
  attach: (operationId: string, sink: OperationEventSink) => FanoutAttachment
  dispose: () => void
  /** Test seam: drive the demultiplexer without a live backend, the way
   *  `operations-store.svelte.ts` exposes `_testApplySnapshot`. */
  _testEmit: (streamEvent: OperationStreamEvent) => void
}

/**
 * How long an unclaimed operation's buffered events are kept. Matches the
 * backend's own retention precedent for scan results (`SCAN_RESULT_TTL`, 300 s
 * in `scan_cache.rs`), including its eviction trigger: the sweep runs on the
 * next insert, never on a timer, so an idle window costs nothing.
 */
export const UNCLAIMED_BUFFER_TTL_MS = 300_000

/** Per-id holding area: the newest delivery of each kind, in arrival order. */
interface UnclaimedBuffer {
  byKind: Map<OperationEventDelivery['kind'], OperationEventDelivery>
  lastEventAtMs: number
}

/** The newest `write-progress` for a LIVE operation, kept whether or not anything
 *  is watching. Separate from the buffer, which is dropped on the first claim. */
interface LastProgress {
  delivery: OperationEventDelivery
  atMs: number
}

export function createOperationEventFanout(): OperationEventFanout {
  const sinks = new Map<string, OperationEventSink>()
  const buffers = new Map<string, UnclaimedBuffer>()
  const lastProgress = new Map<string, LastProgress>()
  let latestSnapshot: OperationSnapshot[] = []
  let unlisteners: UnlistenFn[] = []
  let disposed = false
  /** Whether an `operations-changed` has landed. The seed at init is an `await`
   *  behind the subscriptions, so a broadcast arriving meanwhile is FRESHER and
   *  the seed must stand down. A flag rather than a `latestSnapshot.length`
   *  test: an `operations-changed` carrying zero rows is a real event (the last
   *  operation finishing broadcasts one), and a seed landing after it would
   *  repopulate a snapshot the backend has already emptied. */
  let receivedSnapshot = false

  /** Whether the seed taken at init has been overtaken: by a live broadcast, or
   *  by the window going away. Read through a call rather than off the flags,
   *  because both are set from outside this straight line (a stream callback, a
   *  `dispose()`) and type narrowing can't see that. */
  function seedSuperseded(): boolean {
    return disposed || receivedSnapshot
  }

  function route(delivery: OperationEventDelivery): void {
    const operationId = delivery.event.operationId
    remember(operationId, delivery)
    const sink = sinks.get(operationId)
    if (sink) {
      sink(delivery)
      return
    }
    buffer(operationId, delivery)
  }

  /**
   * Keeps where a LIVE operation had got to, for a session that arrives later.
   *
   * The buffer can't answer that: it is dropped on the first claim, and a paused
   * operation emits nothing to refill it, so a view adopting one would sit at
   * zero for as long as the pause lasts. One tick per live operation, forgotten
   * the moment the operation ends — a session claiming an id after a terminal
   * event must resolve, ❌ never paint bars over an ending.
   */
  function remember(operationId: string, delivery: OperationEventDelivery): void {
    if (delivery.kind === 'progress') {
      lastProgress.set(operationId, { delivery, atMs: Date.now() })
    } else if (delivery.kind !== 'conflict' && delivery.kind !== 'conflictResolved') {
      // A clash and its answer bracket a pause, ❌ not an ending: the operation
      // is exactly where its last tick left it, and a session arriving after
      // either one still needs that tick to paint anything at all.
      lastProgress.delete(operationId)
    }
  }

  function buffer(operationId: string, delivery: OperationEventDelivery): void {
    const nowMs = Date.now()
    sweep(nowMs)
    let held = buffers.get(operationId)
    if (!held) {
      held = { byKind: new Map(), lastEventAtMs: nowMs }
      buffers.set(operationId, held)
    }
    // Re-insert so the map's iteration order stays ARRIVAL order of each kind's
    // newest event: a flush must never hand a session an older progress sample
    // after a newer one (the session's ETA smoother is stateful).
    held.byKind.delete(delivery.kind)
    held.byKind.set(delivery.kind, delivery)
    held.lastEventAtMs = nowMs
  }

  function sweep(nowMs: number): void {
    for (const [operationId, held] of buffers) {
      if (nowMs - held.lastEventAtMs > UNCLAIMED_BUFFER_TTL_MS) buffers.delete(operationId)
    }
    for (const [operationId, held] of lastProgress) {
      if (nowMs - held.atMs > UNCLAIMED_BUFFER_TTL_MS) lastProgress.delete(operationId)
    }
  }

  function applySnapshot(operations: OperationSnapshot[]): void {
    receivedSnapshot = true
    latestSnapshot = operations
    for (const row of operations) {
      sinks.get(row.operationId)?.({ kind: 'snapshot', snapshot: row })
    }
  }

  function attach(operationId: string, sink: OperationEventSink): FanoutAttachment {
    if (sinks.has(operationId)) {
      throw new Error(`Operation ${operationId} already has a session in this window`)
    }
    // Claim first, so an event arriving during the flush is delivered live
    // rather than buffered behind it.
    sinks.set(operationId, sink)

    const row = latestSnapshot.find((op) => op.operationId === operationId)
    if (row) sink({ kind: 'snapshot', snapshot: row })

    const held = buffers.get(operationId)
    if (held) {
      buffers.delete(operationId)
      for (const delivery of held.byKind.values()) sink(delivery)
    }

    // Where the operation had got to, for a session that arrives later. Skipped
    // when the flush already carried a tick: that one is newer, and feeding an
    // older sample after a newer one corrupts the session's ETA smoother.
    const remembered = lastProgress.get(operationId)
    if (remembered && !held?.byKind.has('progress')) sink(remembered.delivery)

    return {
      detach(): void {
        if (sinks.get(operationId) === sink) sinks.delete(operationId)
      },
    }
  }

  async function init(): Promise<void> {
    try {
      const subscriptions = await Promise.all([
        onWriteProgress((event) => {
          route({ kind: 'progress', event })
        }),
        onWriteComplete((event) => {
          route({ kind: 'complete', event })
        }),
        onWriteError((event) => {
          route({ kind: 'error', event })
        }),
        onWriteCancelled((event) => {
          route({ kind: 'cancelled', event })
        }),
        onWriteSettled((event) => {
          route({ kind: 'settled', event })
        }),
        onWriteConflict((event) => {
          route({ kind: 'conflict', event })
        }),
        onWriteConflictResolved((event) => {
          route({ kind: 'conflictResolved', event })
        }),
        onOperationsChanged((event) => {
          applySnapshot(event.operations)
        }),
      ])
      unlisteners = subscriptions
      // Disposed while we awaited: undo whatever landed late.
      if (disposed) {
        dropListeners()
        return
      }

      // Open holding the registry snapshot, so the first session to attach is
      // handed its row instead of asking for it. A cold window has heard no
      // `operations-changed` yet, and without this every row's session would
      // spend an IPC round trip on the answer the next broadcast carries
      // anyway. Seeded AFTER the subscriptions, never beside them: a seed taken
      // before the listeners are live could be older than a broadcast nobody
      // was there to hear, and nothing would correct it.
      const operations = await listOperations()
      // A broadcast that landed while we awaited is fresher than the seed. Same
      // shape and same guard as `createOperationsStore.init`.
      if (seedSuperseded()) return
      applySnapshot(operations)
    } catch (error) {
      // A dead fan-out means silent sessions, not a dead window; a failed seed
      // just leaves each session to seed itself, as it did before this call.
      log.warn('Failed to start the operation event fan-out: {error}', { error: String(error) })
    }
  }

  function dropListeners(): void {
    for (const unlisten of unlisteners) unlisten()
    unlisteners = []
  }

  function dispose(): void {
    disposed = true
    dropListeners()
    sinks.clear()
    buffers.clear()
    lastProgress.clear()
    latestSnapshot = []
  }

  return {
    init,
    attach,
    dispose,
    _testEmit(streamEvent: OperationStreamEvent): void {
      if (streamEvent.kind === 'snapshot') applySnapshot(streamEvent.operations)
      else route(streamEvent)
    },
  }
}
