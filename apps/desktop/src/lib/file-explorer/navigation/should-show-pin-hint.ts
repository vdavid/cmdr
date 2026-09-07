/**
 * Whether the switcher's Network group has grown long enough to teach the user
 * how to shorten it.
 *
 * Pure, because the decision is the part worth pinning down: the seam that
 * observes the counts and raises the toast is `$lib/stores/volume-store`.
 */

/** How many pinned places make the group long enough to be worth a word. */
export const PIN_HINT_AT = 5

/** How many favorites make the favorites line worth adding. */
export const FAVORITES_LINE_AT = 3

/** The switcher as the hint sees it. */
export interface PinHintInputs {
  /** Server places pinned to the switcher right now. */
  pinnedCount: number
  /** Favorites in the switcher right now. */
  favoriteCount: number
  /** `behavior.serversPinHintSeen`: whether this has already been said once. */
  seen: boolean
}

/** What the toast should say, when there is something to say. */
export interface PinHint {
  /** Whether to add the line about favorites working the same way. */
  mentionFavorites: boolean
}

/**
 * The hint to raise, or `null` for silence.
 *
 * ❗ "At least five", ❌ not "the fifth one just landed": nothing records what
 * the count was last launch, and a person whose list was already long is exactly
 * who the hint is for. The once-ever flag is what keeps it from nagging.
 */
export function shouldShowPinHint({ pinnedCount, favoriteCount, seen }: PinHintInputs): PinHint | null {
  if (seen) return null
  if (pinnedCount < PIN_HINT_AT) return null
  return { mentionFavorites: favoriteCount >= FAVORITES_LINE_AT }
}
