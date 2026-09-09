/**
 * Whether Cmdr has earned the right to ask for a place in the Dock.
 *
 * Pure, because this is the part worth pinning down: the offer is once-ever, so
 * a wrong yes is unrecoverable and a wrong no is a feature nobody is offered.
 * The seam that gathers the inputs and raises the toast is
 * `routes/(main)/startup-gates.ts`.
 */

import type { DockPinState } from '$lib/ipc/bindings'

/**
 * How many distinct launch days make Cmdr worth keeping around.
 *
 * Three, because it's the smallest number that means "came back on purpose"
 * rather than "tried it once". The toast says "a few days" rather than naming
 * this, so moving it doesn't make the copy a lie.
 */
export const DOCK_NUDGE_AFTER_DAYS = 3

/** The half of the decision that costs nothing to ask. */
export interface DockNudgeContext {
  /** `isE2eRun()`: a Playwright shard or the screenshot capture pass. */
  automatedRun: boolean
  /** Whether this is a Mac at all. There's no Dock anywhere else. */
  onMacOs: boolean
  /** `behavior.dockPinNudgeSeen`: whether the offer has already been made. */
  seen: boolean
  /** `onboarding.completed`. */
  onboarded: boolean
  /** Whether the onboarding wizard is on screen right now. */
  onboardingShowing: boolean
}

/** Everything the decision needs, including the two answers only the backend has. */
export interface DockNudgeInputs extends DockNudgeContext {
  /** Distinct local calendar days in the launch-day ledger. */
  launchDayCount: number
  /** `get_dock_pin_state()`, which already covers "already there" and "installed where a tile may point". */
  pinState: DockPinState
}

/**
 * Whether it's still worth asking the backend anything.
 *
 * Split out so the common case (an offer already made, which is every launch
 * after the first yes or no) never pays for two IPC round trips.
 * {@link shouldShowDockNudge} runs it too, so the two can't drift.
 */
export function dockNudgeCouldFire({
  automatedRun,
  onMacOs,
  seen,
  onboarded,
  onboardingShowing,
}: DockNudgeContext): boolean {
  if (automatedRun) return false
  if (!onMacOs) return false
  if (seen) return false
  if (!onboarded) return false
  if (onboardingShowing) return false
  return true
}

/**
 * Whether to raise the "keep Cmdr in your Dock?" offer.
 *
 * ❗ "At least three days", ❌ not "the third day is today": the ledger only
 * started counting when it shipped, and someone who has been here for months is
 * exactly who the offer is for. The once-ever flag is what keeps it from asking
 * twice.
 *
 * `pinState` answers "already down there" and "installed somewhere a tile may
 * point at" in one value, so neither is re-derived here.
 */
export function shouldShowDockNudge(inputs: DockNudgeInputs): boolean {
  if (!dockNudgeCouldFire(inputs)) return false
  if (inputs.launchDayCount < DOCK_NUDGE_AFTER_DAYS) return false
  return inputs.pinState.kind === 'offerable'
}
