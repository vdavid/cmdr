/**
 * Whether Cmdr has earned the right to ask for a place in the Dock.
 *
 * Pure, because this is the part worth pinning down: the offer is once-ever, so
 * a wrong yes is unrecoverable and a wrong no is a feature nobody is offered.
 * The seam that gathers the inputs and raises the toast is
 * `routes/(main)/startup-gates.ts`.
 *
 * Everything this shares with the other offers — the automated-run and macOS
 * gates, "already asked", and the cooldown one nudge puts on the next — lives in
 * `$lib/nudges/nudge-ledger.ts`. Only the two conditions below are the Dock's own.
 */

import type { DockPinState } from '$lib/ipc/bindings'
import { nudgeCouldFire, type NudgeContext } from '$lib/nudges/nudge-ledger'

/**
 * How many distinct launch days make Cmdr worth keeping around.
 *
 * Four, one behind the reveal offer, which is the more valuable of the two and
 * goes first. The toast says "a few days" rather than naming this, so moving it
 * doesn't make the copy a lie.
 *
 * ❗ Reaching day four doesn't mean the offer lands on day four: the shared
 * cooldown outranks it, so a daily user who was offered the reveal handler on
 * day two hears about the Dock three days after that. That's the cooldown doing
 * its job, ❌ not something to exempt the Dock from.
 */
export const DOCK_NUDGE_AFTER_DAYS = 4

/** Everything the decision needs, including the two answers only the backend has. */
export interface DockNudgeInputs extends NudgeContext {
  /** Distinct local calendar days in the launch-day ledger. */
  launchDayCount: number
  /** `get_dock_pin_state()`, which already covers "already there" and "installed where a tile may point". */
  pinState: DockPinState
}

/**
 * Whether to raise the "keep Cmdr in your Dock?" offer.
 *
 * ❗ "At least four days", ❌ not "the fourth day is today": the ledger only
 * started counting when it shipped, and someone who has been here for months is
 * exactly who the offer is for. The stamp is what keeps it from asking twice.
 *
 * `pinState` answers "already down there" and "installed somewhere a tile may
 * point at" in one value, so neither is re-derived here.
 */
export function shouldShowDockNudge(inputs: DockNudgeInputs): boolean {
  if (!nudgeCouldFire(inputs, 'dockPin')) return false
  if (inputs.launchDayCount < DOCK_NUDGE_AFTER_DAYS) return false
  return inputs.pinState.kind === 'offerable'
}
