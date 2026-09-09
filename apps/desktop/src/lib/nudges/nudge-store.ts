/**
 * Where the nudge ledger lives: one hidden `behavior.*NudgeOfferedAt` setting
 * per offer, read as a whole and written one stamp at a time.
 *
 * Separate from `nudge-ledger.ts` so the rule itself stays pure and the only
 * settings knowledge in the feature — which key holds which stamp — sits in one
 * table.
 */

import { getSetting, setSetting } from '$lib/settings'
import type { SettingId } from '$lib/settings/types'
import { type NudgeKind, type NudgeLedger } from './nudge-ledger'

/**
 * The setting each offer stamps.
 *
 * A `Record<NudgeKind, …>`, so adding a `NudgeKind` without giving it a home is
 * a compile error rather than a nudge that silently forgets it ever fired.
 */
const NUDGE_SETTINGS = {
  dockPin: 'behavior.dockPinNudgeOfferedAt',
  reveal: 'behavior.revealNudgeOfferedAt',
  // `as const` so each id keeps its literal type and `getSetting` answers a
  // `string` rather than the whole `SettingsValues` union.
} as const satisfies Record<NudgeKind, SettingId>

/** Every stamp at once, since the cooldown is a question about all of them. */
export function readNudgeLedger(): NudgeLedger {
  return {
    dockPin: getSetting(NUDGE_SETTINGS.dockPin),
    reveal: getSetting(NUDGE_SETTINGS.reveal),
  }
}

/**
 * Stamps `kind` as offered now.
 *
 * ❗ Called when the toast goes UP, ❌ never when it's answered: a crash
 * mid-toast then costs one offer, where the other order risks repeating it
 * forever. Same rule as `maybeFireUpgradeNudge`.
 */
export function markNudgeOffered(kind: NudgeKind, now: Date = new Date()): void {
  setSetting(NUDGE_SETTINGS[kind], now.toISOString())
}
