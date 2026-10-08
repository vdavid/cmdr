import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const h = vi.hoisted(() => ({
  keepListingsAlive: vi.fn(),
  goneListeners: new Set<(listingId: string) => void>(),
}))
vi.mock('$lib/tauri-commands', () => ({
  keepListingsAlive: h.keepListingsAlive,
  onListingGone: (listener: (listingId: string) => void) => {
    h.goneListeners.add(listener)
    return () => h.goneListeners.delete(listener)
  },
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import { LISTING_HEARTBEAT_MS, trackLiveListing, untrackLiveListing } from './listing-liveness'

const tracked: string[] = []
function track(id: string, onGone: () => void): void {
  tracked.push(id)
  trackLiveListing(id, onGone)
}

/** What a listing-read wrapper does with the backend's `Gone` refusal (`throwListingLookupError`). */
function readFails(listingId: string): void {
  for (const listener of h.goneListeners) listener(listingId)
}

/** Lets one heartbeat fire and settle. */
async function heartbeat(): Promise<void> {
  await vi.advanceTimersByTimeAsync(LISTING_HEARTBEAT_MS)
}

beforeEach(() => {
  vi.useFakeTimers()
  h.keepListingsAlive.mockReset()
  h.keepListingsAlive.mockResolvedValue([])
})

afterEach(() => {
  for (const id of tracked.splice(0)) untrackLiveListing(id)
  vi.useRealTimers()
})

describe('listing liveness', () => {
  it('tells the owning pane once when a read finds its listing gone', () => {
    const onGone = vi.fn()
    track('a', onGone)

    readFails('a')
    readFails('a')

    expect(onGone).toHaveBeenCalledTimes(1)
  })

  // A read on a listing the pane already walked away from, or one still loading, also
  // answers "gone"; neither may make a pane re-list.
  it('ignores a gone refusal for a listing nobody tracks', () => {
    const onGone = vi.fn()
    track('a', onGone)
    untrackLiveListing('a')

    readFails('a')
    readFails('never-tracked')

    expect(onGone).not.toHaveBeenCalled()
  })

  it('a later owner replaces the earlier one (a pane swap hands a listing over)', () => {
    const first = vi.fn()
    const second = vi.fn()
    track('a', first)
    track('a', second)

    readFails('a')

    expect(first).not.toHaveBeenCalled()
    expect(second).toHaveBeenCalledTimes(1)
  })

  it('the heartbeat names every tracked listing and re-lists the ones the backend lost', async () => {
    const onGoneA = vi.fn()
    const onGoneB = vi.fn()
    track('a', onGoneA)
    track('b', onGoneB)
    h.keepListingsAlive.mockResolvedValue(['b'])

    await heartbeat()

    expect(h.keepListingsAlive).toHaveBeenCalledWith(['a', 'b'])
    expect(onGoneA).not.toHaveBeenCalled()
    expect(onGoneB).toHaveBeenCalledTimes(1)
  })

  it('beats well inside the six-hour reaper window, and only while something is tracked', async () => {
    expect(LISTING_HEARTBEAT_MS).toBeLessThanOrEqual(60 * 60 * 1000)

    await heartbeat()
    expect(h.keepListingsAlive).not.toHaveBeenCalled()

    track('a', vi.fn())
    await heartbeat()
    expect(h.keepListingsAlive).toHaveBeenCalledTimes(1)

    untrackLiveListing('a')
    await heartbeat()
    await heartbeat()
    expect(h.keepListingsAlive).toHaveBeenCalledTimes(1)
  })

  it('a heartbeat the backend refuses leaves every pane alone', async () => {
    const onGone = vi.fn()
    track('a', onGone)
    h.keepListingsAlive.mockRejectedValue(new Error('IPC down'))

    await heartbeat()

    expect(onGone).not.toHaveBeenCalled()
  })
})
