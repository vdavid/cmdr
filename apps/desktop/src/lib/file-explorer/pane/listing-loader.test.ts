/**
 * Integration tests for `createListingLoader` — the streaming directory-load
 * pipeline + the generation / listingId drop-foreign-listings token model.
 *
 * The crown jewel: once a newer `loadDirectory` has run, the OLDER load's still-
 * registered streaming listeners must no-op. These tests fire an older load's
 * captured callbacks AFTER a newer load has started and assert nothing lands.
 * Proven red-by-mutation: deleting the generation half of the predicate in
 * `listing-token.ts` (`captured.generation === liveGeneration`) makes the
 * "foreign * dropped" cases fail; restoring it greens them.
 *
 * Two async tails are deliberately UNGUARDED (current behavior, preserved): the
 * `onListingError` `pathExistsChecked` continuation and `handleListingComplete`'s
 * post-`await findFileIndex` cursor write. The "async tail runs unguarded" test
 * is a behavior-lock, not a correctness assertion — it pins today's shape so a
 * later tidy-up can't silently re-guard it.
 *
 * The shared harness lives in `listing-loader.test-fixtures.ts`. How a load ends,
 * the navigation branches, and teardown have sibling suites of their own.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const h = vi.hoisted(() => ({
  listeners: {
    opening: [] as ((p: unknown) => void)[],
    progress: [] as ((p: unknown) => void)[],
    readComplete: [] as ((p: unknown) => void)[],
    complete: [] as ((p: unknown) => void)[],
    error: [] as ((p: unknown) => void)[],
    cancelled: [] as ((p: unknown) => void)[],
  },
  cancelListing: vi.fn(),
  listDirectoryEnd: vi.fn(),
  listDirectoryStart: vi.fn(),
  findFileIndex: vi.fn(),
  pathExistsChecked: vi.fn(),
  resolvePathVolume: vi.fn(),
  trackEvent: vi.fn(),
  resolveValidPath: vi.fn(),
  getSetting: vi.fn(),
}))

function register(bucket: ((p: unknown) => void)[]) {
  return (cb: (p: unknown) => void) => {
    bucket.push(cb)
    return Promise.resolve(vi.fn())
  }
}

vi.mock('$lib/tauri-commands', () => ({
  onListingOpening: register(h.listeners.opening),
  onListingProgress: register(h.listeners.progress),
  onListingReadComplete: register(h.listeners.readComplete),
  onListingComplete: register(h.listeners.complete),
  onListingError: register(h.listeners.error),
  onListingCancelled: register(h.listeners.cancelled),
  cancelListing: h.cancelListing,
  listDirectoryEnd: h.listDirectoryEnd,
  listDirectoryStart: h.listDirectoryStart,
  findFileIndex: h.findFileIndex,
  pathExistsChecked: h.pathExistsChecked,
  resolvePathVolume: h.resolvePathVolume,
  trackEvent: h.trackEvent,
}))
vi.mock('./tag-sweep', () => ({ sweepListingTags: vi.fn() }))
vi.mock('../navigation/path-resolution', () => ({ resolveValidPath: h.resolveValidPath }))
vi.mock('$lib/error-messages/listing-error', () => ({ renderListingError: (e: unknown) => ({ rendered: e }) }))
vi.mock('$lib/icon-cache', () => ({ evictPerPathIconsForDir: vi.fn() }))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('$lib/ui/toast', () => ({ dismissTransientToastsForPane: vi.fn() }))
vi.mock('$lib/settings', () => ({ getSetting: h.getSetting }))
vi.mock('$lib/benchmark', () => ({ resetEpoch: vi.fn(), logEvent: vi.fn(), logEventValue: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import { deferred, makeHarness } from './listing-loader.test-fixtures'

/** The listingId the factory generated for the most recent (Nth) registered listener set. */
function completeCb(n: number) {
  return h.listeners.complete[n]
}

beforeEach(() => {
  vi.clearAllMocks()
  for (const b of Object.values(h.listeners)) b.length = 0
  h.listDirectoryStart.mockImplementation(
    (_v: unknown, _p: unknown, _hid: unknown, _sb: unknown, _so: unknown, listingId: string) =>
      Promise.resolve({ listingId, status: { kind: 'ok' } }),
  )
  h.findFileIndex.mockResolvedValue(0)
  h.pathExistsChecked.mockResolvedValue({ data: true, timedOut: false })
  // Default: the fallback target's owner is unresolvable, so the loader keeps the
  // pane's own volume. Tests that cross a volume boundary override this.
  h.resolvePathVolume.mockResolvedValue({ volume: null, timedOut: false })
  h.resolveValidPath.mockResolvedValue('/valid')
  h.getSetting.mockReturnValue(false) // 'listing.showTags' off → skip the tag sweep
})

