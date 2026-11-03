/**
 * When the rail says "This chat is getting long": a pure read of the context gauge's numbers.
 *
 * The nudge follows what the model actually sees, not how many items the thread holds: a chat of
 * short questions can run for a long time, while a few big tool results can fill the budget fast.
 * It fires at {@link LONG_CHAT_SOFT_PERCENT} of the budget or once the backend set older results
 * aside (`contextTrimmed`), whichever comes first, and once more at
 * {@link LONG_CHAT_URGENT_PERCENT} if the user dismissed the first one.
 */

import type { ContextUsage } from './ask-cmdr-context-usage'

/** How pressing the nudge is: `soft` from 70% or a trim, `urgent` from 90% (or over budget). */
export type LongChatLevel = 'soft' | 'urgent'

/** Where the nudge first shows. Earlier than the gauge's own `filling` (80%), so the suggestion to
 * start fresh lands before the gauge turns amber. */
export const LONG_CHAT_SOFT_PERCENT = 70

/** Where a dismissed nudge comes back, once. */
export const LONG_CHAT_URGENT_PERCENT = 90

/** How pressing the thread's length is right now, or `null` while it has room. `trimmed` is true
 * once the backend set history aside for this thread (a `contextTrimmed` line, or a measured call
 * that elided results). An unmeasured thread (`null`, or a zero budget) only counts a trim. */
export function longChatLevel(usage: ContextUsage | null, trimmed: boolean): LongChatLevel | null {
  const percent = usage && usage.budgetTokens > 0 ? (usage.estimatedTokens / usage.budgetTokens) * 100 : 0
  if (percent >= LONG_CHAT_URGENT_PERCENT) return 'urgent'
  if (percent >= LONG_CHAT_SOFT_PERCENT || trimmed) return 'soft'
  return null
}

/** Whether the nudge shows at `level` after the user dismissed it at `dismissedAt` (`null`: never
 * dismissed). A `soft` dismissal holds until `urgent`; an `urgent` one holds for good. */
export function longChatNudgeShows(level: LongChatLevel | null, dismissedAt: LongChatLevel | null): boolean {
  if (level === null) return false
  if (dismissedAt === null) return true
  return dismissedAt === 'soft' && level === 'urgent'
}
