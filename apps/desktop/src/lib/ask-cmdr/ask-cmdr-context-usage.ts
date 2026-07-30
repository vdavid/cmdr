/**
 * How full the model's view of this chat got, as the rail's gauge reports it.
 *
 * Pure on purpose: the backend measures (`ContextUsage`, once per answered turn) and this
 * decides only what the user is told. Both figures are `chars/4` ESTIMATES, never a
 * tokenizer's count, so every surface that renders them says "estimated".
 */

/** The last measured turn: what the prompt cost, the budget it was assembled against, and how
 * many older tool results that assembly set aside. */
export type ContextUsage = {
  estimatedTokens: number
  budgetTokens: number
  elidedResults: number
}

/**
 * What the gauge is saying, as one of four named states:
 *
 * - `unmeasured`: no turn has finished yet, so there is nothing honest to show. Deliberately
 *   distinct from 0%, which would read as "plenty of room" for a thread nobody has measured.
 * - `calm`: under {@link FILLING_THRESHOLD_PERCENT} of the budget.
 * - `filling`: at or over the threshold, with nothing dropped yet.
 * - `setAside`: something from the history left the model's view this turn. Going OVER the
 *   budget lands here too rather than in a state of its own: the turn worked, older material
 *   made room for it, and "over budget" is engine vocabulary, not something a user did wrong.
 */
export type ContextUsageState = 'unmeasured' | 'calm' | 'filling' | 'setAside'

/** Where `calm` becomes `filling`. Early enough that a long chat warns before it starts
 * dropping history, late enough that an ordinary chat never nags. */
export const FILLING_THRESHOLD_PERCENT = 80

export function contextUsageState(usage: ContextUsage | null): ContextUsageState {
  // A budget of zero can't produce a percentage, so it is not a measurement — the same
  // "read them as a pair" rule the store applies.
  if (!usage || usage.budgetTokens <= 0) return 'unmeasured'
  if (usage.elidedResults > 0 || usage.estimatedTokens > usage.budgetTokens) return 'setAside'
  return contextUsagePercent(usage) >= FILLING_THRESHOLD_PERCENT ? 'filling' : 'calm'
}

/**
 * The fill percentage, whole numbers, clamped to 0–100 so the bar can't overrun its track.
 *
 * A measured turn never rounds down to 0%: an empty-looking bar for a chat that did send
 * something reads as "nothing used", which is the one thing this gauge exists to correct.
 */
export function contextUsagePercent(usage: ContextUsage | null): number {
  if (!usage || usage.budgetTokens <= 0) return 0
  const exact = (usage.estimatedTokens / usage.budgetTokens) * 100
  return Math.min(100, Math.max(usage.estimatedTokens > 0 ? 1 : 0, Math.round(exact)))
}
