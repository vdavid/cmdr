/**
 * The listing loader's staleness predicate: decides whether a streaming listing
 * event belongs to the load that is still current, or is a foreign / stale
 * listing whose events must be dropped.
 *
 * Each `loadDirectory` captures its identity as `{ listingId, generation }` and
 * registers the six streaming listeners closing over it. Every listener's
 * SYNCHRONOUS entry calls this predicate against the pane's live generation
 * counter; a newer `loadDirectory` advances the counter, so an older load's
 * captured generation no longer matches and its listeners no-op — even before
 * their `unlisten*` fires. The `listingId` half is defense-in-depth against a
 * backend event tagged with a different listing id.
 *
 * This is the pane-local guard only. The coordinator-level drop-foreign policy
 * lives separately in `navigate.ts::commitPathFromListing`.
 */
export interface CapturedLoad {
  /** The listing id generated when this load registered its listeners. */
  listingId: string
  /** The load-generation counter value captured at that same point. */
  generation: number
}

/**
 * Returns whether a streaming event should be handled. `true` only when the
 * event's listing id matches the captured load AND that load's generation still
 * equals the live counter. Pure: same two comparisons, same order, as the inline
 * guard it replaces.
 */
export function isEventForCurrentLoad(
  payloadListingId: string,
  captured: CapturedLoad,
  liveGeneration: number,
): boolean {
  return payloadListingId === captured.listingId && captured.generation === liveGeneration
}
