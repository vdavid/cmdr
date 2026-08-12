/**
 * How a view binds to an operation's session.
 *
 * A view names the operation it is looking at, and gets back the session for it
 * for as long as it keeps looking. The refcount is handled here, so no view has
 * to remember to release: the component's own reactive scope owns the binding,
 * and letting go happens when the view unmounts or points somewhere else.
 *
 * ❌ Never call `createOperationSession` directly. The registry is what makes
 * "two views of one operation share one session" structural rather than
 * remembered, and the reason is the stateful estimators: a second smoother
 * started later disagrees with the first until it converges.
 *
 * The session is `null` for the first frame (the effect runs after mount) and
 * until the window's registry has been created, so a caller renders whatever it
 * can say without one. That is one tick of a missing ETA, never a missing row.
 *
 * The runes in here are the VIEW's, which is why this lives beside a session
 * rather than inside one: a session may hold no `$derived` at all, because it
 * outlives whichever component happened to create it. This binder is the
 * opposite — it's born and torn down with its view, and that's the whole job.
 */

import type { OperationSession } from './operation-session.svelte'
import { getOperationSessions } from './window-operation-sessions.svelte'

export interface BoundOperationSession {
  /** The session for the operation this view named, or `null` before the
   *  binding takes hold. */
  readonly current: OperationSession | null
}

/**
 * Binds this view to the session for `operationId()`, following it as the view
 * changes its mind. Call from a component's init (it owns an `$effect`).
 */
export function bindOperationSession(operationId: () => string | null): BoundOperationSession {
  let session = $state.raw<OperationSession | null>(null)

  /** The id as a VALUE, so the binding survives everything around it churning.
   *  A caller reads its id off an object the store rebuilds on every
   *  `operations-changed` tick, and re-binding on each of those would hand the
   *  operation a brand-new smoother mid-transfer: the estimate would restart
   *  whenever some unrelated operation started or finished. A `$derived` string
   *  compares equal and stops the churn here. */
  const boundId = $derived(operationId())

  $effect(() => {
    const id = boundId
    // Reactive on purpose: the registry appears when the window finishes its
    // init, and a view mounted before that must pick it up rather than stay
    // sessionless for the operation's whole life.
    const registry = getOperationSessions()
    if (id === null || registry === null) {
      session = null
      return
    }
    session = registry.acquire(id)
    return () => {
      registry.release(id)
    }
  })

  return {
    get current(): OperationSession | null {
      return session
    },
  }
}
