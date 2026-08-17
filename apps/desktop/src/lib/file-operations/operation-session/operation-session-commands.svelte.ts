/**
 * What you can DO to an operation: pause, resume, cancel, roll back, and answer
 * the clash it is parked on.
 *
 * An operation doesn't care where a command came from. A queue row, the main
 * window's conflict prompt, the progress dialog, and later an MCP call all issue
 * the same five against the same guards — and because the in-flight flags live
 * on the shared session, every surface watching that operation sees the press.
 * Two views of one transfer can't offer a Cancel button that one of them has
 * already used.
 *
 * ## Nothing here throws, and every command says whether it landed
 *
 * A view can `void session.cancel()` with no try/catch: a refused request is
 * logged here and reported as `false` (or a `null` verdict). `false` means
 * nothing changed, so whatever the caller is showing should stay exactly as it
 * is. It is also what a guard returns, which is the same answer for the same
 * reason: the request was not sent.
 *
 * ## The guards, and the one asymmetry in them
 *
 * Pause, resume, and the toggle share one guard, because they are one button.
 * Cancel and rollback each hold theirs until the operation is gone rather than
 * until the IPC returns, so a second click sends nothing. Rollback is refused
 * once a cancel is on its way (nothing left to put back), but cancel is NOT
 * refused during a rollback: "stop undoing and keep what's left" is a real
 * thing to want, and it is the only way to ask for it.
 */

import {
  cancelOperation,
  cancelWriteOperation,
  pauseOperation,
  resolveWriteConflict,
  resumeOperation,
  type ConflictId,
  type ConflictResolutionOutcome,
} from '$lib/tauri-commands'
import type { ConflictResolution } from '$lib/file-explorer/types'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('operationSession')

export interface OperationSessionCommands {
  /** Park the operation at its next between-files boundary. It keeps its lane. */
  pause: () => Promise<boolean>
  resume: () => Promise<boolean>
  /** The single Pause/Resume button every surface renders. Which way it goes is
   *  decided from the registry snapshot, never from the caller's idea of the
   *  state. */
  togglePause: () => Promise<boolean>
  /** Stop, keeping what has already been written. Goes through the manager, so
   *  an operation still queued behind a busy lane is dropped before it spawns. */
  cancel: () => Promise<boolean>
  /** Stop AND delete what has already been written. Only meaningful where the
   *  snapshot says `supportsRollback`; the view decides when to offer it. */
  rollback: () => Promise<boolean>
  /** Answer the clash named by `conflictId`, which every surface reads off the
   *  `write-conflict` event it is showing. Returns what the backend acted on —
   *  any verdict means the question is over, whoever asked it — or `null` when
   *  the answer never landed, in which case the prompt stays up. */
  resolveConflict: (
    conflictId: ConflictId,
    resolution: ConflictResolution,
    applyToAll: boolean,
  ) => Promise<ConflictResolutionOutcome | null>

  /** A pause or resume is on its way to the backend. */
  readonly pauseInFlight: boolean
  /** A cancel has been sent and wasn't refused: the operation is on its way out. */
  readonly cancelling: boolean
  /** A rollback has been sent and wasn't refused. */
  readonly rollingBack: boolean
  /** An answer to the conflict is on its way to the backend. */
  readonly resolvingConflict: boolean
}

/**
 * ❌ Never call this directly. Commands reach a view through the session that
 * owns them, so the guards are shared; a second command object for one operation
 * would offer a button the first one has already used.
 *
 * @param isPaused reads the operation's lifecycle status out of the registry
 *   snapshot the session already holds. A PREDICATE rather than a query on
 *   purpose: ❌ the toggle must not spend a round trip on an answer that is
 *   already on screen, and would arrive possibly stale.
 */
export function createOperationSessionCommands(operationId: string, isPaused: () => boolean): OperationSessionCommands {
  let pauseInFlight = $state(false)
  let cancelling = $state(false)
  let rollingBack = $state(false)
  let resolvingConflict = $state(false)

  async function setPaused(next: boolean): Promise<boolean> {
    if (pauseInFlight) return false
    pauseInFlight = true
    try {
      if (next) await pauseOperation(operationId)
      else await resumeOperation(operationId)
      return true
    } catch (error) {
      log.warn('Failed to {intent} op={operationId}: {error}', {
        intent: next ? 'pause' : 'resume',
        operationId,
        error: String(error),
      })
      return false
    } finally {
      pauseInFlight = false
    }
  }

  return {
    pause: () => setPaused(true),
    resume: () => setPaused(false),
    togglePause: () => setPaused(!isPaused()),

    async cancel(): Promise<boolean> {
      if (cancelling) return false
      cancelling = true
      try {
        await cancelOperation(operationId)
        return true
      } catch (error) {
        // The request never landed, so the operation is still going and the
        // user must be able to ask again.
        cancelling = false
        log.error('Failed to cancel op={operationId}: {error}', { operationId, error: String(error) })
        return false
      }
    },

    async rollback(): Promise<boolean> {
      if (rollingBack || cancelling) return false
      rollingBack = true
      try {
        await cancelWriteOperation(operationId, true)
        return true
      } catch (error) {
        rollingBack = false
        log.error('Failed to roll back op={operationId}: {error}', { operationId, error: String(error) })
        return false
      }
    },

    async resolveConflict(
      conflictId: ConflictId,
      resolution: ConflictResolution,
      applyToAll: boolean,
    ): Promise<ConflictResolutionOutcome | null> {
      if (resolvingConflict) return null
      resolvingConflict = true
      try {
        const outcome = await resolveWriteConflict(operationId, conflictId, resolution, applyToAll)
        if (outcome !== 'resolved') {
          log.info('The clash on op={operationId} was settled without this answer ({outcome})', {
            operationId,
            outcome,
          })
        }
        return outcome
      } catch (error) {
        log.error('Failed to answer the clash on op={operationId}: {error}', { operationId, error: String(error) })
        return null
      } finally {
        resolvingConflict = false
      }
    },

    get pauseInFlight(): boolean {
      return pauseInFlight
    },
    get cancelling(): boolean {
      return cancelling
    },
    get rollingBack(): boolean {
      return rollingBack
    },
    get resolvingConflict(): boolean {
      return resolvingConflict
    },
  }
}
