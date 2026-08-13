/**
 * The window's event demultiplexer: seven app-wide streams in, one per-operation
 * delivery out.
 *
 * Everything the backend emits about write operations is broadcast to every
 * webview with no addressee, so each interested party would otherwise subscribe
 * to all of them and filter by `operationId`. Ten sessions would mean seventy
 * subscriptions, but listener count is the least of it: the fan-out is a
 * correctness boundary. It is the one place that holds events arriving for an
 * operation no session has claimed yet, and the one place that defines arrival
 * order.
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
  onWriteProgress,
  onWriteComplete,
  onWriteError,
  onWriteCancelled,
  onWriteSettled,
  onWriteConflict,
  onOperationsChanged,
  type OperationSnapshot,
  type UnlistenFn,
  type WriteCancelledEvent,
  type WriteCompleteEvent,
  type WriteConflictEvent,
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
  /** Subscribe to every stream. Call once at window init, before any session
   *  exists: `listen()` is async, so subscribing lazily on the first session
   *  would miss everything that arrives while the promise is in flight. */
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

export function createOperationEventFanout(): OperationEventFanout {
  const sinks = new Map<string, OperationEventSink>()
  const buffers = new Map<string, UnclaimedBuffer>()
  let latestSnapshot: OperationSnapshot[] = []
  let unlisteners: UnlistenFn[] = []
  let disposed = false

  function route(delivery: OperationEventDelivery): void {
    const operationId = delivery.event.operationId
    const sink = sinks.get(operationId)
    if (sink) {
      sink(delivery)
      return
    }
    buffer(operationId, delivery)
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
  }

  function applySnapshot(operations: OperationSnapshot[]): void {
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
        onOperationsChanged((event) => {
          applySnapshot(event.operations)
        }),
      ])
      unlisteners = subscriptions
      // Disposed while we awaited: undo whatever landed late.
      if (disposed) dropListeners()
    } catch (error) {
      // A dead fan-out means silent sessions, not a dead window.
      log.warn('Failed to subscribe the operation event fan-out: {error}', { error: String(error) })
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
