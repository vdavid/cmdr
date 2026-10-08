/**
 * The fire-and-forget arm's one shape: start the work, don't wait for it, and
 * absorb its rejection.
 *
 * A bare `void promise` escapes the gesture dispatcher's `catch` (the dispatch
 * already returned), so whatever the opener throws lands as an unhandled
 * rejection, logged at error level and attached to error reports. The opener
 * has already said its piece to the user (a toast, or a pane that re-lists a
 * gone listing), so a debug line is all that's left to say, the same one the
 * gesture dispatcher writes for an awaited arm.
 */
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('user-action')

/** Runs `work` without awaiting it; a rejection becomes a debug line. */
export function detached(work: Promise<unknown> | undefined): void {
  work?.catch((e: unknown) => {
    log.debug('Fire-and-forget command work rejected: {reason}', {
      reason: e instanceof Error ? e.message : String(e),
    })
  })
}
