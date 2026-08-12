/**
 * One session per operation per window, refcounted.
 *
 * Two views of the same operation MUST share one session, and the registry is
 * what makes that structural rather than remembered. A view that attaches twenty
 * minutes into a transfer would otherwise build its own ETA smoother, whose
 * first sample is the current rate, while the queue's smoother carries twenty
 * minutes of history: the two then disagree on screen until the new one
 * converges. Late attachment is the point of the whole design, so the shared
 * instance is the design. `DETAILS.md` § "Why a registry".
 *
 * `acquire` / `release` pair like any refcount: the session lives until the last
 * view lets go, and a re-acquired id gets a FRESH session rather than a revived
 * one (a disposed session has detached from the fan-out and would never update
 * again).
 */

import {
  createOperationEventFanout,
  type OperationEventFanout,
  type OperationStreamEvent,
} from './operation-event-fanout'
import { createOperationSession, type OperationSession } from './operation-session.svelte'

export interface OperationSessionRegistry {
  /** Subscribe the fan-out. Call once at window init, before any view asks for
   *  a session. */
  init: () => Promise<void>
  /** The session for this operation, created on first ask. Every caller must
   *  {@link release} exactly once. */
  acquire: (operationId: string) => OperationSession
  release: (operationId: string) => void
  dispose: () => void
  /** Test seam: drive the window's streams without a live backend. */
  _testEmit: (streamEvent: OperationStreamEvent) => void
}

interface Held {
  session: OperationSession
  viewers: number
}

export function createOperationSessionRegistry(
  fanout: OperationEventFanout = createOperationEventFanout(),
): OperationSessionRegistry {
  const held = new Map<string, Held>()

  return {
    init(): Promise<void> {
      return fanout.init()
    },
    acquire(operationId: string): OperationSession {
      const existing = held.get(operationId)
      if (existing) {
        existing.viewers += 1
        return existing.session
      }
      const session = createOperationSession(operationId, fanout)
      held.set(operationId, { session, viewers: 1 })
      return session
    },
    release(operationId: string): void {
      const entry = held.get(operationId)
      if (!entry) return
      entry.viewers -= 1
      if (entry.viewers > 0) return
      entry.session.dispose()
      held.delete(operationId)
    },
    dispose(): void {
      for (const entry of held.values()) entry.session.dispose()
      held.clear()
      fanout.dispose()
    },
    _testEmit(streamEvent: OperationStreamEvent): void {
      fanout._testEmit(streamEvent)
    },
  }
}
