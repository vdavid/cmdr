/**
 * Whether Cmdr has earned the right to ask for "Show in Finder".
 *
 * Pure, because this is the part worth pinning down: the offer is once-ever AND
 * it changes a machine-wide macOS preference, so a wrong yes is unrecoverable
 * and rude. The seam that gathers the inputs and raises the toast is
 * `routes/(main)/startup-gates.ts`.
 *
 * The sibling of `$lib/dock/should-show-dock-nudge.ts`; everything the two
 * share is in `$lib/nudges/nudge-ledger.ts`.
 */

import type { RevealHandlerState } from '$lib/ipc/bindings'
import { nudgeCouldFire, type NudgeContext } from '$lib/nudges/nudge-ledger'

/**
 * How many distinct launch days make the offer worth making.
 *
 * Two, a launch day ahead of the Dock offer: catching "Show in Finder" is the
 * more valuable of the two, so it gets the earlier slot. The toast says "for a
 * while now" rather than naming this, so moving it doesn't make the copy a lie.
 */
export const REVEAL_NUDGE_AFTER_DAYS = 2

/** Everything the decision needs, including the two answers only the backend has. */
export interface RevealNudgeInputs extends NudgeContext {
  /** Distinct local calendar days in the launch-day ledger. */
  launchDayCount: number
  /** `get_reveal_handler_state()`: who holds the `NSFileViewer` key right now. */
  handlerState: RevealHandlerState
}

/**
 * Whether to raise the "open Show in Finder in Cmdr?" offer.
 *
 * `notRegistered` is the ONLY state that speaks up, and each of the other three
 * is a deliberate silence:
 *
 * - `registered`: reveals already land here, so there's nothing to offer.
 * - `heldByOtherApp`: someone chose Path Finder or ForkLift on purpose.
 *   Offering to take a working setup over out of nowhere would be rude; a
 *   take-over belongs in the Settings row, where the person asked for it.
 * - `unavailable`: a build that must never write the key (debug, worktree, E2E)
 *   or a platform with no mechanism, so the click couldn't deliver anything.
 */
export function shouldShowRevealNudge(inputs: RevealNudgeInputs): boolean {
  if (!nudgeCouldFire(inputs, 'reveal')) return false
  if (inputs.launchDayCount < REVEAL_NUDGE_AFTER_DAYS) return false
  return inputs.handlerState.kind === 'notRegistered'
}