describe('createListingLoader — generation / drop-foreign token model', () => {
  it('accepts the current load’s complete event and commits its listing', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    const idA = state.listingId

    completeCb(0)({ listingId: idA, totalCount: 5, volumeRoot: '/' })
    await vi.waitFor(() => {
      expect(state.totalCount).toBe(5)
    })
    expect(state.loading).toBe(false)
    expect(spies.onPathChange).toHaveBeenCalledWith('/a')
  })

  it('drops a foreign complete event once a newer load has advanced the generation', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    const idA = state.listingId
    const cbA = completeCb(0)

    await loader.loadDirectory({ path: '/b' })
    const idB = state.listingId
    expect(idB).not.toBe(idA)
    spies.onPathChange.mockClear()

    // Load A's complete arrives late — it must be inert.
    cbA({ listingId: idA, totalCount: 999, volumeRoot: '/' })
    await Promise.resolve()
    await Promise.resolve()
    expect(state.totalCount).not.toBe(999)
    expect(spies.onPathChange).not.toHaveBeenCalledWith('/a')

    // Load B's complete is accepted.
    completeCb(1)({ listingId: idB, totalCount: 7, volumeRoot: '/' })
    await vi.waitFor(() => {
      expect(state.totalCount).toBe(7)
    })
    expect(spies.onPathChange).toHaveBeenCalledWith('/b')
  })

  it('drops a complete event tagged with a foreign listingId even at the current generation', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    completeCb(0)({ listingId: 'not-the-current-id', totalCount: 999, volumeRoot: '/' })
    await Promise.resolve()
    await Promise.resolve()
    expect(state.totalCount).not.toBe(999)
    expect(spies.onPathChange).not.toHaveBeenCalled()
  })

  it('drops foreign opening / progress / read-complete / error / cancelled events after a newer load', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    const idA = state.listingId
    const opening = h.listeners.opening[0]
    const progress = h.listeners.progress[0]
    const readComplete = h.listeners.readComplete[0]
    const error = h.listeners.error[0]
    const cancelled = h.listeners.cancelled[0]

    await loader.loadDirectory({ path: '/b' })
    // Reset the state fields these would touch, so a leak is visible.
    state.openingFolder = false
    state.loadingCount = undefined
    state.finalizingCount = undefined
    spies.onMtpFatalError.mockClear()

    opening({ listingId: idA })
    progress({ listingId: idA, loadedCount: 42 })
    readComplete({ listingId: idA, totalCount: 42 })
    error({ listingId: idA, message: 'boom' })
    cancelled({ listingId: idA })
    await Promise.resolve()

    expect(state.openingFolder).toBe(false)
    expect(state.loadingCount).toBeUndefined()
    expect(state.finalizingCount).toBeUndefined()
    expect(spies.onMtpFatalError).not.toHaveBeenCalled()
  })

  it('cancels the abandoned listing when the generation advances during listDirectoryStart (post-await guard)', async () => {
    const { loader } = makeHarness()
    const startA = deferred<{ listingId: string; status: unknown }>()
    h.listDirectoryStart.mockImplementationOnce(() => startA.promise)

    const loadA = loader.loadDirectory({ path: '/a' })
    // Load A is parked at `await listDirectoryStart`. Grab its listingId from the call.
    await vi.waitFor(() => {
      expect(h.listDirectoryStart).toHaveBeenCalled()
    })
    const idA = String(h.listDirectoryStart.mock.calls[0][5])

    // Supersede via adoptListing: it advances the generation but does NOT itself
    // cancel idA, so the post-await guard is the ONLY thing that can cancel it.
    // (Deleting the `thisGeneration !== loadGeneration` post-await check fails this.)
    loader.adoptListing({
      currentPath: '/x',
      listingId: 'swapped',
      totalCount: 0,
      cursorIndex: 0,
      selectedIndices: [],
      lastSequence: 0,
    })

    // Now let A's listDirectoryStart resolve — the post-await guard must cancel idA.
    startA.resolve({ listingId: idA, status: { kind: 'ok' } })
    await loadA

    expect(h.cancelListing).toHaveBeenCalledWith(idA)
  })
})

describe('createListingLoader — async-tail behavior lock (deliberately unguarded)', () => {
  it('runs the onListingError pathExistsChecked tail even after a newer load started', async () => {
    // Behavior-lock, NOT a correctness assertion: the error tail is intentionally
    // not re-guarded on generation. If this ever changes, revisit the extraction.
    const { loader, state, spies } = makeHarness()
    const existsA = deferred<{ data: boolean; timedOut: boolean }>()
    h.pathExistsChecked.mockReturnValueOnce(existsA.promise)

    await loader.loadDirectory({ path: '/a' })
    const idA = state.listingId
    const errorCbA = h.listeners.error[0]

    // Fire A's error while A is still current — its tail starts and awaits pathExistsChecked.
    errorCbA({ listingId: idA, message: 'gone', error: { code: 'x' } })
    await Promise.resolve()

    // A newer load supersedes A.
    await loader.loadDirectory({ path: '/b' })
    spies.onPathChange.mockClear()

    // The path "exists", so the tail shows the original error + pushes history — unguarded.
    existsA.resolve({ data: true, timedOut: false })
    await vi.waitFor(() => {
      expect(spies.onPathChange).toHaveBeenCalledWith('/a')
    })
  })
})
