/**
 * The two rules behind the main window's conflict prompt, pure so each branch is
 * provable on its own: who a conflict belongs to, and how much stops while it
 * waits for an answer.
 *
 * They live apart from the controller because both are seams. Ownership widens
 * when a queue row can hand a running operation back to the progress dialog, and
 * the pause narrows to one operation once parallel lanes are allowed to carry on
 * through someone else's clash. Neither change should have to touch a listener.
 */

import type { OperationRow } from './queue/operations-store.svelte'

/** Who answers a `write-conflict`. `unknown` means "not decidable yet" — the
 *  caller must hold the event and ask again, never fall back to a default. */
export type ConflictOwner = 'here' | 'foreground' | 'unknown'

/** What the ownership question is asked against: the foreground slots as they
 *  stand right now. */
export interface ForegroundState {
  /** The operation the progress dialog owns, from `getForegroundOperationId()`. */
  foregroundOperationId: string | null
  /** Whether any dialog is mid-dispatch, from `isForegroundClaimPending()`. */
  claimPending: boolean
}

/**
 * Decides who prompts for a conflict.
 *
 * The `unknown` arm is the one that matters. A dialog whose start command hasn't
 * returned yet owns an operation that has no name, and the backend is already
 * free to emit for it — so an empty slot proves nothing during that window. The
 * two wrong answers cost a double prompt (both surfaces ask about one clash) or
 * a wedge (neither does, and the operation stays parked). Deferring costs
 * milliseconds.
 */
export function conflictOwner(operationId: string, foreground: ForegroundState): ConflictOwner {
  if (foreground.claimPending) return 'unknown'
  return operationId === foreground.foregroundOperationId ? 'foreground' : 'here'
}

/**
 * Which operations stop while the user answers.
 *
 * Today: everything executing, the one that asked included. That is David's
 * call, taken for simplicity, and it is not a constraint — the shape it makes
 * room for is "pause the conflicting operation and let the parallel and
 * next-in-line ones carry on", which is a change to this function and nothing
 * else.
 *
 * `queued` and `paused` rows stay out. A queued operation isn't executing, and
 * an operation the USER paused must not come back when the prompt resumes what
 * it stopped: the controller resumes exactly this list.
 *
 * The conflicting operation is always in the list, even when the snapshot hasn't
 * caught up with it yet: it asked the question, so it is unambiguously live.
 */
export function operationsToPauseFor(conflictOperationId: string, rows: OperationRow[]): string[] {
  const running = rows.filter((r) => r.snapshot.status === 'running').map((r) => r.snapshot.operationId)
  return running.includes(conflictOperationId) ? running : [conflictOperationId, ...running]
}
