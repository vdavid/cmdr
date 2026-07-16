/**
 * Pure decision for the image-index reclaim-space line: is the stored coverage far
 * enough beyond the current setting to bother offering a cleanup?
 *
 * Lowering the slider is forward-only — it never deletes rows, so a drive indexed at a
 * broad setting keeps those rows searchable after the user narrows the setting (the
 * forward-only slider contract). The reclaim line surfaces that leftover coverage and offers to
 * delete it, but ONLY when it's meaningfully large: a handful of leftover rows isn't
 * worth a destructive prompt. Kept pure so the threshold is unit-testable without a
 * component.
 */

/** Absolute floor: don't offer reclaim for fewer than this many doomed rows. */
export const RECLAIM_MIN_EXCESS = 100
/** Relative floor: the doomed rows must also be more than this fraction of all stored. */
export const RECLAIM_MIN_FRACTION = 0.05

/**
 * Whether to show the reclaim line, given the total stored image rows and how many fall
 * OUTSIDE the current setting (the doomed set). Requires the doomed set to clear BOTH an
 * absolute floor (more than {@link RECLAIM_MIN_EXCESS} rows) and a relative one (more
 * than {@link RECLAIM_MIN_FRACTION} of all stored), so neither a tiny leftover on a huge
 * index nor a small-but-large-fraction sliver on a near-empty index nags the user.
 */
export function shouldOfferReclaim(totalStored: number, doomedCount: number): boolean {
  return doomedCount > RECLAIM_MIN_EXCESS && doomedCount > RECLAIM_MIN_FRACTION * totalStored
}
