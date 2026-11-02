/**
 * Whether Cmdr has earned the right to offer the Finder extension: "Open in Cmdr"
 * at the top level of Finder's right-click menu.
 *
 * Pure, like its siblings `should-show-reveal-nudge.ts` and
 * `#lib/dock/should-show-dock-nudge.ts`; everything they share is in
 * `#lib/nudges/nudge-ledger.ts`.
 */

import type { FinderExtensionStatus } from '#lib/ipc/bindings.js'
import { nudgeCouldFire, type NudgeContext } from '#lib/nudges/nudge-ledger.js'

/**
 * How many distinct launch days make the offer worth making.
 *
 * Seven, after the reveal handler (2) and the Dock (4): it's the only offer with
 * an ongoing cost, a process running alongside Finder, and the service already
 * gives everyone the same action one submenu down.
 */
export const FINDER_EXTENSION_NUDGE_AFTER_DAYS = 7

/**
 * The switch for the whole offer. Off until a release has proven the real
 * right-click in Finder: this asks once per install, so an offer for a menu item
 * that doesn't work can't be taken back. Tracked in
 * [#853](https://github.com/vdavid/cmdr/issues/853).
 */
export const FINDER_EXTENSION_OFFER_LIVE: boolean = false

/** Everything the decision needs, including the two answers only the backend has. */
export interface FinderExtensionNudgeInputs extends NudgeContext {
  /** Distinct local calendar days in the launch-day ledger. */
  launchDayCount: number
  /** `get_finder_extension_status()`. `null` where there's nothing to ask. */
  extensionStatus: FinderExtensionStatus | null
}

/**
 * Whether to raise the offer. Silent when the extension is already on, when the
 * click would be refused (`blockedBy`: a dev or test build, or a copy outside an
 * Applications folder), and when nothing answered.
 */
export function shouldShowFinderExtensionNudge(inputs: FinderExtensionNudgeInputs): boolean {
  if (!nudgeCouldFire(inputs, 'finderExtension')) return false
  if (inputs.launchDayCount < FINDER_EXTENSION_NUDGE_AFTER_DAYS) return false
  if (inputs.extensionStatus === null) return false
  if (inputs.extensionStatus.blockedBy !== null) return false
  return !inputs.extensionStatus.enabled
}
