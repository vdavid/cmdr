/**
 * Pure layout helper for the recent-searches footer strip (R3 U1).
 *
 * Round 2 sliced the first 6 entries unconditionally and let the strip
 * scroll horizontally on overflow. David's R3 brief asks for a dynamic
 * layout instead:
 *   - "Recent searches:" label on the left, always rendered.
 *   - "All searches… ⌘H" button on the right, always rendered.
 *   - Middle slot: pack AS MANY chips as fit in the remaining width; the
 *     rest are dropped silently (no scroll, no ellipsis chip).
 *
 * This helper takes the strip width and a per-chip width estimate (the
 * caller pre-measures with pretext or with a rendered + hidden DOM probe)
 * and returns the count of chips that fit. Pure, so we can unit-test the
 * greedy-fit rule with mocked widths.
 */

export interface RecentChipsLayoutInput {
  /** Total width available to the strip in pixels. */
  stripWidth: number
  /** Width of the leading "Recent searches:" label including margin. */
  leadingLabelWidth: number
  /** Width of the trailing "All searches… ⌘H" button including margin. */
  trailingButtonWidth: number
  /** Gap between consecutive items inside the middle slot. */
  itemGap: number
  /** Per-chip measured widths, in the order the chips would be rendered. */
  chipWidths: number[]
}

export interface RecentChipsLayoutResult {
  /** Number of chips from the start of the list that should be rendered. */
  visibleCount: number
}

/**
 * Returns the largest prefix of `chipWidths` that fits the strip's middle
 * slot. The middle slot is `stripWidth - leadingLabelWidth - trailingButtonWidth`
 * minus two `itemGap`s (one between label + first chip, one between last
 * chip + trailing button). Between consecutive chips we add one `itemGap` too.
 *
 * Returns `{ visibleCount: 0 }` when the strip is too narrow for even one
 * chip; the caller still renders the leading label and trailing button.
 *
 * Defensive: if any measurement is non-finite or non-positive we fall back
 * to "show all" so a measurement bug doesn't silently hide the user's
 * recent searches.
 */
export function computeRecentChipsLayout(input: RecentChipsLayoutInput): RecentChipsLayoutResult {
  const { stripWidth, leadingLabelWidth, trailingButtonWidth, itemGap, chipWidths } = input
  if (!Number.isFinite(stripWidth) || stripWidth <= 0) {
    return { visibleCount: chipWidths.length }
  }
  const middle = stripWidth - leadingLabelWidth - trailingButtonWidth
  if (middle <= 0) return { visibleCount: 0 }
  // Account for the two outer gaps (label↔first chip, last chip↔trailing button).
  let used = 2 * itemGap
  let visible = 0
  for (let i = 0; i < chipWidths.length; i++) {
    const w = chipWidths[i]
    const next = used + w + (visible === 0 ? 0 : itemGap)
    if (next > middle) break
    used = next
    visible += 1
  }
  return { visibleCount: visible }
}
