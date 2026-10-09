// An expanded queue row's on-demand details: the full source and destination
// paths and the operation's timing, fetched over `get_operation_details` when
// the row opens. ❌ Never on `operations-changed`, which stays thin: most rows
// are never expanded, and a selection can hold thousands of sources. DETAILS §
// "Row details".

import type { OperationDetails } from '$lib/ipc/bindings'
import { getOperationDetails } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('queue')

export interface OperationDetailsLoader {
  /** The latest answer for the operation last asked about, or null before one
   *  lands and once the operation is gone. A refresh keeps the previous answer
   *  up until the new one replaces it, so the panel doesn't blink. */
  readonly details: OperationDetails | null
  /** The bridge failed (a missing permission, a dead backend). The panel says
   *  so instead of showing an empty box that looks like a bug. */
  readonly unavailable: boolean
  /** Ask (again) about `operationId`. Only the newest request's answer lands. */
  load(operationId: string): void
  /** Drops every answer still on its way. Call when the row goes away. */
  dispose(): void
}

/** One per expanded row. Answers are keyed by the operation id they were asked
 *  about and by a request counter, ❌ never by the row's position: a row can
 *  move when another operation settles, and an answer that raced a newer
 *  request (or the row leaving) is dropped rather than shown on the wrong one. */
export function createOperationDetailsLoader(): OperationDetailsLoader {
  let details = $state.raw<OperationDetails | null>(null)
  let unavailable = $state(false)
  let latestRequest = 0
  let disposed = false

  async function fetchFor(operationId: string, request: number): Promise<void> {
    try {
      const answer = await getOperationDetails(operationId)
      if (disposed || request !== latestRequest) return
      details = answer?.operationId === operationId ? answer : null
      unavailable = false
    } catch (error) {
      if (disposed || request !== latestRequest) return
      log.warn('Could not load details for op={operationId}: {error}', { operationId, error: String(error) })
      unavailable = true
    }
  }

  return {
    get details() {
      return details
    },
    get unavailable() {
      return unavailable
    },
    load(operationId: string): void {
      if (disposed) return
      latestRequest += 1
      void fetchFor(operationId, latestRequest)
    },
    dispose(): void {
      disposed = true
    },
  }
}
