/**
 * Pins the crown-jewel drop-foreign-listings predicate for the listing loader.
 *
 * A streaming listing event is accepted ONLY when it belongs to the load that is
 * still current: its `listingId` matches the one captured when the listeners were
 * registered AND the generation captured at that load still equals the live
 * generation counter. A newer `loadDirectory` advances the live generation, so an
 * older load's events (foreign / stale listings) become inert. If either half of
 * the predicate is dropped, a foreign listing can land in the wrong pane or
 * overwrite a newer navigation — the exact regression these tests catch.
 */
import { describe, it, expect } from 'vitest'
import { isEventForCurrentLoad } from './listing-token'

describe('isEventForCurrentLoad', () => {
  const captured = { listingId: 'load-A', generation: 3 }

  it('accepts an event whose listingId + generation both still match the current load', () => {
    expect(isEventForCurrentLoad('load-A', captured, 3)).toBe(true)
  })

  it('drops an event once a newer load has advanced the live generation', () => {
    // Same listingId, but a later loadDirectory bumped the live generation to 4.
    expect(isEventForCurrentLoad('load-A', captured, 4)).toBe(false)
  })

  it('drops an event tagged with a different listingId (defense-in-depth)', () => {
    // Same live generation, but the event carries a foreign listing id.
    expect(isEventForCurrentLoad('load-B', captured, 3)).toBe(false)
  })

  it('drops an event that is stale on both counts', () => {
    expect(isEventForCurrentLoad('load-B', captured, 4)).toBe(false)
  })
})
